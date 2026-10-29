import { call, on } from "../api";
import { closeFloating, openFloating, savedPos } from "../floating";
import { settings, type CombatShow } from "./settings.svelte";
import { rules } from "./automation.svelte";
import { TAGS } from "../monsterTags";

/**
 * Current fight (crates/modules/src/combat.rs): the monsters fought and their notes, followed from startup whatever tab is
 * open (App.svelte calls `startCombat` once the backend is ready). The feature switch `settings.combat.enabled` is pushed
 * to the backend, and so is the Automation « À votre tour » rule (`combat:turn` is only sent while it is on).
 */
export interface Notes {
  warning?: string[];
  invulnerable?: string[];
  tips?: string[];
}
export interface Fighter {
  actor: number;
  gid: number;
  name: string;
  grade: number;
  level: number;
  summoned: boolean;
  /** died during the fight (still sent: a monster with notes stays listed, dimmed) */
  dead: boolean;
  /** wave of a multi-wave fight (dungeons, Songes); 0 = none */
  wave?: number;
  notes: Notes | null;
  /** DofusDB tags (lib/monsterTags.ts) */
  tags: string[];
  /** the sheet's "intéraction dangereuse": tags that make the other monsters of the fight dangerous next to this one */
  danger?: string[];
  /** round of the fight it appeared in (placement = 1) */
  since?: number | null;
  /** last round its turn started in */
  played?: number | null;
  /** element of the last damage it took that was not neutral (1 earth, 2 fire, 3 water, 4 air) */
  hit?: number | null;
}
/** A PA / PM poison on the player ("X dommages <element> pour 1 PA / PM utilisé"). */
export interface Poison {
  action: number;
  kind: "PA" | "PM";
  element: string;
  target: number;
}
export interface Fight {
  seen: boolean;
  active: boolean;
  started: number;
  ended: number | null;
  fighters: Fighter[];
  poisons?: Poison[];
  /** the player's confusion: their spells land this many quarter turns clockwise around them (negative = counter clockwise);
   *  null until a characteristics refresh carried it (lib/bosses.ts) */
  confusion?: number | null;
  /** current round of the fight (0 = placement, or no new round seen yet) */
  round?: number;
  /** the player's character id (null until a login was seen or a typed name found: no poison is shown) */
  me?: number | null;
  /** every character id known to be the player's (login seen, or found by name) */
  mine?: number[];
  /** the same, with their names once seen in a fight */
  characters?: { id: number; name: string | null }[];
  /** names typed in the Automation tab, not found in a fight yet */
  pending?: string[];
}

export const fight = $state<Fight>({ seen: false, active: false, started: 0, ended: null, fighters: [], poisons: [], me: null });

/** Keep `fight` up to date (main window and the floating window); `onStart` runs when a new fight begins. Resolves to the
 *  unsubscribe function. */
export async function followFight(onStart?: () => void): Promise<() => void> {
  const set = (f: Fight) => {
    const fresh = f.active && !(fight.active && fight.started === f.started);
    Object.assign(fight, f);
    if (fresh) onStart?.();
  };
  const off = await on<Fight>("combat:fight", set);
  set(await call<Fight>("combat", "state", {}, true));
  return off;
}

let started = false;
export async function startCombat() {
  if (started) return;
  started = true;
  $effect.root(() => {
    $effect(() => {
      const on = settings.combat.enabled;
      call("combat", "set_enabled", { on }, true)
        .then(() => !on && Object.assign(fight, { seen: false, active: false, started: 0, ended: null, fighters: [], poisons: [], confusion: null }))
        .catch(() => {});
    });
    $effect(() => {
      const on = rules.turn.enabled;
      call("combat", "set_turn", { on }, true).catch(() => {});
    });
    // the floating window follows the fight: shown when it starts (or when the setting is switched on during one), closed when it ends
    $effect(() => {
      const want = settings.combat.enabled && settings.combat.overlay && fight.active;
      void (want ? openCombatOverlay(false) : closeCombatOverlay());
    });
  });
  await followFight();
}

/** The player's characters (Automation tab): a name to look for among the fighters, or one to forget. */
export const addCharacterName = (name: string) => call<Fight>("combat", "add_name", { name }).then((f) => Object.assign(fight, f));
export const removeCharacterName = (name: string) => call<Fight>("combat", "remove_name", { name }).then((f) => Object.assign(fight, f));
export const removeCharacter = (id: number) => call<Fight>("combat", "remove", { id }).then((f) => Object.assign(fight, f));

/** Where the user left the floating window (logical px: x, y = top edge, bottom = bottom edge), remembered by the window itself. */
export const OVERLAY_POS_KEY = "karrelage.combat.overlayPos";

/** The floating fight window (combat.html, src/CombatOverlay.svelte): frameless, transparent, always on top, like the scan list;
 *  its height follows its content. `focus` = false when it opens by itself (the game keeps the focus). */
export async function openCombatOverlay(focus = true) {
  const pos = savedPos(OVERLAY_POS_KEY);
  const h = 120;
  // growing up: the bottom edge is the anchor (the window then grows to its content upwards)
  const y = pos && (settings.combat.overlayGrow === "up" && pos.bottom != null ? pos.bottom - h : pos.y);
  await openFloating("combat", { url: "combat.html", title: "Combat", width: OVERLAY_WIDTH, height: h, pos: pos && { x: pos.x, y: y! }, focus });
}

export const closeCombatOverlay = () => closeFloating("combat");

