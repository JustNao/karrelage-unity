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
  /** ints of its monster information that are pack monsters (its gid among them) */
  monsters: number[];
  /** who summoned it (0: nobody / not told) */
  summoner: number;
  /** % resistances: neutral, earth, fire, water, air (null: not told) */
  resists: [number, number, number, number, number] | null;
  /** Invisible: its last known cell is not where it stands. */
  invisible: boolean;
}
/** An Eliotrope portal on the ground (closed: a fighter on it, or deactivated this turn). */
export interface MapPortal {
  cell: number;
  author: number;
  open: boolean;
}
export interface FightMapState {
  map: number | null;
  fight: boolean;
  fighters: MapFighter[];
  /** in the order they were put down */
  portals: MapPortal[];
}
/** Diagonal coordinates of a cell (crates/data/src/maps.rs `coords`). */
function coords(cell: number): [number, number] {
  const a = Math.floor(cell / (2 * ROW)), r = cell % (2 * ROW);
  return [a + Math.floor(r / ROW) + (r % ROW), (r % ROW) - a];
}
/** Distance between two cells in PO (range), as the game counts it (maps.rs `distance`). */
export function cellDistance(a: number, b: number): number {
  const [xa, ya] = coords(a), [xb, yb] = coords(b);
  return Math.abs(xa - xb) + Math.abs(ya - yb);
}

/** The cells fighters stand on (and block). */
export const occupiedCells = (s: FightMapState) => s.fighters.filter((f) => f.alive && !f.carried).map((f) => f.cell);

/** What a fighter on a cell reaches at the chosen range: cells in line of sight, and the ones behind an obstacle. */
export interface Sight {
  seen: number[];
  hidden: number[];
}

/** Combat tab « Portails » mode (call `plan`): the two portals completing four from an entrance, the way ending at the other
 *  portal put down with the most cells between them. */
export interface PortalPlan {
  /** entrance, the two new portals in the order gone through, exit */
  path: number[];
  distance: number;
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

/** Line of sight with no spell range: every cell of the map. */
export const ALL_RANGE = { min: 1, max: 99 };

/**
 * Where the game draws the grid in its window (in-game overlay, FightMapOverlay.svelte): a cell's centre `centre(c, 1)` in
 * grid units lands at (ox + sx × gx, oy + sy × gy) physical pixels of the Dofus window's client area. Computed from the window's
 * size (`gridOfWindow`) unless the player took one by hand (two clicks, valid for the client size it was taken at).
 */
export interface Calibration {
  width: number;
  height: number;
  ox: number;
  oy: number;
  sx: number;
  sy: number;
}
/** The calibration taken by hand; none: `gridOfWindow`. */
export const CALIBRATION_KEY = "karrelage.combat.manualCalibration";
/** The next view asks for a calibration by hand (Combat tab). */
export const CALIBRATE_ASKED_KEY = "karrelage.combat.calibrateAsked";

/** A cell's width and the map's centre in the picture, as parts of the client height: fitted on the floor's edges of a capture
 *  of a fight at 2560 × 1440 (s = 123.6, centre 1281.5, 628; a careful calibration by hand on another map gave the same). */
const CELL_OF_HEIGHT = 0.08583;
const CENTRE_Y_OF_HEIGHT = 0.4361;
const CENTRE_X_SHIFT_OF_HEIGHT = 0.001;

/** The grid from the client area's size alone: the picture scales with the height, the map centred across the width. Only
 *  16:9 measured; assumes the camera isn't zoomed. */
export function gridOfWindow(width: number, height: number): Calibration {
  const s = CELL_OF_HEIGHT * height;
  const [mx, my] = [ROW / 2 + 0.25, ROWS / 8 + 0.125]; // the map's middle in grid units (`mapSize(1)` / 2)
  return { width, height, ox: width / 2 + CENTRE_X_SHIFT_OF_HEIGHT * height - s * mx, oy: CENTRE_Y_OF_HEIGHT * height - s * my, sx: s, sy: s };
}

/** The calibration from two clicks (physical px) on the centres of cells `a` and `b`; null when they don't make sense
 *  (cells too close, or a picture squeezed in one direction: a click went wrong). One scale for both directions (the game's
 *  diamonds are twice as wide as high, like centre()), so that a click a little off doesn't stretch the grid. */
export function calibrate(a: number, pa: [number, number], b: number, pb: [number, number], width: number, height: number): Calibration | null {
  const [ga, gb] = [centre(a, 1), centre(b, 1)];
  const [dgx, dgy] = [gb[0] - ga[0], gb[1] - ga[1]];
  const [dx, dy] = [pb[0] - pa[0], pb[1] - pa[1]];
  const sx = Math.abs(dgx) >= 1 ? dx / dgx : NaN;
  const sy = Math.abs(dgy) >= 0.5 ? dy / dgy : NaN;
  if (Number.isNaN(sx) && Number.isNaN(sy)) return null;
  if (!Number.isNaN(sx) && !Number.isNaN(sy) && !(sx / sy > 0.75 && sx / sy < 1.33)) return null;
  const s = (dgx * dx + dgy * dy) / (dgx * dgx + dgy * dgy); // least squares
  if (!(s > 10)) return null;
  return { width, height, ox: (pa[0] + pb[0]) / 2 - (s * (ga[0] + gb[0])) / 2, oy: (pa[1] + pb[1]) / 2 - (s * (ga[1] + gb[1])) / 2, sx: s, sy: s };
}

/** The cell whose diamond holds the point (physical px of the client area), null outside the grid. */
export function cellAt(cal: Calibration, x: number, y: number): number | null {
  const gx = (x - cal.ox) / cal.sx, gy = (y - cal.oy) / cal.sy;
  // diamonds overlap in height: the point's nearest row and its two neighbours (`centre` backwards), the column nearest in each
  const near = Math.round(gy * 4 - 1);
  for (let row = Math.max(0, near - 1); row <= Math.min(ROWS - 1, near + 1); row++) {
    const col = Math.round(gx - (row % 2 ? 1 : 0.5));
    if (col < 0 || col >= ROW) continue;
    const c = row * ROW + col;
    const [cx, cy] = centre(c, 1);
    if (Math.abs(gx - cx) / 0.5 + Math.abs(gy - cy) / 0.25 <= 1) return c;
  }
  return null;
}

/** A cell's centre on screen, CSS px (`dpr` = window.devicePixelRatio). */
export function screenCentre(cal: Calibration, c: number, dpr: number): [number, number] {
  const [gx, gy] = centre(c, 1);
  return [(cal.ox + cal.sx * gx) / dpr, (cal.oy + cal.sy * gy) / dpr];
}

/** A cell's diamond on screen: SVG points in CSS px (`dpr` = window.devicePixelRatio). */
export function screenDiamond(cal: Calibration, c: number, dpr: number): string {
  const [gx, gy] = centre(c, 1);
  const [x, y] = [(cal.ox + cal.sx * gx) / dpr, (cal.oy + cal.sy * gy) / dpr];
  const [hw, hh] = [(cal.sx * 0.5) / dpr, (cal.sy * 0.25) / dpr];
  return `${x},${y - hh} ${x + hw},${y} ${x},${y + hh} ${x - hw},${y}`;
}
