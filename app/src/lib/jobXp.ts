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

/** jobs.rs `catalog`: crafting jobs (id -> name), crafts (result gid -> [job, recipe level, xp ratio %, item category (0
 *  equipment, 1 consumable, 2 resource), item type]) and the item types' names. */
export interface JobCatalog {
  jobs: Record<string, string>;
  crafts: Record<string, [number, number, number, number, number]>;
  types?: Record<string, string>;
}
/** jobs.rs `levels`: the jobs of each character seen logging in (0 = before any login was seen). */
export interface JobLevels {
  current: number;
  characters: Record<string, Record<string, { level: number; xp: number }>>;
}

/** Job 1 « Base »: recipes made without a job (never levelled). */
export const BASE_JOB = 1;

/** The jobs of the character: the last one logged in, else the first one seen ({} = none read yet). */
export function currentJobs(levels: JobLevels): JobLevels["characters"][string] {
  const has = (k: string | number) => Object.keys(levels.characters[k] ?? {}).length > 0;
  const c = has(levels.current) ? String(levels.current) : Object.keys(levels.characters).find(has);
  return c !== undefined ? levels.characters[c] : {};
}

/** « Craftable par moi »: an item one of these jobs crafts at its level (`levelOf` overrides a job's level). */
export function craftableBy(catalog: JobCatalog, jobs: JobLevels["characters"][string], levelOf?: (job: number) => number | undefined) {
  const level = (j: number) => levelOf?.(j) ?? (j === BASE_JOB ? MAX_JOB_LEVEL : (jobs[j]?.level ?? 0));
  return (gid: number) => {
    const c = catalog.crafts[gid];
    return !!c && level(c[0]) >= c[1];
  };
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
  /** cost of one craft of a recipe, every ingredient obtained (null: an ingredient has no price) */
  cost: (gid: number) => number | null;
  startXp: number;
  target: number;
  /** wished crafts: gid -> quantity */
  forced: Map<number, number>;
  excluded: Set<number>;
  coef: number;
  epic: boolean;
  /** most crafts of one recipe the search picks over the whole plan (wished crafts aside); null = no limit. Past it the next
   *  cheapest recipe takes over: buying 10,000 times the same ingredients would drive their price up. */
  maxPerRecipe?: number | null;
  /** resources the player already has (inventory + bank: gid -> quantity), used first: a craft then costs only its missing
   *  ingredients. Needs `ingredients` and `unit`. */
  owned?: Map<number, number>;
  /** a recipe's direct ingredients ([gid, quantity per craft]) */
  ingredients?: (gid: number) => [number, number][];
  /** the cheapest way to get one unit of an ingredient (bought, or crafted: Costs.best) */
  unit?: (gid: number) => number | null;
}

/** Crafts of one recipe in a row of the plan (the rows follow the order they are made: a recipe crafted again after another
 *  one gets a new row). */
export interface PlanRow {
  gid: number;
  level: number;
  qty: number;
  xp: number;
  /** total cost: the ingredients still to get (null: no price) */
  cost: number | null;
  /** job levels it was crafted at */
  from: number;
  to: number;
  forced: boolean;
  /** direct ingredients still to get, past the owned ones ([gid, quantity]); absent = all of them (no owned resources) */
  missing?: [number, number][];
  /** owned ingredient units these crafts use */
  used?: number;
}

export interface Plan {
  rows: PlanRow[];
  cost: number;
  /** some wished craft has no price: `cost` leaves it out */
  unpriced: boolean;
  endXp: number;
  /** the target could not be reached: no priced recipe at that level (or every one at the crafts limit) */
  stuckAt: number | null;
  /** recipes the search crafted up to `maxPerRecipe` */
  capped: number;
}

/** n crafts of a recipe from the stock left: what they use of it, what is still to get and its price (null: unpriced). */
interface Batch {
  cost: number | null;
  use: Map<number, number>;
  missing: Map<number, number>;
}

