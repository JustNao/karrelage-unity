/** Transcendance opportunities: unlocked lots that gain a rune effect worth more than the rune costs. */
import { analyse } from "./density";

export interface Rune { gid: number; name: string; a: number; value: number }
/** unit = price read in the auction house, avg = average of the last week (preload), override = manual. Priority: override, unit, avg. */
export interface StoredPrice { gid: number; unit: number | null; avg?: number | null; override: number | null; updated?: number | null }

/** A captured rune price older than this is flagged (a manual override never is). */
export const STALE_SECS = 24 * 3600;
export const targetKey = (t: { a: number; value: number }) => `${t.a}:${t.value}`;

/** Sale tax taken by the auction house on the selling price. */
export const AH_TAX = 0.02;

/** A lot whose effect was added by a rune: carries the transcendance lock and an effect the item does not have by default. */
export const isTranscended = (r: any) => r.locked && r.effects.some((e: any) => e.status === "added");
/** Overmax (above the default max) lots cannot take a rune; neither can locked ones. */
export const canTranscend = (r: any) => !r.locked && !r.effects.some((e: any) => e.status === "above" || e.status === "added");

export interface Target {
  a: number;
  value: number;
  /** Cheapest transcended lot with this effect (what the finished item would compete with). */
  sale: number;
  lots: number;
  rune: Rune | null;
  runeCost: number | null;
  /** Captured rune price older than a day. */
  stale: boolean;
  /** Only the weekly average is known (never read in the auction house). */
  avgOnly: boolean;
}

export interface Opportunity {
  row: any;
  buy: number;
  /** How far the base lot sits above the item's price/density trend (>0 = more density than its price suggests). */
  z: number;
  best: { target: Target; profit: number } | null;
  /** Targets whose rune has no stored price yet (cannot be ranked). */
  unpriced: Target[];
}

/**
 * `unit` gives a lot's unit price. Targets = every (added effect, value) seen on the transcended lots of this listing.
 * Profit = sale * (1 - tax) - buy price - rune price.
 */
export function transcendance(rows: any[], unit: (r: any) => number, runes: Rune[], prices: Map<number, StoredPrice>, focus: string | null = null, now = Date.now() / 1000) {
  const runeFor = (a: number, v: number) => runes.find((r) => r.a === a && r.value === v) ?? null;
  const isStale = (gid: number) => {
    const p = prices.get(gid);
    return !!p && p.override == null && p.unit != null && now - (p.updated ?? 0) > STALE_SECS;
  };
  const isAvgOnly = (gid: number) => {
    const p = prices.get(gid);
    return !!p && p.override == null && p.unit == null && p.avg != null;
  };
  const price = (gid: number) => {
    const p = prices.get(gid);
    return p ? (p.override ?? p.unit ?? p.avg ?? null) : null;
  };

  const targets = new Map<string, Target>();
  for (const r of rows.filter(isTranscended)) {
    const u = unit(r);
    if (!Number.isFinite(u)) continue;
    for (const e of r.effects.filter((e: any) => e.status === "added" && e.hi != null)) {
      const k = `${e.action}:${e.hi}`;
      const t = targets.get(k);
      if (t) {
        t.sale = Math.min(t.sale, u);
        t.lots++;
      } else {
        const rune = runeFor(e.action, e.hi);
        targets.set(k, { a: e.action, value: e.hi, sale: u, lots: 1, rune, runeCost: rune ? price(rune.gid) : null, stale: rune ? isStale(rune.gid) : false, avgOnly: rune ? isAvgOnly(rune.gid) : false });
      }
    }
  }

  // density trend over the lots that can take a rune and are for sale
  const base = rows.filter((r) => canTranscend(r) && Number.isFinite(unit(r)));
  const fit = base.length >= 3 ? analyse(base.map((r) => ({ x: unit(r), y: r.density }))) : null;
  const list: Opportunity[] = base.map((row, i) => {
    const buy = unit(row);
    let best: Opportunity["best"] = null;
    const unpriced: Target[] = [];
    for (const t of targets.values()) {
      if (focus && targetKey(t) !== focus) continue;
      if (row.effects.some((e: any) => e.action === t.a && e.status !== "removed")) continue; // already has that effect
      if (t.runeCost == null) {
        unpriced.push(t);
        continue;
      }
      const profit = Math.round(t.sale * (1 - AH_TAX) - buy - t.runeCost);
      if (!best || profit > best.profit) best = { target: t, profit };
    }
    return { row, buy, z: fit?.z[i] ?? 0, best, unpriced };
  });
  // cheapest way to make each transcended effect yourself: cheapest candidate lot + rune (ignores the focus)
  const make = new Map<string, { cost: number; buy: number; runeCost: number; rune: Rune }>();
  for (const t of targets.values()) {
    if (t.runeCost == null || !t.rune) continue;
    const cheapest = Math.min(...base.filter((r) => !r.effects.some((e: any) => e.action === t.a && e.status !== "removed")).map(unit));
    if (Number.isFinite(cheapest)) make.set(targetKey(t), { cost: cheapest + t.runeCost, buy: cheapest, runeCost: t.runeCost, rune: t.rune });
  }
  /** For a transcended lot: the cheaper way to get a similar item (an unlocked lot + the rune), if any. */
  const alternative = (row: any) => {
    if (!isTranscended(row)) return null;
    const price = unit(row);
    let best: { cost: number; buy: number; runeCost: number; rune: Rune } | null = null;
    for (const e of row.effects.filter((e: any) => e.status === "added" && e.hi != null)) {
      const m = make.get(`${e.action}:${e.hi}`);
      if (m && m.cost < price && (!best || m.cost < best.cost)) best = m;
    }
    return best && { ...best, saving: Math.round(price - best.cost) };
  };
  return {
    alternative,
    targets: [...targets.values()],
    opportunities: list.filter((o) => o.best && o.best.profit > 0).sort((a, b) => b.best!.profit - a.best!.profit),
    unpricedCount: list.filter((o) => !o.best && o.unpriced.length).length,
  };
}
