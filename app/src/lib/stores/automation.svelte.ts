/**
 * Automation rules (Automation tab), remembered in localStorage `karrelage.automation`, and the notifications they raise
 * (header « Notifications » area; kept for the session only).
 *
 * To add a rule: a default in DEFAULTS (missing keys of older saves fall back to it), its check in lib/automation.ts and a
 * card in the tab. Every rule has `enabled`, `sound` (id from lib/sounds.ts) and `volume` (0-1).
 */
import { DEFAULT_SOUND } from "../sounds";

const KEY = "karrelage.automation";

export interface RuleBase { enabled: boolean; sound: string; volume: number }

const DEFAULTS = {
  /** HDV: a scanned lot with a "% dommages" exo cheaper than the transcendance rune of that stat, that pays back when broken
   *  from a low enough coefficient. */
  exoDo: { enabled: true, sound: DEFAULT_SOUND, volume: 0.6, /** Highest break-even coefficient (%, Brisage « Coef. rentable »). */ maxEven: 500 },
};
export type Rules = typeof DEFAULTS;
export type RuleId = keyof Rules;
export const RULE_DEFAULTS: Rules = DEFAULTS;
export const RULE_TITLES: Record<RuleId, string> = { exoDo: "Exo % dommages à briser" };

function load(): Rules {
  const out = structuredClone(DEFAULTS);
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    for (const r of Object.keys(out) as RuleId[]) Object.assign(out[r], saved?.[r] ?? {});
  } catch {}
  return out;
}

export const rules = $state<Rules>(load());

$effect.root(() => {
  $effect(() => {
    const json = JSON.stringify(rules); // read deeply: every change saves
    try {
      localStorage.setItem(KEY, json);
    } catch {}
  });
});

/** A triggered rule. `gid` + `lot` (lotKeys key) lead to the lot in the HDV tab. */
export interface Notification { id: number; t: number; rule: RuleId; title: string; detail: string; gid: number; lot: string }

/** Most recent first, at most MAX. */
const MAX = 50;
let seq = 0;

class Notifications {
  list = $state<Notification[]>([]);
  /** Raised since the popup was last opened (pulsing dot). */
  unread = $state(0);

  push(n: Omit<Notification, "id" | "t">) {
    this.list = [{ ...n, id: ++seq, t: Date.now() / 1000 }, ...this.list].slice(0, MAX);
    this.unread++;
  }
  seen() {
    this.unread = 0;
  }
  remove(id: number) {
    this.list = this.list.filter((n) => n.id !== id);
  }
  clear() {
    this.list = [];
    this.unread = 0;
  }
}

export const notifications = new Notifications();
