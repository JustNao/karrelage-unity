/** Price/density analysis shared by the chart and the "Good deals" section. */

export interface Fit {
  slope: number;
  icpt: number;
  sigma: number;
  /** Standardised residual of each point: how far above (+) / below (-) the trend it sits. */
  z: number[];
}

/** A lot this far above the trend line (in residual standard deviations) counts as a good deal. */
export const GOOD_Z = 2;
/** Below this many lots the trend is not meaningful, so nothing is flagged. */
export const MIN_POINTS = 5;

/** Least-squares line of y on x, with the residual spread. */
export function analyse(pts: { x: number; y: number }[]): Fit {
  const n = pts.length;
  const mx = pts.reduce((s, p) => s + p.x, 0) / n;
  const my = pts.reduce((s, p) => s + p.y, 0) / n;
  const sxx = pts.reduce((s, p) => s + (p.x - mx) ** 2, 0);
  const slope = sxx > 0 ? pts.reduce((s, p) => s + (p.x - mx) * (p.y - my), 0) / sxx : 0;
  const icpt = my - slope * mx;
  const res = pts.map((p) => p.y - (slope * p.x + icpt));
  const sigma = Math.sqrt(res.reduce((s, r) => s + r * r, 0) / Math.max(1, n - 2));
  return { slope, icpt, sigma, z: res.map((r) => (sigma > 1e-9 ? r / sigma : 0)) };
}

export const isGoodDeal = (fit: Fit, i: number, n: number) => n >= MIN_POINTS && fit.sigma > 1e-9 && fit.z[i] >= GOOD_Z;
