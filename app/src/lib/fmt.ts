/** Number formatting shared by every tab: one cached formatter, one minus sign (typographic "−"). */

const nf = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 3 });
const nf0 = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 0 });
const nfs = new Map<number, Intl.NumberFormat>();
/** Kamas: "," between thousands ("2,380,000"), "." for the rare decimals. */
const nfk = new Intl.NumberFormat("en-US", { maximumFractionDigits: 3 });
const MINUS = "−";

/** "12 345" (rounded to an integer). */
export const fmtInt = (n: number) => (n < 0 ? MINUS : "") + nf0.format(Math.abs(Number(n)));
/** "1 234,57" (at most `digits` decimals). */
export const fmtNum = (n: number, digits = 2) => {
  let f = nfs.get(digits);
  if (!f) nfs.set(digits, (f = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: digits })));
  return (n < 0 ? MINUS : "") + f.format(Math.abs(Number(n)));
};

/** A price with its unit: "12,345 kamas"; from a million up, compacted: 1M3 = 1.3 million, 10M3 = 10.3 million. */
export const kamas = (n: number) => {
  const v = Number(n);
  const sign = v < 0 ? MINUS : "";
  if (Math.abs(v) < 1e6) return `${sign}${nfk.format(Math.abs(v))} kamas`;
  const tenths = Math.round(Math.abs(v) / 1e5); // millions, one decimal, as an integer
  const frac = tenths % 10;
  return `${sign}${Math.floor(tenths / 10)}M${frac || ""} kamas`;
};
/** Full price, no compaction, "K" as unit: "2,380,000 K". */
export const kamasFull = (n: number) => `${Number(n) < 0 ? MINUS : ""}${nfk.format(Math.abs(Number(n)))} K`;
/** A gain or a loss: "+12,345 kamas" / "−3M2 kamas" (nothing in front of 0). */
export const signedKamas = (n: number) => (n > 0 ? "+" : "") + kamas(n);
/** A rate in percent, signed: "+12.3 %" / "−4.0 %". */
export const pct = (n: number, digits = 1) => `${n > 0 ? "+" : n < 0 ? MINUS : ""}${Math.abs(n).toFixed(digits)} %`;
/** Short axis label: 1.25M, 12.5k, 830. */
export const fmtCompact = (n: number) => (n >= 1e6 ? +(n / 1e6).toFixed(2) + "M" : n >= 1e3 ? +(n / 1e3).toFixed(1) + "k" : String(Math.round(n)));

/** A price or "—" when unknown. */
export const kamasOr = (n: number | null | undefined, none = "—") => (n == null ? none : kamas(n));

const dtf = new Intl.DateTimeFormat("fr-FR", { day: "2-digit", month: "2-digit", year: "numeric", hour: "2-digit", minute: "2-digit", second: "2-digit" });
const hmf = new Intl.DateTimeFormat("fr-FR", { hour: "2-digit", minute: "2-digit" });
/** Unix seconds -> "20/10/2026 22:30:39". */
export const fmtDateTime = (t: number) => dtf.format(new Date(t * 1000));
/** Unix seconds -> "22:30". */
export const fmtTime = (t: number) => hmf.format(new Date(t * 1000));
/** Bytes -> "512 o", "12,3 ko". */
export const fmtSize = (n: number | null | undefined) => (n == null ? "" : n < 1024 ? `${n} o` : `${fmtNum(n / 1024, n < 10240 ? 1 : 0)} ko`);

/** A rune's name without its "Rune " prefix: "Rune Pa Fo" -> "Pa Fo". */
export const runeShort = (name: string | undefined) => name?.replace(/^Rune /, "");

/** "il y a 12s / 5min / 3h". */
export const ago = (t: number) => {
  const d = Math.round(Date.now() / 1000 - t);
  return "il y a " + (d < 60 ? `${d}s` : d < 3600 ? `${Math.round(d / 60)}min` : `${Math.round(d / 3600)}h`);
};
