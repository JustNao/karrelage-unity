/**
 * Brisage: breaking an equipment piece into forgemagie runes (Brisage tab).
 *
 * Runes per effect (community formula, as used by kamamaster.fr), for an effect of value `v` on an item of level `L`:
 *   weight = round2(3 x unit x v x L / 200 + 1) x coef / 100,  runes = weight / rune weight
 * `unit` = the effect's weight per point (effectPowerRate: Force 1, Sagesse 3, PA 100, Vitalité 0.2...), rune weight =
 * unit x what the base rune adds (Rune Fo 1, Rune Vi 5 x 0.2 = 1, Rune Pod 10 x 0.25 = 2.5, Rune Ga Pa 100).
 * Focus on an effect: only its runes come out, its own plus half the weight of every other effect, converted into it.
 * An exo effect (added by forgemagie, not a default effect of the item) gives its runes but cannot be focused.
 * Fractions are kept (an expected value): the coefficient scales everything, so value(coef) = coef / 100 x value(100 %).
 */
import { afterTax } from "./price";

export interface BrisRune {
  gid: number;
  name: string;
  /** What the rune adds ("Rune Vi" = 5). */
  points: number;
  /** Weight per point of the effect (effectPowerRate). */
  unit: number;
  /** unit x points. */
  weight: number;
  label: string | null;
}

export interface BrisItem {
  gid: number;
  name: string;
  level: number;
  type_id: number;
  type: string;
  /** Dropped by some monster. */
  drop: boolean;
  craft: boolean;
  /** Default effects that yield a rune. */
  effects: { a: number; min: number; max: number }[];
}

export interface BrisPack {
  items: BrisItem[];
  /** effect id -> its base rune */
  runes: Record<string, BrisRune>;
}

/** One effect of the item being broken. */
export interface BrisEffect { a: number; value: number; /** Not a default effect of the item: cannot be focused. */ exo?: boolean }

export interface RuneLine {
  a: number;
  rune: BrisRune;
  value: number;
  /** Runes without focus, at 100 %. */
  count: number;
  /** Runes when focusing this effect, at 100 % (0 for an exo: it cannot be focused). */
  focus: number;
  exo: boolean;
  /** Rune price (null = unknown). */
  price: number | null;
}

export interface Outcome {
  lines: RuneLine[];
  /** Kamas at 100 % without focus (runes sold, after the tax; unknown prices count 0). */
  plain: number;
  /** Kamas at 100 % focusing each effect (same order as `lines`; 0 for an exo). */
  focus: number[];
  /** The best of `plain` and every focus: which (null = no focus) and its kamas at 100 %. */
  best: { focus: number | null; value: number };
  /** Some rune worth something here has no price. */
  unpriced: boolean;
}

/** Runes one effect gives at 100 % (0 for a null or negative value). */
export function runeCount(value: number, level: number, r: BrisRune): number {
  if (value <= 0) return 0;
  return Math.round((3 * r.unit * value * level / 200 + 1) * 100) / 100 / r.weight;
}

/** Breaking an item with these effects, at 100 % (same effect twice = added up). */
export function outcome(effects: BrisEffect[], level: number, runes: Record<string, BrisRune>, price: (gid: number) => number | null): Outcome {
  const by = new Map<number, RuneLine>();
  for (const e of effects) {
    const rune = runes[e.a];
    if (!rune || e.value <= 0) continue;
    const l = by.get(e.a);
    if (l) {
      l.value += e.value;
      l.count += runeCount(e.value, level, rune);
    } else by.set(e.a, { a: e.a, rune, value: e.value, count: runeCount(e.value, level, rune), focus: 0, exo: !!e.exo, price: price(rune.gid) });
  }
  const lines = [...by.values()];
  const total = lines.reduce((s, l) => s + l.count * l.rune.weight, 0);
  for (const l of lines) l.focus = l.exo ? 0 : l.count + (total - l.count * l.rune.weight) / l.rune.weight / 2;
  const net = (n: number, p: number | null) => afterTax(n * (p ?? 0));
  const plain = lines.reduce((s, l) => s + net(l.count, l.price), 0);
  const focus = lines.map((l) => net(l.focus, l.price));
  let best: Outcome["best"] = { focus: null, value: plain };
  focus.forEach((v, i) => v > best.value && (best = { focus: lines[i].a, value: v }));
  return { lines, plain, focus, best, unpriced: lines.some((l) => l.price == null) };
}

/** Coefficient (%) at which breaking pays back `cost` (null when the runes are worth nothing). */
export const breakEven = (cost: number, value100: number) => (value100 > 0 ? (cost / value100) * 100 : null);

/** Mean roll of each default effect (what a craft gives on average). */
export const meanRoll = (i: BrisItem): BrisEffect[] => i.effects.map((e) => ({ a: e.a, value: (e.min + e.max) / 2 }));

/** Effects of a scanned lot (its current values; missing default effects give nothing). */
export const lotEffects = (lot: any): BrisEffect[] =>
  (lot.effects ?? []).filter((e: any) => e.status !== "removed" && typeof e.hi === "number").map((e: any) => ({ a: e.action, value: e.hi, exo: e.status === "added" }));
