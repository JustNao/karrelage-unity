/**
 * Forgemagie overlay (ForgemagieOverlay.svelte): the game's forgemagie window measured once on a screenshot (`REF`), placed
 * on the screen by a calibration (two clicks on its corners: it can be dragged and the interface scaled in game), and the
 * backend's item (crates/modules/src/forgemagie.rs) with each rune's chances (dofocus.fr's players' model).
 */

export interface FmOdds {
  sc: number;
  sn: number;
  ec: number;
}
export interface FmRune {
  gid: number;
  name: string;
  points: number;
  /** null: the game refuses it (line or item over its cap) */
  odds: FmOdds | null;
}
export interface FmLine {
  stat: number;
  value: number;
  min: number | null;
  max: number | null;
  /** from the smallest: the window's Rune / Pa / Ra columns */
  runes: FmRune[];
}
export interface FmItem {
  uid: number;
  gid: number;
  name: string;
  level: number;
  lines: FmLine[];
}

/** The forgemagie window's top-left corner in the Dofus window's client area and its scale (physical px), for that client
 *  size: the UI scale follows it, so another size needs a new calibration. */
export interface FmCalibration {
  width: number;
  height: number;
  x: number;
  y: number;
  sx: number;
  sy: number;
}
export const CALIBRATION_KEY = "karrelage.forgemagie.calibration";
/** The chances shown under the runes (the overlay's button), remembered. */
export const ADVANCED_KEY = "karrelage.forgemagie.advanced";

/** The forgemagie window as measured on a capture (px from its top-left corner, at scale 1). */
export const REF = {
  width: 826,
  height: 704,
  /** centre of the first line's first rune icon, distance between lines / rune columns, an icon's side */
  rune: { x: 698, y: 176.5, row: 40.4, col: 40.5, size: 28 },
  /** the « Fusionner tout » / « Fusionner » row: its middle, a button's height; our button ends this far from the right edge */
  merge: { y: 100, height: 25, right: 14, width: 104 },
  /** lines visible before the list scrolls */
  rows: 13,
};

/** The calibration from the two clicks (physical px of the client area) on the window's top-left and bottom-right corners;
 *  null when they don't look like the window (too small, or its proportions off by more than 15 %). */
export function calibrate(a: [number, number], b: [number, number], width: number, height: number): FmCalibration | null {
  const [sx, sy] = [(b[0] - a[0]) / REF.width, (b[1] - a[1]) / REF.height];
  if (sx < 0.3 || sy < 0.3 || Math.abs(sx / sy - 1) > 0.15) return null;
  return { width, height, x: a[0], y: a[1], sx, sy };
}

/** A point of the reference window on the screen (physical px of the client area). */
export const at = (c: FmCalibration, x: number, y: number): [number, number] => [c.x + x * c.sx, c.y + y * c.sy];

/** Centre of line `row`'s rune column `col`. */
export const runeCentre = (c: FmCalibration, row: number, col: number) => at(c, REF.rune.x + col * REF.rune.col, REF.rune.y + row * REF.rune.row);

/** The overlay's button (physical px of the client area): at the right end of the merge row once calibrated, else at the
 *  top of the game's picture (it starts the calibration). */
export function buttonRect(c: FmCalibration | null, width: number, dpr: number) {
  if (!c) {
    const [w, h] = [190 * dpr, 28 * dpr];
    return { x: (width - w) / 2, y: 10 * dpr, w, h };
  }
  const { merge } = REF;
  const [x, y] = at(c, REF.width - merge.right - merge.width, merge.y - merge.height / 2);
  return { x, y, w: merge.width * c.sx, h: merge.height * c.sy };
}

/** "52 %", "2,8 %" (one decimal under 10). */
export const pctText = (v: number) => `${v < 10 ? v.toFixed(1).replace(".", ",") : Math.round(v)} %`;
