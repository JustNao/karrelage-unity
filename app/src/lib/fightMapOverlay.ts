import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { emitTo } from "@tauri-apps/api/event";
import { call, on } from "./api";
import { openFloating } from "./floating";
import type { FightMapState } from "./fightMap";

/**
 * The in-game view's window (FightMapOverlay.svelte) seen from the main window, the only one allowed to create windows: made
 * hidden when a fight starts so that a shortcut draws a view at once instead of loading a window; it closes itself at the
 * fight's end.
 */

/** What the in-game view shows: the line of sight, the class help (named like their shortcut), or a portal simulated under
 *  the mouse (the Eliotrope's passive portal key). */
export type OverlayView = "los" | "classHelp" | "portals";

/** A view's shortcut was pressed (`hold`: shown while it is held). */
export interface OverlayPress {
  view: OverlayView;
  hold: boolean;
}

const OVERLAY = "fightmap";
/** the window listens (it said `fightmap:ready`) */
let ready = false;
/** a press waiting for the window */
let pending: OverlayPress | null = null;
/** the window was asked for in this fight (so fighters' moves don't look it up again) */
let made = false;

async function ensure(s: FightMapState) {
  if (!s.fight) {
    ready = made = false;
    pending = null;
    return;
  }
  if (made || s.map === null) return;
  made = true;
  if (await WebviewWindow.getByLabel(OVERLAY)) return;
  await openFloating(OVERLAY, { url: "fightmap.html", title: "Vue en jeu", width: 800, height: 600, focus: false, visible: false, clearBackground: true });
}

/** A view's shortcut was pressed, or the fight popup's « LdV » button clicked: tells the window (a view shown: it closes when
 *  it is that view, else switches to it; nothing shown: it shows that view). During a fight, a window still loading (or gone)
 *  gets the press once ready; pressed again meanwhile: forgotten. */
export async function pressFightMapOverlay(view: OverlayView, hold: boolean) {
  if (ready) return emitTo(OVERLAY, "fightmap:press", { view, hold });
  const s = await call<FightMapState>("fightmap", "state", {}, true);
  if (!s.fight || s.map === null) return;
  pending = pending?.view === view ? null : { view, hold };
  made = false;
  await ensure(s);
}

/** Main window, once: follows the fights to keep the window ready during each. */
export async function prewarmFightMapOverlay() {
  await on("fightmap:ready", () => {
    ready = true;
    const p = pending;
    pending = null;
    if (p) void emitTo(OVERLAY, "fightmap:press", p);
  });
  await on<FightMapState>("fightmap:state", (s) => void ensure(s).catch(() => {}));
  void call<FightMapState>("fightmap", "state", {}, true).then(ensure).catch(() => {});
}
