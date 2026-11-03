/**
 * Class help of the in-game view (fightmap/ClassLayer.svelte, shortcut `classHelp`).
 *
 * Steamer: each turret gives its passive bonus to the Steamer while he stands at most `TURRET_RANGE` cells from it and in its
 * line of sight. Turrets are recognised by their monster gid (pack monsters, names from the game's strings).
 */
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
