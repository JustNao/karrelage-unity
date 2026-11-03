/**
 * Class help of the in-game view (fightmap/ClassLayer.svelte, shortcut `classHelp`).
 *
 * Steamer: each turret gives its passive bonus to the Steamer while he stands at most `TURRET_RANGE` cells from it and in its
 * line of sight. Turrets are recognised by their monster gid (pack monsters, names from the game's strings).
 */
import type { FightMapState } from "./fightMap";

export const TURRET_RANGE = 7;

export const TURRETS: Record<number, { name: string; colour: string }> = {
  5836: { name: "Harponneuse", colour: "#ff5a5a" },
  5835: { name: "Gardienne", colour: "#4cd27a" },
  5837: { name: "Tactirelle", colour: "#f2c230" },
  5832: { name: "Chalutier", colour: "#5aa9ff" },
  5833: { name: "Foreuse", colour: "#ff9a3c" },
  5831: { name: "Bathyscaphe", colour: "#b57bff" },
};

/** The turret gid among a fighter's monster ints, if it is one. */
export const turretOf = (monsters: number[]) => monsters.find((g) => g in TURRETS);

/**
 * Eliotrope (fightmap/PortalLayer.svelte): four portals at most, the oldest goes when a fifth is put down; a spell going into
 * a portal comes out of the network's exit (crates/data/src/maps.rs `portal_path`).
 */
export interface PortalNet {
  /** 1 = the oldest */
  portals: { cell: number; number: number; open: boolean }[];
  /** the simulated portal (also in `portals`) */
  added: number | null;
  /** the oldest portal, gone because of the simulated one */
  dropped: number | null;
  /** cast into one of the portals: `cast` = the spell's way through the network, then the cell `added` lands on */
  via: boolean;
  cast: number[];
  /** the new network's way from the new portal to its exit */
  path: number[];
  /** cells between the path's portals */
  distance: number;
}

/** Damage bonus of a spell through the portals: % per cell between them along the way. */
export const PORTAL_BONUS = 2;

/** With four portals: the next one to go (#1) and the one after (#2) stand out. */
export const portalColour = (number: number, full: boolean) => (full && number === 1 ? "#ff4d4d" : full && number === 2 ? "#ffa53c" : "#5aa9ff");

/** Whose portals to show: the player's (the caster of one of their characters' latest portal, else their character in the
 *  fight: a first portal simulated), else the latest portal's caster; null when nothing tells. */
export function portalAuthor(live: FightMapState, mine: Set<number>): number | null {
  const portals = live.portals ?? [];
  const own = [...portals].reverse().find((p) => mine.has(p.author))?.author ?? live.fighters.find((f) => mine.has(f.id))?.id;
  return own ?? portals.at(-1)?.author ?? null;
}
