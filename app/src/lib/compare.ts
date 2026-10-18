import { groupRank, isNegative } from "./effects";

export interface Chip {
  text: string;
  /** up = better than the pinned lot, down = worse. */
  tone: "up" | "down";
}

interface StatValue {
  value: number;
  fallback: string;
  group: string;
  order: number;
}

/** Signed value per stat of a lot ("+Force" and "-Force" share a stat; negative effects count negative). */
function byStat(row: any): Map<number, StatValue> {
  const m = new Map<number, StatValue>();
  for (const e of row.effects ?? []) {
    if (e.status === "removed" || e.hi == null) continue;
    const key = e.stat ?? -e.action;
    const v = isNegative(e) ? -e.hi : e.hi;
    const cur = m.get(key);
    if (cur) cur.value += v;
    else m.set(key, { value: v, fallback: String(e.text).replace(/^[-+]?[\d\s]+(à [\d\s]+)?/, "").trim(), group: e.group, order: e.order ?? 0 });
  }
  return m;
}

const fmt = (n: number) => Math.abs(n).toLocaleString("fr-FR");

/**
 * What `row` has more (+) or less (-) than the pinned lot, stat by stat, then the price. Works across different items.
 * `label` resolves a stat id to its positive effect name (e.g. "Force").
 */
export function diffAgainst(pin: any, row: any, label: (stat: number, fallback: string) => string, unitPrice: (r: any) => number): Chip[] {
  const a = byStat(pin);
  const b = byStat(row);
  const stats = [...new Set([...a.keys(), ...b.keys()])];
  const lines = stats
    .map((s) => {
      const delta = (b.get(s)?.value ?? 0) - (a.get(s)?.value ?? 0);
      const src = b.get(s) ?? a.get(s)!;
      return { delta, src, text: label(s, src.fallback) };
    })
    .filter((l) => Math.abs(l.delta) > 1e-9)
    .sort((x, y) => groupRank(x.src.group) - groupRank(y.src.group) || x.src.order - y.src.order);
  const chips: Chip[] = lines.map((l) => ({ text: `${l.delta > 0 ? "+" : "−"}${fmt(l.delta)} ${l.text}`, tone: l.delta > 0 ? "up" : "down" }));
  const [pp, rp] = [unitPrice(pin), unitPrice(row)];
  if (Number.isFinite(pp) && Number.isFinite(rp) && pp !== rp) chips.push({ text: `Price ${rp > pp ? "+" : "−"}${fmt(rp - pp)} kamas`, tone: rp < pp ? "up" : "down" });
  return chips;
}
