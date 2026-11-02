/**
 * User settings (Paramètres tab; the combat ones in the Automation tab), one section per tab, remembered in localStorage `karrelage.settings`. Reactive: views
 * and helpers read `settings.hdv.goodZ` and follow changes at once.
 *
 * To add a setting: give it a default in DEFAULTS (missing keys of older saves fall back to it) and a field in the tab.
 */
import { persisted } from "../persist.svelte";

const KEY = "karrelage.settings";

const DEFAULTS = {
  general: {
    /** Closing the window keeps the app running in the notification area (right click on its icon: open / quit). */
    closeToTray: false,
  },
  hdv: {
    /** A lot this many residual standard deviations above the price / density trend is a good deal. */
    goodZ: 2,
  },
  combat: {
    /** Follow the monsters of the current fight (off: the fight messages are not even decoded). */
    enabled: true,
    /** Notes shown per monster: none (tags only), warnings only, + invulnerabilities, + tips. */
    show: "all" as CombatShow,
    /** Floating (always on top) fight window: shown while a fight runs, closed when it ends. */
    overlay: false,
    /** Which way the floating window grows with its content: its top edge stays put ("down") or its bottom edge ("up"). */
    overlayGrow: "down" as "down" | "up",
  },
  /** Global keyboard shortcuts (lib/shortcuts.ts), one per name: work while the game has the focus. */
  shortcuts: {
    /** In-game line of sight over the fight (FightMapOverlay.svelte). */
    los: { mods: 2 | 4, vk: 0x54, label: "Ctrl+Maj+T" } as Binding,
  },
};
/** A shortcut: `mods` = Alt 1 | Ctrl 2 | Shift 4 | Win 8, `vk` = Windows virtual key code (0 = none), `label` as shown. */
export interface Binding {
  mods: number;
  vk: number;
  label: string;
}
export type CombatShow = "none" | "warning" | "invulnerable" | "all";
export type Settings = typeof DEFAULTS;
export const SETTING_DEFAULTS: Settings = DEFAULTS;

/** Shared with the floating windows (`sync`): a setting changed in one applies everywhere. */
export const settings = persisted<Settings>(KEY, DEFAULTS, { sections: true, sync: true });