/** The owned stock walked through the plan: each craft takes its ingredients from it first. */
class Stock {
  private left: Map<number, number>;
  constructor(
    private p: PlanInput,
    owned: Map<number, number>,
  ) {
    this.left = new Map(owned);
  }
  get active() {
    return !!(this.p.ingredients && this.p.unit && this.left.size);
  }
  batch(gid: number, n: number): Batch {
    if (!this.active) {
      const c = this.p.cost(gid);
      return { cost: c === null ? null : c * n, use: new Map(), missing: new Map((this.p.ingredients?.(gid) ?? []).map(([g, q]) => [g, q * n])) };
    }
    const use = new Map<number, number>(), missing = new Map<number, number>();
    let cost: number | null = 0;
    for (const [g, q] of this.p.ingredients!(gid)) {
      const need = q * n;
      const have = Math.min(need, this.left.get(g) ?? 0);
      if (have) use.set(g, have);
      if (need > have) {
        missing.set(g, need - have);
        const u = this.p.unit!(g);
        cost = cost === null || u === null ? null : cost + u * (need - have);
      }
    }
    return { cost, use, missing };
  }
  /** crafts of `gid` its owned ingredients still lower (0: none left) */
  covered(gid: number): number {
    if (!this.active) return 0;
    let k = 0;
    for (const [g, q] of this.p.ingredients!(gid)) k = Math.max(k, Math.ceil((this.left.get(g) ?? 0) / q));
    return k;
  }
  take(b: Batch) {
    for (const [g, q] of b.use) {
      const r = (this.left.get(g) ?? 0) - q;
      r > 0 ? this.left.set(g, r) : this.left.delete(g);
    }
  }
}

/** Adds crafts to the rows (a row per run of the same recipe), with what they cost from the stock left. */
function rowsBuilder(stock: Stock, track: boolean) {
  const rows: PlanRow[] = [];
  const craft = (gid: number, recipeLevel: number, n: number, level: number, each: number, forced: boolean, b = stock.batch(gid, n)) => {
    stock.take(b);
    let row = rows[rows.length - 1];
    if (row?.gid !== gid || row.forced !== forced) rows.push((row = { gid, level: recipeLevel, qty: 0, xp: 0, cost: 0, from: level, to: level, forced, missing: [], used: 0 }));
    row.qty += n;
    row.xp += n * each;
    row.cost = row.cost === null || b.cost === null ? null : row.cost + b.cost;
    row.to = level;
    if (track) {
      const m = new Map(row.missing);
      for (const [g, q] of b.missing) m.set(g, (m.get(g) ?? 0) + q);
      row.missing = [...m];
    } else delete row.missing;
    row.used! += [...b.use.values()].reduce((s, q) => s + q, 0);
    return n * each;
  };
  return { rows, craft };
}

function summary(rows: PlanRow[], endXp: number, stuckAt: number | null, capped: number): Plan {
  return { rows, cost: rows.reduce((s, r) => s + (r.cost ?? 0), 0), unpriced: rows.some((r) => r.cost === null), endXp, stuckAt, capped };
}

