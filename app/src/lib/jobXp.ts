/**
 * Job xp planner (Crafting « XP métiers »): the cheapest crafts to bring a job from its current xp to a target level.
 *
 * Xp of one craft (DofusDB's model, src of dofusdb.fr/en/tools/jobs-xp): 20 × recipe level / ((job level − recipe level)^1.1 / 10
 * + 1) × ratio % (item's craftXpRatio, else its type's, else 100: pack crafts.json) × bonus %, ×3 on an epic server, nothing
 * once the job is 100 levels above the recipe; the job level is the one before each craft. A recipe needs a job level at
 * least its own. A level's total xp is L × (L − 1) × 10 (checked on the game's JobExperience floors).
 *
 * Plan: craft by craft from the start xp: wished crafts (`forced`) first as soon as their level allows (earliest = most xp),
 * else the recipe with the lowest cost per xp at the current level, as many as reach the next level (its xp per craft is
 * fixed within a level). Wished crafts not done when the target is reached are still made at the end.
 */
import type { Recipes } from "./craft";
import { priceOf, type StoredPrice } from "./price";

export const MAX_JOB_LEVEL = 200;
/** Xp bonuses offered (Almanax, events...): % of the base xp. */
export const XP_COEFS = [100, 125, 150, 200, 225, 250, 300, 375, 400, 450, 600];

/** jobs.rs `catalog`: crafting jobs (id -> name) and crafts (result gid -> [job, recipe level, xp ratio %]). */
export interface JobCatalog {
  jobs: Record<string, string>;
  crafts: Record<string, [number, number, number]>;
}
/** jobs.rs `levels`: the jobs of each character seen logging in (0 = before any login was seen). */
export interface JobLevels {
  current: number;
  characters: Record<string, Record<string, { level: number; xp: number }>>;
}

export const levelFloor = (level: number) => level * (level - 1) * 10;
export const levelOf = (xp: number) => Math.max(1, Math.min(MAX_JOB_LEVEL, Math.floor((Math.sqrt(1 + 0.4 * xp) + 1) / 2)));

/** Xp of one craft of a recipe at a job level (null: the job is below the recipe's level). */
export function craftXp(recipeLevel: number, ratio: number, jobLevel: number, coef: number, epic: boolean): number | null {
  if (jobLevel < recipeLevel) return null;
  if (jobLevel - 100 > recipeLevel) return 0;
  const base = (20 * recipeLevel * (epic ? 3 : 1)) / ((jobLevel - recipeLevel) ** 1.1 / 10 + 1);
  return Math.floor((base * ratio * coef) / 10000);
}

/** What one unit costs and what it takes: resources bought, intermediate items crafted (gid -> quantity per unit). */
export interface Bill {
  unit: number | null;
  buy: Map<number, number>;
  craft: Map<number, number>;
}

/**
 * Costs of crafting items: each ingredient bought, or crafted when that is cheaper and allowed (`canCraft`: a job the player
 * has, at the recipe's level). Memoized: one instance per (recipes, prices, allowed jobs).
 */
export class Costs {
  private memo = new Map<number, Bill>();
  private made = new Map<number, Bill>();
  private busy = new Set<number>();
  constructor(
    private recipes: Recipes,
    private prices: Map<number, StoredPrice>,
    private canCraft: (gid: number) => boolean,
  ) {}

  /** One unit crafted from its recipe (the item crafted for the job xp: always crafted). */
  crafted(gid: number): Bill {
    const hit = this.made.get(gid);
    if (hit) return hit;
    const r = this.recipes.recipes[gid];
    const bill: Bill = { unit: 0, buy: new Map(), craft: new Map() };
    if (!r) return { ...bill, unit: null };
    this.busy.add(gid);
    for (const [g, q] of r) {
      const b = this.best(g);
      if (b.unit === null) bill.unit = null;
      else if (bill.unit !== null) bill.unit += b.unit * q;
      add(bill.buy, b.buy, q);
      add(bill.craft, b.craft, q);
    }
    this.busy.delete(gid);
    this.made.set(gid, bill);
    return bill;
  }

  /** The cheapest way to get one unit: bought, or crafted (allowed, every ingredient priced, cheaper). */
  best(gid: number): Bill {
    const hit = this.memo.get(gid);
    if (hit) return hit;
    const buy = priceOf(this.prices, gid);
    let out: Bill = { unit: buy, buy: new Map([[gid, 1]]), craft: new Map() };
    if (this.recipes.recipes[gid] && this.canCraft(gid) && !this.busy.has(gid)) {
      const c = this.crafted(gid);
      if (c.unit !== null && (buy === null || c.unit < buy)) out = { unit: c.unit, buy: c.buy, craft: new Map([...c.craft, [gid, (c.craft.get(gid) ?? 0) + 1]]) };
    }
    this.memo.set(gid, out);
    return out;
  }
}

