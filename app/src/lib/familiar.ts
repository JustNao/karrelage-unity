/** Familiar / mount levelling: the resources that give xp (dofustool.com table + croquette enrichie) and the cheapest way to feed a given xp. */
import { call } from "./api";
import { priceOf } from "./price";
import { xpOfLevel } from "./companion";
import type { StoredPrice } from "./transcend";
import table from "../modules/crafting/subs/familiers/familiar_xp.json";

export interface Food { name: string; xp: number; qty: number }

/** Croquette enrichie: a flat 500 xp (not in the site's table). */
const CROQUETTE: Food = { name: "Croquette enrichie", xp: 500, qty: 1 };

/** [name, xp per meal, resource quantity per meal]; the site lists some resources twice: keep the best xp per unit. */
export const FOODS: Food[] = (() => {
  const by = new Map<string, Food>();
  for (const f of [CROQUETTE, ...(table.foods as [string, number, number][]).map(([name, xp, qty]) => ({ name, xp, qty }))]) {
    const old = by.get(f.name);
    if (!old || f.xp / f.qty > old.xp / old.qty) by.set(f.name, f);
  }
  return [...by.values()];
})();

/** Total xp of a level; level 1 starts at 0. */
export const levelBase = (l: number) => (l <= 1 ? 0 : xpOfLevel(Math.min(l, 100)));

let gidsPromise: Promise<Record<string, number>> | null = null;
/** Food name -> item gid (cached; the backend resolves the names against the game data). */
export const foodGids = () => (gidsPromise ??= call<Record<string, number>>("prices", "gids_by_name", { names: FOODS.map((f) => f.name) }).catch((e) => ((gidsPromise = null), Promise.reject(e))));

export interface Feed { food: string; meals: number; units: number; cost: number }
export interface FeedPlan { cost: number; xp: number; feeds: Feed[] }

/**
 * Cheapest known way to give `need` xp: the best foods (by price per xp) are tried as the bulk, the remainder is covered by
 * the cheapest single food that reaches it. Foods without a stored price are ignored. null = no priced food.
 */
export function feeder(gids: Record<string, number>, prices: Map<number, StoredPrice>) {
  const foods = FOODS.flatMap((f) => {
    const g = gids[f.name];
    const unit = g != null ? priceOf(prices, g) : null;
    return unit != null ? [{ ...f, meal: unit * f.qty }] : [];
  }).sort((a, b) => a.meal / a.xp - b.meal / b.xp);
  const top = foods.slice(0, 40); // the bulk food is always among the best per xp
  const memo = new Map<number, FeedPlan | null>();
  const cover = (rem: number) => {
    let best: (typeof foods)[number] | null = null;
    for (const f of foods) if (!best || Math.ceil(rem / f.xp) * f.meal < Math.ceil(rem / best.xp) * best.meal) best = f;
    return best;
  };
  const plan = (need: number): FeedPlan | null => {
    if (need <= 0) return { cost: 0, xp: 0, feeds: [] };
    if (!foods.length) return null;
    if (memo.has(need)) return memo.get(need)!;
    let best: FeedPlan | null = null;
    for (const f of top) {
      const n = Math.floor(need / f.xp);
      const rem = need - n * f.xp;
      const feeds: Feed[] = n ? [{ food: f.name, meals: n, units: n * f.qty, cost: n * f.meal }] : [];
      let xp = n * f.xp;
      let cost = n * f.meal;
      if (rem > 1e-9) {
        const c = cover(rem)!;
        const m = Math.ceil(rem / c.xp);
        feeds.push({ food: c.name, meals: m, units: m * c.qty, cost: m * c.meal });
        xp += m * c.xp;
        cost += m * c.meal;
      }
      const merged: Feed[] = []; // same food for the bulk and the remainder: one line
      for (const x of feeds) {
        const m = merged.find((y) => y.food === x.food);
        if (m) (m.meals += x.meals), (m.units += x.units), (m.cost += x.cost);
        else merged.push({ ...x });
      }
      if (!best || cost < best.cost) best = { cost, xp, feeds: merged };
    }
    memo.set(need, best);
    return best;
  };
  return { plan, priced: foods.length };
}
