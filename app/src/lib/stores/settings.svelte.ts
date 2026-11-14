/**
 * User settings (Paramètres tab; the fight following in the Combat tab, the forgemagie one in the Automation tab), one section per tab, remembered in localStorage `karrelage.settings`. Reactive: views
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
    /** The player's game server (prices are stored per server, prices.rs); 295 = Draconiros. */
    gameServer: 295,
    /** The first-start modal asked it (GameServerModal.svelte). */
    gameServerAsked: false,
    /** Self-hosted sync server (crates/server, lib/stores/remote.svelte.ts): prices shared with its other clients. */
    remote: {
      enabled: false,
      /** http(s)://host:port of the server */
      url: "",
      /** the server's password (requests are signed with it: it never travels) */
      password: "",
      /** seconds between two syncs */
      interval: 60,
    },
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
    /** Comte Harebourg: the cell a spell lands on in the in-game view (fightmap/SpellCellLayer.svelte). */
    harebourgCell: true,
    /** The line of sight view (F3) writes each enemy's weakest element on it (fightmap/ResistLayer.svelte). */
    weakElement: true,
    /** ... during the whole fight, without the shortcut (only while the game is in the foreground). */
    weakElementAlways: false,
  },
  /** Automation « Forgemagie »: the overlay over the game's forgemagie window (ForgemagieOverlay.svelte). */
  forgemagie: {
    /** Follow the workshop and show the overlay's button when an item is put in (off: its messages are not even decoded). */
    enabled: true,
  },
  /** Developer mode only (lib/stores/recorder.svelte.ts): which categories of packets are stored locally for analysis. */
  record: {
    /** Every fight, from its start to its end. */
    fight: false,
    /** Every workshop session with a craft result (forgemagie included), from the workshop's use to leaving it. */
    forgemagie: false,
    /** Every auction house visit (purchases included), from opening its window to leaving it. */
    hdv: false,
  },
  /** Global keyboard shortcuts (lib/shortcuts.ts), one per name: work while the game has the focus. */
  shortcuts: {
    /** In-game line of sight over the fight while held (FightMapOverlay.svelte). */
    los: { mods: 0, vk: 0x72, label: "F3" } as Binding,
    /** Class help over the fight while held (fightmap/ClassLayer.svelte: the Steamer's turrets, the Eliotrope's portals). */
    classHelp: { mods: 0, vk: 0x73, label: "F4" } as Binding,
    /** Comte Harebourg's spell cell alone while held (none: it comes with the class help). */
    spellCell: { mods: 0, vk: 0, label: "" } as Binding,
  },
  /** Class help (Combat tab « Aide de classe »), per class. */
  classHelp: {
    /** The turrets' bonus zones in the class help view. */
    steamer: { enabled: true },
    eliotrope: {
      /** The portals in the class help view, and the portal simulation. */
      enabled: true,
      /** The mouse over one of the player's portals during the fight, with no view: the way and damage bonus from it. */
      hover: true,
      /** The game's own key of the portal spell: watched without being taken from the game (passive shortcut `portalKey`),
       *  it shows a portal simulated under the mouse until pressed again, a click or Escape. */
      portalKey: { mods: 0, vk: 0, label: "" } as Binding,
      /** The same for the Exil spell (passive shortcut `exilKey`): a portal simulated under the fighter under the mouse. */
      exilKey: { mods: 0, vk: 0, label: "" } as Binding,
    },
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