function add(into: Map<number, number>, from: Map<number, number>, times: number) {
  for (const [g, q] of from) into.set(g, (into.get(g) ?? 0) + q * times);
}

/** A recipe of the job (`crafts` entry). */
export interface JobRecipe {
  gid: number;
  level: number;
  ratio: number;
}

export interface PlanInput {
  recipes: JobRecipe[];
  /** cost of one craft of a recipe (null: an ingredient has no price) */
  cost: (gid: number) => number | null;
  startXp: number;
  target: number;
  /** wished crafts: gid -> quantity */
  forced: Map<number, number>;
  excluded: Set<number>;
  coef: number;
  epic: boolean;
}

/** Crafts of one recipe in the plan. */
export interface PlanRow {
  gid: number;
  level: number;
  qty: number;
  xp: number;
  /** total cost (null: no price) */
  cost: number | null;
  /** job levels it was crafted at */
  from: number;
  to: number;
  forced: boolean;
}

export interface Plan {
  rows: PlanRow[];
  cost: number;
  /** some wished craft has no price: `cost` leaves it out */
  unpriced: boolean;
  endXp: number;
  /** the target could not be reached: no priced recipe at that level */
  stuckAt: number | null;
}

export function plan(p: PlanInput): Plan {
  const rows = new Map<number, PlanRow>();
  const left = new Map(p.forced);
  const byGid = new Map(p.recipes.map((r) => [r.gid, r]));
  const goal = levelFloor(Math.min(MAX_JOB_LEVEL, p.target));
  let xp = Math.max(0, p.startXp);
  let stuckAt: number | null = null;

  const craft = (r: JobRecipe, n: number, level: number, each: number, forced: boolean) => {
    const c = p.cost(r.gid);
    const row = rows.get(r.gid) ?? { gid: r.gid, level: r.level, qty: 0, xp: 0, cost: 0, from: level, to: level, forced };
    row.qty += n;
    row.xp += n * each;
    row.cost = row.cost === null || c === null ? null : row.cost + c * n;
    row.to = level;
    row.forced ||= forced;
    rows.set(r.gid, row);
    xp += n * each;
  };
  const wished = (level: number) => [...left].find(([g]) => (byGid.get(g)?.level ?? Infinity) <= level);

  for (let guard = 0; xp < goal && guard < 1_000_000; guard++) {
    const level = levelOf(xp);
    const w = wished(level);
    if (w) {
      const r = byGid.get(w[0])!;
      craft(r, 1, level, craftXp(r.level, r.ratio, level, p.coef, p.epic) ?? 0, true);
      w[1] > 1 ? left.set(w[0], w[1] - 1) : left.delete(w[0]);
      continue;
    }
    // the lowest cost per xp at this level
    let best: { r: JobRecipe; each: number; per: number } | null = null;
    for (const r of p.recipes) {
      if (r.level > level || p.excluded.has(r.gid)) continue;
      const each = craftXp(r.level, r.ratio, level, p.coef, p.epic);
      const c = p.cost(r.gid);
      if (!each || c === null) continue;
      const per = c / each;
      if (!best || per < best.per) best = { r, each, per };
    }
    if (!best) {
      stuckAt = level;
      break;
    }
    const next = Math.min(goal, level < MAX_JOB_LEVEL ? levelFloor(level + 1) : goal);
    craft(best.r, Math.max(1, Math.ceil((next - xp) / best.each)), level, best.each, false);
  }
  // wished crafts not reached yet: made at the end
  for (const [g, n] of left) {
    const r = byGid.get(g);
    if (!r) continue;
    for (let i = 0; i < n; i++) {
      const level = levelOf(xp);
      craft(r, 1, level, craftXp(r.level, r.ratio, level, p.coef, p.epic) ?? 0, true);
    }
  }
  const list = [...rows.values()].sort((a, b) => a.from - b.from || a.level - b.level);
  return {
    rows: list,
    cost: list.reduce((s, r) => s + (r.cost ?? 0), 0),
    unpriced: list.some((r) => r.cost === null),
    endXp: xp,
    stuckAt,
  };
}

/** Everything the plan takes: resources to buy and items to craft on the way (gid -> quantity). */
export function shopping(rows: PlanRow[], costs: Costs): { buy: Map<number, number>; craft: Map<number, number> } {
  const buy = new Map<number, number>(), craft = new Map<number, number>();
  for (const r of rows) {
    const b = costs.crafted(r.gid);
    add(buy, b.buy, r.qty);
    add(craft, b.craft, r.qty);
  }
  return { buy, craft };
}
