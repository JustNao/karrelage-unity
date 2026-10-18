/** Effect classification shared by the UI (the backend sends `group`/`order`, see pipeline/export_pack.py). */

export interface EffectMeta {
  id: number;
  label: string;
  /** stats | main | secondary | damage | resist | other */
  group: string;
  /** Rank inside the group (in-game characteristics panel order). */
  order: number;
  /** effectPowerRate (AP 100, MP 90, ...). Not displayed yet. */
  density: number;
}

/** Sections of the filter list / effect picker, following the in-game characteristics panel. */
export const FILTER_GROUPS = [
  { key: "main", label: "Main stats", members: ["stats", "main"] },
  { key: "secondary", label: "Secondary stats", members: ["secondary"] },
  { key: "damage", label: "Damage", members: ["damage"] },
  { key: "resist", label: "Resistances", members: ["resist"] },
  { key: "other", label: "Other", members: ["other"] },
];

/** Order of effects on an item: stats / main stats, damage, resistances, then utility (secondary + other). */
const ITEM_ORDER: Record<string, number> = { stats: 0, main: 1, damage: 2, resist: 3, secondary: 4, other: 5 };

/** New (added) effects first, missing default effects last, the rest by group then rank. */
export function sortEffects<T extends { status?: string; group?: string; order?: number }>(effs: T[]): T[] {
  const phase = (e: T) => (e.status === "added" ? 0 : e.status === "removed" ? 2 : 1);
  return [...effs].sort(
    (a, b) =>
      phase(a) - phase(b) ||
      (ITEM_ORDER[a.group ?? "other"] ?? 5) - (ITEM_ORDER[b.group ?? "other"] ?? 5) ||
      (a.order ?? 0) - (b.order ?? 0),
  );
}

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
