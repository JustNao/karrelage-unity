import { HIDDEN_EFFECTS, isNegative } from "./effects";
import { unitPrice } from "./price";

/**
 * Selling price of one of the player's equipment pieces against the lots on sale (HDV > Vente):
 *  - with EXO effects: only the lots having each of them at least as high ("exo");
 *  - else with over-max effects: only the lots having each of them at least as high ("over");
 *  - else, or when no lot qualifies: every lot ("density").
 * Among those, the lots at least as dense set the price: UNDERCUT under the cheapest of them, so no denser lot is cheaper.
 * None denser: the piece is the best of its kind, priced like the most expensive lot compared (`best`).
 */
export const UNDERCUT = 1000;

export type SellBasis = "exo" | "over" | "density";
export interface SellAdvice {
  /** Recommended price (null: nothing on sale to compare with). */
  price: number | null;
  basis: SellBasis;
  /** The lines the lots were required to have (exo / over basis). */
  lines: { action: number; hi: number }[];
  /** EXO / over wanted but no lot on sale has it: compared on density with every lot. */
  fellBack: boolean;
  /** How many lots were compared. */
  compared: number;
  /** The lot setting the price: the cheapest at least as dense, or (best) the most expensive compared. */
  ref: any | null;
  best: boolean;
}

const lines = (r: any, status: string) =>
  (r.effects ?? []).filter((e: any) => e.status === status && !HIDDEN_EFFECTS.has(e.action) && !isNegative(e) && e.hi != null).map((e: any) => ({ action: e.action as number, hi: e.hi as number }));

/** The lot has every line, each at least as high (an EXO must still be an EXO there). */
const hasAll = (r: any, want: { action: number; hi: number }[], status?: string) =>
  want.every((w) => (r.effects ?? []).some((e: any) => e.action === w.action && e.status !== "removed" && (!status || e.status === status) && (e.hi ?? -Infinity) >= w.hi));

export function sellAdvice(piece: any, lots: any[]): SellAdvice {
  const onSale = lots.filter((r) => Number.isFinite(unitPrice(r)));
  const exo = lines(piece, "added");
  const over = exo.length ? [] : lines(piece, "above");
  const basis: SellBasis = exo.length ? "exo" : over.length ? "over" : "density";
  const want = exo.length ? exo : over;
  let pool = basis === "density" ? onSale : onSale.filter((r) => hasAll(r, want, basis === "exo" ? "added" : undefined));
  const fellBack = basis !== "density" && !pool.length;
  if (fellBack) pool = onSale;
  const d = piece.density ?? 0;
  const denser = pool.filter((r) => (r.density ?? 0) >= d).sort((a, b) => unitPrice(a) - unitPrice(b));
  const base = { basis, lines: want, fellBack, compared: pool.length };
  if (denser.length) return { ...base, price: Math.max(1, unitPrice(denser[0]) - UNDERCUT), ref: denser[0], best: false };
  const top = [...pool].sort((a, b) => unitPrice(b) - unitPrice(a))[0] ?? null;
  return { ...base, price: top ? unitPrice(top) : null, ref: top, best: !!top };
}
