import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { call } from "./api";
import { closeFloating, openFloating } from "./floating";

/**
 * Fight map (crates/modules/src/fight_map.rs): the map being fought on and where every fighter stands, for the Combat tab's
 * minimap. A map is 560 cells in 40 staggered rows of 14, odd rows shifted half a cell right (as the game draws them).
 */
export const MAP_CELLS = 560;
export const ROW = 14;
export const ROWS = 40;
/** Cell flags (pack maps.bin): walkable, lets the line of sight through, red / blue placement cell. */
export const MOV = 1, LOS = 2, RED = 4, BLUE = 8;

export interface MapFighter {
  id: number;
  cell: number;
  team: number | null;
  kind: "player" | "monster" | "creature" | "other";
  alive: boolean;
  summoned: boolean;
  carried: boolean;
}
export interface FightMapState {
  map: number | null;
  fight: boolean;
  fighters: MapFighter[];
}
/** What a fighter on a cell reaches at the chosen range: cells in line of sight, and the ones behind an obstacle. */
export interface Sight {
  seen: number[];
  hidden: number[];
}

/** Centre of a cell in a grid of `w` × `w / 2` diamonds (top-left of the map at 0, 0). */
export function centre(cell: number, w: number): [number, number] {
  const row = Math.floor(cell / ROW), col = cell % ROW;
  return [col * w + (row % 2 ? w : w / 2), (row * w) / 4 + w / 4];
}

/** SVG points of a cell's diamond. */
export function diamond(cell: number, w: number): string {
  const [x, y] = centre(cell, w);
  const h = w / 4;
  return `${x},${y - h} ${x + w / 2},${y} ${x},${y + h} ${x - w / 2},${y}`;
}

/** Size of the whole map drawn with diamonds `w` wide. */
export const mapSize = (w: number): [number, number] => [ROW * w + w / 2, (ROWS * w) / 4 + w / 4];

/** The « Ligne de vue en jeu » shortcut: closes the in-game view when it is open, else opens it during a fight. */
export async function toggleFightMapOverlay() {
  if (await WebviewWindow.getByLabel(OVERLAY)) {
    await call("fightmap", "watch_escape", { on: false }, true).catch(() => {});
    return closeFloating(OVERLAY);
  }
  const s = await call<FightMapState>("fightmap", "state", {}, true);
  if (s.fight && s.map !== null) await openFloating(OVERLAY, { url: "fightmap.html", title: "Ligne de vue", width: 800, height: 600 });
}
const OVERLAY = "fightmap";

/** Line of sight with no spell range: every cell of the map. */
export const ALL_RANGE = { min: 1, max: 99 };

/**
 * Where the game draws the grid in its window (in-game overlay, FightMapOverlay.svelte): a cell's centre `centre(c, 1)` in
 * grid units lands at (ox + sx × gx, oy + sy × gy) physical pixels of the Dofus window's client area. Taken from two clicks on
 * fighters whose cells are known; only valid for the client size it was taken at.
 */
export interface Calibration {
  width: number;
  height: number;
  ox: number;
  oy: number;
  sx: number;
  sy: number;
}
export const CALIBRATION_KEY = "karrelage.combat.calibration";

/** The calibration from two clicks (physical px) on the centres of cells `a` and `b`; null when they don't make sense
 *  (cells too close, or a picture squeezed in one direction: a click went wrong). */
export function calibrate(a: number, pa: [number, number], b: number, pb: [number, number], width: number, height: number): Calibration | null {
  const [ga, gb] = [centre(a, 1), centre(b, 1)];
  const [dgx, dgy] = [gb[0] - ga[0], gb[1] - ga[1]];
  let sx = Math.abs(dgx) >= 1 ? (pb[0] - pa[0]) / dgx : NaN;
  let sy = Math.abs(dgy) >= 0.5 ? (pb[1] - pa[1]) / dgy : NaN;
  if (Number.isNaN(sx)) sx = sy;
  if (Number.isNaN(sy)) sy = sx;
  // the game draws diamonds twice as wide as high, like centre(): both scales are about the same
  if (!(sx > 10 && sy > 10 && sx / sy > 0.75 && sx / sy < 1.33)) return null;
  return { width, height, ox: pa[0] - sx * ga[0], oy: pa[1] - sy * ga[1], sx, sy };
}

/** The cell whose diamond holds the point (physical px of the client area), null outside the grid. */
export function cellAt(cal: Calibration, x: number, y: number): number | null {
  const gx = (x - cal.ox) / cal.sx, gy = (y - cal.oy) / cal.sy;
  for (let c = 0; c < MAP_CELLS; c++) {
    const [cx, cy] = centre(c, 1);
    if (Math.abs(gx - cx) / 0.5 + Math.abs(gy - cy) / 0.25 <= 1) return c;
  }
  return null;
}

/** A cell's diamond on screen: SVG points in CSS px (`dpr` = window.devicePixelRatio). */
export function screenDiamond(cal: Calibration, c: number, dpr: number): string {
  const [gx, gy] = centre(c, 1);
  const [x, y] = [(cal.ox + cal.sx * gx) / dpr, (cal.oy + cal.sy * gy) / dpr];
  const [hw, hh] = [(cal.sx * 0.5) / dpr, (cal.sy * 0.25) / dpr];
  return `${x},${y - hh} ${x + hw},${y} ${x},${y + hh} ${x - hw},${y}`;
}
