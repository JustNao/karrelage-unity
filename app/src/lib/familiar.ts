/**
 * Familiar / montilier levelling: the xp each resource gives (community sheet via huzounet.fr, by item id) and the cheapest
 * way to feed a given xp. Feeding n units of a resource gives floor(n × its xp per unit).
 */
import { call } from "./api";
import { priceOf, type StoredPrice } from "./price";
import { craftCost, type Recipes } from "./craft";
import table from "../modules/crafting/subs/familiers/familiar_xp.json";

export interface Food { gid: number; xp: number }

/** Every resource with a known xp per unit. */
export const FOODS: Food[] = (table.foods as [number, number][]).map(([gid, xp]) => ({ gid, xp }));

/** Total xp of each level (the game's table: level L = LEVELS[L - 1], level 100 the last). */
const LEVELS: number[] = table.levels;
export const MAX_LEVEL = LEVELS.length;

/** Total xp of a level (clamped to 1..100). */
export const levelBase = (l: number) => LEVELS[Math.min(Math.max(Math.round(l), 1), MAX_LEVEL) - 1];

/** Units of a food needed for `need` xp (the batch's xp is rounded down). */
export function unitsFor(need: number, xp: number) {
  if (need <= 0) return 0;
  let n = Math.max(1, Math.ceil(need / xp - 1e-9));
  while (Math.floor(n * xp + 1e-9) < need) n++;
  return n;
}
/** Xp of a batch of `n` units. */
export const batchXp = (n: number, xp: number) => Math.floor(n * xp + 1e-9);

let namesPromise: Promise<Record<string, string>> | null = null;
/** Food gid -> item name (cached; from the data pack). */
export const foodNames = () =>
  (namesPromise ??= call<Record<string, string>>("prices", "item_names", { gids: FOODS.map((f) => f.gid) }).catch((e) => ((namesPromise = null), Promise.reject(e))));

export interface FoodCost {
  /** Cheaper of the two (null = neither known). */
  unit: number | null;
  how: "achat" | "craft" | null;
  buy: number | null;
  /** Craft cost from the stored prices (ingredients bought or crafted, whichever is cheaper); null = no recipe or a price missing. */
  craft: number | null;
}

/** Unit cost of every food: bought, or crafted when that is cheaper. */
export function foodCosts(recipes: Recipes, prices: Map<number, StoredPrice>) {
  const out = new Map<number, FoodCost>();
  for (const f of FOODS) {
    const buy = priceOf(prices, f.gid);
    const craft = recipes.recipes[f.gid] ? (craftCost(f.gid, recipes, prices)?.cost ?? null) : null;
    const how = craft != null && (buy == null || craft < buy) ? "craft" : buy != null ? "achat" : null;
    out.set(f.gid, { unit: how === "craft" ? craft : buy, how, buy, craft });
  }
  return out;
}

export interface Feed { gid: number; units: number; xp: number; cost: number; how: "achat" | "craft" }
export interface FeedPlan { cost: number; xp: number; feeds: Feed[] }

/**
 * Cheapest known way to give `need` xp: the best foods (by cost per xp) are tried as the bulk, the remainder is covered by
 * the cheapest single food that reaches it. Foods without a cost are ignored. null = no priced food.
 */
export function feeder(costs: Map<number, FoodCost>) {
  const foods = FOODS.flatMap((f) => {
    const c = costs.get(f.gid);
    return c?.unit != null && f.xp > 0 ? [{ ...f, unit: c.unit, how: c.how! }] : [];
  }).sort((a, b) => a.unit / a.xp - b.unit / b.xp);
  const top = foods.slice(0, 40); // the bulk food is always among the best per xp
  type F = (typeof foods)[number];
  const feed = (f: F, units: number): Feed => ({ gid: f.gid, units, xp: batchXp(units, f.xp), cost: units * f.unit, how: f.how });
  const cover = (rem: number) => {
    let best: Feed | null = null;
    for (const f of foods) {
      const x = feed(f, unitsFor(rem, f.xp));
      if (!best || x.cost < best.cost) best = x;
    }
    return best!;
  };
  const memo = new Map<number, FeedPlan | null>();
  const plan = (need: number): FeedPlan | null => {
    if (need <= 0) return { cost: 0, xp: 0, feeds: [] };
    if (!foods.length) return null;
    if (memo.has(need)) return memo.get(need)!;
    let best: FeedPlan | null = null;
    for (const f of top) {
      const n = Math.floor(need / f.xp);
      const bulk = n ? feed(f, n) : null;
      const rem = need - (bulk?.xp ?? 0);
      const feeds = bulk ? [bulk] : [];
      if (rem > 0) {
        const c = cover(rem);
        if (bulk && c.gid === bulk.gid) feeds[0] = feed(f, unitsFor(need, f.xp)); // one batch of the same food
        else feeds.push(c);
      }
      const cost = feeds.reduce((s, x) => s + x.cost, 0);
      if (!best || cost < best.cost) best = { cost, xp: feeds.reduce((s, x) => s + x.xp, 0), feeds };
    }
    memo.set(need, best);
    return best;
  };
  return { plan, priced: foods.length };
}
