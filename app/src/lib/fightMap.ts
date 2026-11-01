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
