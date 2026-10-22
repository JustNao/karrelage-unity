import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { call, on } from "../api";
import { app } from "../app.svelte";
import { settings, type CombatShow } from "./settings.svelte";

/**
 * Current fight (crates/modules/src/combat.rs): the monsters fought and their notes, followed from startup whatever tab is
 * open (App.svelte calls `startCombat` once the backend is ready). The feature switch `settings.combat.enabled` is pushed
 * to the backend; `autoOpen` opens the Combat tab when a fight starts.
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
  notes: Notes | null;
}
export interface Fight {
  seen: boolean;
  active: boolean;
  started: number;
  ended: number | null;
  fighters: Fighter[];
}

export const fight = $state<Fight>({ seen: false, active: false, started: 0, ended: null, fighters: [] });

/** Keep `fight` up to date (main window and the floating window); `onStart` runs when a new fight begins. */
export async function followFight(onStart?: () => void) {
  const set = (f: Fight) => {
    const fresh = f.active && !(fight.active && fight.started === f.started);
    Object.assign(fight, f);
    if (fresh) onStart?.();
  };
  await on<Fight>("combat:fight", set);
  set(await call<Fight>("combat", "state", {}, true));
}

let started = false;
export async function startCombat() {
  if (started) return;
  started = true;
  $effect.root(() => {
    $effect(() => {
      const on = settings.combat.enabled;
      call("combat", "set_enabled", { on }, true)
        .then(() => !on && Object.assign(fight, { seen: false, active: false, started: 0, ended: null, fighters: [] }))
        .catch(() => {});
    });
  });
  await followFight(() => {
    if (!settings.combat.enabled) return;
    if (settings.combat.autoOpen) app.tab = "combat";
    if (settings.combat.overlayAuto) void openCombatOverlay();
  });
}

/** The floating fight window (combat.html, src/CombatOverlay.svelte): frameless, transparent, always on top, like the scan list. */
export async function openCombatOverlay() {
  const open = await WebviewWindow.getByLabel("combat");
  if (open) {
    await open.setFocus();
    return;
  }
  new WebviewWindow("combat", { url: "combat.html", title: "Combat", width: 340, height: 420, decorations: false, transparent: true, shadow: false, alwaysOnTop: true, skipTaskbar: true, resizable: true });
}

/** The kinds of notes shown for a display level. */
export const KINDS: Record<CombatShow, (keyof Notes)[]> = {
  warning: ["warning"],
  invulnerable: ["warning", "invulnerable"],
  all: ["warning", "invulnerable", "tips"],
};

/** One row per monster kind: the fighters of the same gid (and summoned or not) grouped, with their levels. */
export interface Group {
  key: string;
  gid: number;
  name: string;
  summoned: boolean;
  levels: number[];
  notes: Notes | null;
}

export function groups(list: Fighter[]): Group[] {
  const by = new Map<string, Group>();
  for (const f of list) {
    const key = `${f.gid}:${f.summoned ? 1 : 0}`;
    const g = by.get(key) ?? { key, gid: f.gid, name: f.name, summoned: f.summoned, levels: [], notes: f.notes };
    g.levels.push(f.level);
    by.set(key, g);
  }
  return [...by.values()];
}

/** Notes of a group kept by the display level (empty kinds dropped). */
export function shown(n: Notes | null, show: CombatShow): [keyof Notes, string[]][] {
  return KINDS[show].map((k) => [k, n?.[k] ?? []] as [keyof Notes, string[]]).filter(([, l]) => l.length > 0);
}
