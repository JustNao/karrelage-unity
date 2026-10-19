/** Per-lot price helpers shared by every tab. Lots coming from the backend are never mutated (an item's listing is replaced
 *  whole on each scan), so results are memoized per lot object. */

const units = new WeakMap<object, number>();

/** Cheapest price per unit of a lot (price / qty over its priced stack sizes); Infinity when it has no price. */
export function unitPrice(row: any): number {
  let v = units.get(row);
  if (v === undefined) {
    v = Infinity;
    for (const p of row.prices ?? []) if (p.price > 0 && p.qty > 0) v = Math.min(v, p.price / p.qty);
    units.set(row, v);
  }
  return v;
}

/** Same as `unitPrice`, null instead of Infinity when the lot has no price. */
export const unitPriceOrNull = (row: any): number | null => {
  const v = unitPrice(row);
  return Number.isFinite(v) ? v : null;
};

/** Ascending by unit price, lots without a price last (whatever the direction). */
export const byUnitPrice = (dir = 1) => (p: any, q: any) => {
  const a = unitPrice(p), b = unitPrice(q);
  return a === b ? 0 : a === Infinity ? 1 : b === Infinity ? -1 : (a - b) * dir;
};

const keys = new WeakMap<object, Map<any, string>>();

/** Small stable string hash (djb2). */
const hash = (s: string) => {
  let h = 5381;
  for (let i = 0; i < s.length; i++) h = ((h << 5) + h + s.charCodeAt(i)) | 0;
  return (h >>> 0).toString(36);
};

/**
 * Stable key of every lot of a listing: its uid, else a hash of its content (prices + effects), so keys do not depend on the
 * position in a filtered / sorted list. Identical lots without a uid get a "#n" suffix (keyed lists need unique keys).
 * Memoized per listing array.
 */
export function lotKeys(rows: any[]): Map<any, string> {
  let m = keys.get(rows);
  if (m) return m;
  m = new Map();
  const seen = new Map<string, number>();
  for (const r of rows) {
    let k = r.uid != null ? String(r.uid) : "h" + hash(JSON.stringify([r.prices, (r.effects ?? []).map((e: any) => [e.action, e.hi, e.status])]));
    const n = seen.get(k) ?? 0;
    seen.set(k, n + 1);
    if (n) k += "#" + n;
    m.set(r, k);
  }
  keys.set(rows, m);
  return m;
}
