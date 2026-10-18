import { groupRank, isNegative } from "./effects";

/** One stat of the comparison: the lot's effect (left), the pinned lot's effect (right) and the difference between them. */
export interface Pair {
  key: number;
  left?: any;
  right?: any;
  /** left - right, in signed values (a negative effect counts negative); effects marked "removed" count as absent. */
  delta: number;
  group: string;
  order: number;
  /** 0 = added (on either lot), 1 = normal, 2 = default effect missing on the lot. */
  phase: number;
}

const keyOf = (e: any): number => e.stat ?? -e.action;
const signed = (e: any): number => (e.status === "removed" || e.hi == null ? 0 : isNegative(e) ? -e.hi : e.hi);

function bucket(row: any): Map<number, any[]> {
  const m = new Map<number, any[]>();
  for (const e of row.effects ?? []) {
    const k = keyOf(e);
    m.set(k, [...(m.get(k) ?? []), e]);
  }
  return m;
}

/**
 * Align the effects of a lot with the pinned lot, stat by stat ("+Force" and "-Force" are one stat), so they can be shown
 * side by side with the difference in between. Works across different items. Ordered like an item's own effects.
 */
export function pairRows(pin: any, row: any): Pair[] {
  const L = bucket(row);
  const R = bucket(pin);
  const sum = (es?: any[]) => (es ?? []).reduce((s, e) => s + signed(e), 0);
  return [...new Set([...L.keys(), ...R.keys()])]
    .map((key) => {
      const left = L.get(key)?.[0];
      const right = R.get(key)?.[0];
      const ref = left ?? right;
      return {
        key,
        left,
        right,
        delta: sum(L.get(key)) - sum(R.get(key)),
        group: ref.group,
        order: ref.order ?? 0,
        phase: left?.status === "added" || right?.status === "added" ? 0 : left?.status === "removed" ? 2 : 1,
      };
    })
    .sort((a, b) => a.phase - b.phase || groupRank(a.group) - groupRank(b.group) || a.order - b.order);
}

/** "+12" / "−20" (typographic minus), "=" when equal. */
export const fmtDelta = (d: number) => (Math.abs(d) < 1e-9 ? "=" : `${d > 0 ? "+" : "−"}${Math.abs(d).toLocaleString("fr-FR")}`);
