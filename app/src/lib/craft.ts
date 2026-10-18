/** Craft cost of an item from the stored prices, recursive: an ingredient is bought or crafted, whichever is cheaper. */
import { AH_TAX, type StoredPrice } from "./transcend";

export interface Recipes {
  /** result gid -> [[ingredient gid, quantity], ...] */
  recipes: Record<string, [number, number][]>;
  names: Record<string, string>;
}

export interface CraftLine {
  gid: number;
  name: string;
  qty: number;
  /** Unit cost used (null = unknown). */
  unit: number | null;
  how: "achat" | "craft" | null;
  /** Ingredients of that sub-craft (when crafted). */
  sub?: CraftLine[];
}

export interface CraftResult {
  /** Total cost, null while some ingredient has no price at all. */
  cost: number | null;
  lines: CraftLine[];
  missing: string[];
}

/** Stored price of an item: manual override, then the price read in the auction house, then the weekly average. */
export const storedPrice = (prices: Map<number, StoredPrice>, gid: number): number | null => {
  const p = prices.get(gid);
  return p ? (p.override ?? p.unit ?? p.avg ?? null) : null;
};

export function craftCost(gid: number, r: Recipes, prices: Map<number, StoredPrice>): CraftResult | null {
  if (!r.recipes[gid]) return null;
  const missing = new Set<string>();
  const memo = new Map<number, number | null>(); // gid -> cheapest unit cost (buy or craft)
  const busy = new Set<number>(); // cycle guard

  const best = (g: number): { unit: number | null; how: CraftLine["how"]; sub?: CraftLine[] } => {
    const buy = storedPrice(prices, g);
    let craft: number | null = null;
    let sub: CraftLine[] | undefined;
    if (r.recipes[g] && !busy.has(g)) {
      busy.add(g);
      const lines = r.recipes[g].map(([ig, q]) => line(ig, q));
      busy.delete(g);
      if (lines.every((l) => l.unit != null)) {
        craft = lines.reduce((s, l) => s + l.unit! * l.qty, 0);
        sub = lines;
      }
    }
    if (craft != null && (buy == null || craft < buy)) return { unit: craft, how: "craft", sub };
    return buy != null ? { unit: buy, how: "achat" } : { unit: null, how: null };
  };

  const line = (g: number, qty: number): CraftLine => {
    const b = best(g);
    if (b.unit == null) missing.add(r.names[g] ?? `#${g}`);
    memo.set(g, b.unit);
    return { gid: g, name: r.names[g] ?? `#${g}`, qty, unit: b.unit, how: b.how, sub: b.sub };
  };

  busy.add(gid);
  const lines = r.recipes[gid].map(([g, q]) => line(g, q));
  const cost = lines.every((l) => l.unit != null) ? lines.reduce((s, l) => s + l.unit! * l.qty, 0) : null;
  return { cost, lines, missing: [...missing] };
}

/** Profit of selling `sale` (cheapest unit price in the auction house) for a craft costing `cost`, after the AH tax. */
export function benefit(sale: number, cost: number) {
  const profit = Math.round(sale * (1 - AH_TAX) - cost);
  return { profit, pct: cost > 0 ? (profit / cost) * 100 : 0 };
}
