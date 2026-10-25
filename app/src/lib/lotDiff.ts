/**
 * What changed between two scans of an item's listing (the HDV "Modifications" view). Lots are matched by their exact
 * characteristics (every effect's action + value + text), not their uid: within a set of identical lots, those at the same
 * price are unchanged, the others pair up as price changes (any of the matching lots, cheapest with cheapest), and what is
 * left over was added (new scan) or removed (old one).
 */
import { unitPrice } from "./price";

export interface LotDiff {
  /** Rows of the new scan absent from the old one. */
  added: Set<any>;
  /** Rows of the old scan absent from the new one. */
  removed: any[];
  /** Row of the new scan -> its price in the old scan (identical characteristics, another price). */
  was: Map<any, number>;
}

/** The lot's characteristics as text (a base effect missing from the lot is no characteristic). */
const sig = (r: any) =>
  JSON.stringify(
    (r.effects ?? [])
      .filter((e: any) => e.status !== "removed")
      .map((e: any) => [e.action, e.hi ?? null, e.text ?? ""])
      .sort((a: any[], b: any[]) => a[0] - b[0] || (a[1] ?? -Infinity) - (b[1] ?? -Infinity) || (a[2] < b[2] ? -1 : a[2] > b[2] ? 1 : 0)),
  );

const cache = new WeakMap<object, WeakMap<object, LotDiff>>();

/** Memoized per pair of listing arrays (a scan replaces the rows whole). */
export function lotDiff(before: any[], after: any[]): LotDiff {
  let byAfter = cache.get(before);
  const hit = byAfter?.get(after);
  if (hit) return hit;
  const groups = new Map<string, { old: any[]; now: any[] }>();
  const group = (r: any) => {
    const k = sig(r);
    let g = groups.get(k);
    if (!g) groups.set(k, (g = { old: [], now: [] }));
    return g;
  };
  for (const r of before) group(r).old.push(r);
  for (const r of after) group(r).now.push(r);
  const d: LotDiff = { added: new Set(), removed: [], was: new Map() };
  const byPrice = (a: any, b: any) => unitPrice(a) - unitPrice(b) || 0;
  for (const { old, now } of groups.values()) {
    // same price: unchanged
    const left = [...old];
    const rest: any[] = [];
    for (const r of now) {
      const i = left.findIndex((o) => unitPrice(o) === unitPrice(r));
      if (i >= 0) left.splice(i, 1);
      else rest.push(r);
    }
    left.sort(byPrice);
    rest.sort(byPrice);
    const n = Math.min(left.length, rest.length);
    for (let i = 0; i < n; i++) d.was.set(rest[i], unitPrice(left[i]));
    for (const r of rest.slice(n)) d.added.add(r);
    d.removed.push(...left.slice(n));
  }
  if (!byAfter) cache.set(before, (byAfter = new WeakMap()));
  byAfter.set(after, d);
  return d;
}