export const OVERLAY_WIDTH = 340;

/** How each kind of note is shown (tab and floating window). */
export const NOTE_KINDS: Record<keyof Notes, { icon: string; title: string }> = {
  warning: { icon: "⚠", title: "Avertissement" },
  invulnerable: { icon: "🛡", title: "Invulnérabilité" },
  tips: { icon: "💡", title: "Conseil" },
};

/** The kinds of notes shown for a display level. */
export const KINDS: Record<CombatShow, (keyof Notes)[]> = {
  none: [],
  warning: ["warning"],
  invulnerable: ["warning", "invulnerable"],
  all: ["warning", "invulnerable", "tips"],
};

/** One row per monster kind: the fighters of the same gid (and summoned or not) grouped; `alive` = how many are still alive. */
export interface Group {
  key: string;
  gid: number;
  name: string;
  summoned: boolean;
  levels: number[];
  alive: number;
  notes: Notes | null;
  tags: string[];
}

/** The groups worth listing at this display level: a monster without any note shown is dropped once all its copies died.
 *  Dangerous interactions: a living monster whose sheet says e.g. "poussée" puts a warning on every other monster of the
 *  fight that can push ("Poussée : dangereux avec Glourséleste"), and the Glourséleste itself ("Dangereux avec Poussée : …"). */
export function groups(list: Fighter[], show: CombatShow): Group[] {
  const by = new Map<string, Group>();
  // tag -> the living monsters that make it dangerous
  const danger = new Map<string, Set<string>>();
  for (const f of list) {
    const key = `${f.gid}:${f.summoned ? 1 : 0}`;
    const g = by.get(key) ?? { key, gid: f.gid, name: f.name, summoned: f.summoned, levels: [], alive: 0, notes: f.notes, tags: f.tags ?? [] };
    g.levels.push(f.level);
    if (!f.dead) {
      g.alive++;
      for (const t of f.danger ?? []) {
        if (!danger.has(t)) danger.set(t, new Set());
        danger.get(t)!.add(f.name);
      }
    }
    by.set(key, g);
  }
  // the living monsters having each tag (the other side of a dangerous interaction)
  const having = new Map<string, Set<string>>();
  for (const g of by.values()) if (g.alive) for (const t of g.tags) (having.get(t) ?? having.set(t, new Set()).get(t)!).add(g.name);
  const sourceDanger = new Map<string, string[]>(); // monster name -> its danger tags
  for (const f of list) if (!f.dead && f.danger?.length) sourceDanger.set(f.name, f.danger);
  for (const g of by.values()) {
    if (!g.alive) continue;
    const lines = g.tags.flatMap((t) => {
      const who = [...(danger.get(t) ?? [])].filter((n) => n !== g.name);
      return who.length ? [`${TAGS[t]?.label ?? t} : dangereux avec ${who.join(", ")}`] : [];
    });
    // the dangerous monster itself (e.g. Glourséleste with pushers around): warned too, so it is always listed
    for (const t of sourceDanger.get(g.name) ?? []) {
      const who = [...(having.get(t) ?? [])].filter((n) => n !== g.name);
      if (who.length) lines.push(`Dangereux avec ${TAGS[t]?.label ?? t} : ${who.join(", ")}`);
    }
    if (lines.length) g.notes = { ...g.notes, warning: [...lines, ...(g.notes?.warning ?? [])] };
  }
  return [...by.values()].filter((g) => g.alive > 0 || shown(g.notes, show).length > 0);
}

/** `groups` in display order: dead last, then summons; warnings first, then monsters with notes, then by name. */
export function sortedGroups(list: Fighter[], show: CombatShow): Group[] {
  const notes = (g: Group) => shown(g.notes, show).length > 0;
  return groups(list, show).sort(
    (a, b) =>
      Number(!a.alive) - Number(!b.alive) ||
      Number(a.summoned) - Number(b.summoned) ||
      (b.notes?.warning?.length ?? 0) - (a.notes?.warning?.length ?? 0) ||
      Number(notes(b)) - Number(notes(a)) ||
      a.name.localeCompare(b.name),
  );
}

/** A wave of a multi-wave fight and its groups (`sortedGroups` of its fighters); `alive` = its living monsters. */
export interface Wave {
  wave: number;
  alive: number;
  groups: Group[];
}

/** The fighters split by wave, in wave order, when they belong to several (null otherwise); waves with nothing to list are dropped. */
export function waves(list: Fighter[], show: CombatShow): Wave[] | null {
  const by = new Map<number, Fighter[]>();
  for (const f of list) (by.get(f.wave ?? 0) ?? by.set(f.wave ?? 0, []).get(f.wave ?? 0)!).push(f);
  if (by.size < 2) return null;
  return [...by.entries()]
    .sort(([a], [b]) => a - b)
    .map(([wave, l]) => ({ wave, alive: l.filter((f) => !f.dead).length, groups: sortedGroups(l, show) }))
    .filter((w) => w.groups.length);
}

/** The poisons to warn about: "Poison PA (Terre)" lines. */
export function poisonText(p: Poison): string {
  return `Poison ${p.kind} ${p.element} : chaque ${p.kind} utilisé inflige des dommages ${p.element}`;
}

/** Notes of a group kept by the display level (empty kinds dropped). */
export function shown(n: Notes | null, show: CombatShow): [keyof Notes, string[]][] {
  return KINDS[show].map((k) => [k, n?.[k] ?? []] as [keyof Notes, string[]]).filter(([, l]) => l.length > 0);
}
