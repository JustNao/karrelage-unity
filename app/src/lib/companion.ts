/** Familiers / montiliers: what matters is their level ("X niveau") and the legendary status. */

/** Effect "#1 niveau #3". */
export const LEVEL_EFFECT = 2821;
/** Effect "Statut légendaire". */
export const LEGEND_EFFECT = 2862;

/** Total xp needed to reach level n (index n-1), n = 1..100. Each level costs more than the previous one. */
const XP_TOTAL = [
  1, 2, 3, 5, 7, 9, 11, 13, 15, 18, 21, 24, 28, 32, 37, 42, 48, 55, 62, 70, 79, 89, 101, 114, 128, 144, 162, 182, 204, 228,
  255, 285, 318, 355, 396, 441, 491, 546, 607, 675, 750, 832, 923, 1023, 1134, 1256, 1391, 1539, 1702, 1882, 2080, 2298,
  2537, 2800, 3089, 3407, 3756, 4140, 4561, 5023, 5530, 6087, 6698, 7368, 8102, 8907, 9789, 10755, 11814, 12974, 14244,
  15634, 17156, 18822, 20645, 22640, 24822, 27208, 29818, 32672, 35792, 39202, 42929, 47002, 51452, 56314, 61625, 67426,
  73761, 80678, 88230, 96475, 105475, 115298, 126018, 137715, 150478, 164402, 179591, 196159,
];

/** Total xp of a level (clamped to the table); this, not the level, is what the price follows. */
export const xpOfLevel = (lvl: number) => XP_TOTAL[Math.min(Math.max(Math.round(lvl), 1), XP_TOTAL.length) - 1];

/** Level of a lot, null when it carries no level effect. */
export function levelOf(row: any): number | null {
  const e = row.effects.find((e: any) => e.action === LEVEL_EFFECT && e.status !== "removed");
  return e?.hi ?? null;
}

export const isLegendary = (row: any) => row.effects.some((e: any) => e.action === LEGEND_EFFECT && e.status !== "removed");
