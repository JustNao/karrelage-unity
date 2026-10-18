/** Effect classification shared by the UI (the backend sends `group`/`order`, see pipeline/export_pack.py). */

export interface EffectMeta {
  id: number;
  label: string;
  /** weapon | stats | main | secondary | damage | resist | other */
  group: string;
  /** Rank inside the group (in-game characteristics panel order). */
  order: number;
  /** effectPowerRate (AP 100, MP 90, ...). Not displayed yet. */
  density: number;
  /** Offered in the item filter (no negative twins, nothing from "other" but the hunting weapon). */
  filterable?: boolean;
  /** Characteristic the effect modifies ("+Force" and "-Force" share it). */
  stat?: number;
}

/** Sections of the filter list / effect picker, following the in-game characteristics panel. */
export const FILTER_GROUPS = [
  { key: "weapon", label: "Dommages d'arme", members: ["weapon"] },
  { key: "main", label: "Caractéristiques principales", members: ["stats", "main"] },
  { key: "secondary", label: "Caractéristiques secondaires", members: ["secondary"] },
  { key: "damage", label: "Dommages", members: ["damage"] },
  { key: "resist", label: "Résistances", members: ["resist"] },
  { key: "other", label: "Autres", members: ["other"] },
];

/** Order of effects on an item: weapon hits, stats / main stats, damage, resistances, then utility (secondary + other). */
const ITEM_ORDER: Record<string, number> = { weapon: 0, stats: 1, main: 2, damage: 3, resist: 4, secondary: 5, other: 6 };

/** Rank of an effect group in the item ordering. */
export const groupRank = (g: string) => ITEM_ORDER[g] ?? 6;

/** New (added) effects first, missing default effects last, the rest by group then rank. */
export function sortEffects<T extends { status?: string; group?: string; order?: number }>(effs: T[]): T[] {
  const phase = (e: T) => (e.status === "added" ? 0 : e.status === "removed" ? 2 : 1);
  return [...effs].sort(
    (a, b) =>
      phase(a) - phase(b) ||
      (ITEM_ORDER[a.group ?? "other"] ?? 6) - (ITEM_ORDER[b.group ?? "other"] ?? 6) ||
      (a.order ?? 0) - (b.order ?? 0),
  );
}

/** How close to the default max still counts as "perfect": vitality within 10; force, intelligence, agility, chance (and wisdom) within 3. */
const PERFECT_TOLERANCE: Record<number, number> = { 125: 10, 118: 3, 126: 3, 119: 3, 123: 3, 124: 3 };

const tolerance = (action: number) => PERFECT_TOLERANCE[action] ?? 0;

/** The weapon's own effects (hits, -PA, pushback...): shown plain, without range or colours. */
/** Effects that are only markers (element change: the lot has the new hit, flagged "added", on its own). */
export const HIDDEN_EFFECTS = new Set([700]);
/** Flag effects (hunting weapon): present or not, no value. */
/** Passive of a legendary item (shown as the passive's name, in yellow). */
export const LEGENDARY_EFFECT = 1175;
export const FLAG_EFFECTS = new Set([795]);
export const isWeaponEffect = (e: any) => e.group === "weapon";
export const isNegative = (e: any) => !isWeaponEffect(e) && e.density < 0 || String(e.text ?? "").startsWith("-");
/** The line is at (or above) its default max, within the main-stat tolerance. */
export const isPerfectLine = (e: any) => !isWeaponEffect(e) && !!e.range && !isNegative(e) && e.status !== "removed" && (e.hi ?? -Infinity) >= e.range[1] - tolerance(e.action);
/** The line is below the default minimum of its range. */
export const isBelowMin = (e: any) => !isWeaponEffect(e) && !!e.range && !isNegative(e) && e.status !== "removed" && (e.hi ?? Infinity) < e.range[0];

export interface Perfection {
  damage: boolean;
  resist: boolean;
  main: boolean;
}

/**
 * Perfect rolls of a lot, judged on the item's default effects only (those carry a `range`): a group is perfect when it has
 * default effects and all of them are present at (or above) their default max, within the tolerance for main stats.
 */
export function perfection(effs: any[]): Perfection {
  const check = (groups: string[]) => {
    const rel = effs.filter((e) => groups.includes(e.group) && e.range && !(e.density < 0));
    return rel.length > 0 && rel.every(isPerfectLine);
  };
  return { damage: check(["damage"]), resist: check(["resist"]), main: check(["stats", "main"]) };
}

/** [X] for a single possible value, [min–max] for a range (filter rows and item lines alike). */
export const rangeLabel = (def: number[] | null | undefined) => (def && (def[0] || def[1]) ? (def[0] === def[1] ? `[${def[0]}]` : `[${def[0]}–${def[1]}]`) : "");

/** Accent/case-insensitive form for matching what the user types. */
export const norm = (s: string) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();

/** Split effects into the FILTER_GROUPS sections (empty sections dropped), each sorted by rank then label. */
export function sections<T extends { group: string; order: number; label: string }>(list: T[]) {
  return FILTER_GROUPS.map((g) => ({
    ...g,
    items: list
      .filter((e) => g.members.includes(e.group))
      .sort((a, b) => g.members.indexOf(a.group) - g.members.indexOf(b.group) || a.order - b.order || a.label.localeCompare(b.label)),
  })).filter((s) => s.items.length);
}