export function plan(p: PlanInput): Plan {
  const stock = new Stock(p, p.owned ?? new Map());
  const { rows, craft } = rowsBuilder(stock, !!p.ingredients);
  const left = new Map(p.forced);
  const byGid = new Map(p.recipes.map((r) => [r.gid, r]));
  const goal = levelFloor(Math.min(MAX_JOB_LEVEL, p.target));
  let xp = Math.max(0, p.startXp);
  let stuckAt: number | null = null;
  const cap = p.maxPerRecipe != null && p.maxPerRecipe > 0 ? Math.floor(p.maxPerRecipe) : Infinity;
  /** crafts the search picked per recipe (wished ones aside) */
  const used = new Map<number, number>();
  const wished = (level: number) => [...left].find(([g]) => (byGid.get(g)?.level ?? Infinity) <= level);

  for (let guard = 0; xp < goal && guard < 1_000_000; guard++) {
    const level = levelOf(xp);
    const w = wished(level);
    if (w) {
      const r = byGid.get(w[0])!;
      xp += craft(r.gid, r.level, 1, level, craftXp(r.level, r.ratio, level, p.coef, p.epic) ?? 0, true);
      w[1] > 1 ? left.set(w[0], w[1] - 1) : left.delete(w[0]);
      continue;
    }
    const next = Math.min(goal, level < MAX_JOB_LEVEL ? levelFloor(level + 1) : goal);
    // the lowest cost per xp at this level; a recipe whose owned ingredients lower it is priced over the crafts they cover
    let best: { r: JobRecipe; each: number; per: number; n: number; b: Batch } | null = null;
    for (const r of p.recipes) {
      const done = used.get(r.gid) ?? 0;
      if (r.level > level || p.excluded.has(r.gid) || done >= cap) continue;
      const each = craftXp(r.level, r.ratio, level, p.coef, p.epic);
      if (!each) continue;
      const needed = Math.max(1, Math.ceil((next - xp) / each));
      const covered = stock.covered(r.gid);
      const n = Math.min(cap - done, covered ? Math.min(needed, covered) : needed);
      const b = stock.batch(r.gid, n);
      if (b.cost === null) continue;
      const per = b.cost / (n * each);
      if (!best || per < best.per) best = { r, each, per, n, b };
    }
    if (!best) {
      stuckAt = level;
      break;
    }
    used.set(best.r.gid, (used.get(best.r.gid) ?? 0) + best.n);
    xp += craft(best.r.gid, best.r.level, best.n, level, best.each, false, best.b);
  }
  // wished crafts not reached yet: made at the end
  for (const [g, n] of left) {
    const r = byGid.get(g);
    if (!r) continue;
    for (let i = 0; i < n; i++) {
      const level = levelOf(xp);
      xp += craft(r.gid, r.level, 1, level, craftXp(r.level, r.ratio, level, p.coef, p.epic) ?? 0, true);
    }
  }
  return summary(rows, xp, stuckAt, [...used.values()].filter((n) => n >= cap).length);
}

/** A frozen plan's crafts (same recipes, quantities and order), priced again: today's prices and owned stock, the xp
 *  computed again from `startXp`. */
export function reprice(frozen: Pick<PlanRow, "gid" | "level" | "qty" | "forced">[], p: PlanInput): Plan {
  const stock = new Stock(p, p.owned ?? new Map());
  const { rows, craft } = rowsBuilder(stock, !!p.ingredients);
  const byGid = new Map(p.recipes.map((r) => [r.gid, r]));
  let xp = Math.max(0, p.startXp);
  for (const f of frozen) {
    const r = byGid.get(f.gid) ?? { gid: f.gid, level: f.level, ratio: 100 };
    // xp per craft at the level reached, a level at a time (as the plan made them)
    let n = f.qty;
    while (n > 0) {
      const level = levelOf(xp);
      const each = craftXp(r.level, r.ratio, level, p.coef, p.epic) ?? 0;
      const toNext = level < MAX_JOB_LEVEL && each ? Math.max(1, Math.ceil((levelFloor(level + 1) - xp) / each)) : n;
      const k = Math.min(n, toNext);
      xp += craft(r.gid, r.level, k, level, each, f.forced);
      n -= k;
    }
  }
  return summary(rows, xp, null, 0);
}

/** Everything the plan takes: resources to buy and items to craft on the way (gid -> quantity), owned ones left out. */
export function shopping(rows: PlanRow[], costs: Costs): { buy: Map<number, number>; craft: Map<number, number> } {
  const buy = new Map<number, number>(), craft = new Map<number, number>();
  for (const r of rows) {
    if (r.missing) {
      for (const [g, q] of r.missing) {
        const b = costs.best(g);
        add(buy, b.buy, q);
        add(craft, b.craft, q);
      }
      continue;
    }
    const b = costs.crafted(r.gid);
    add(buy, b.buy, r.qty);
    add(craft, b.craft, r.qty);
  }
  return { buy, craft };
}
