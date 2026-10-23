/**
 * Floating windows over the game (the scan list, the fight popup): frameless, transparent, always on top, out of the taskbar.
 * Each is its own page (`<name>.html` + an entry in vite.config.ts) whose root is a `FloatingShell`; its label must be listed
 * in app/src-tauri/capabilities/overlay.json.
 *
 *   await openFloating("scanlist", { url: "scanlist.html", title: "À scanner", width: 280, height: 380 });
 *   await closeFloating("scanlist");
 */
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { readJSON } from "./persist.svelte";

export interface FloatingOptions {
  url: string;
  title: string;
  width: number;
  height: number;
  /** Logical position of the top-left corner (default: centred by the system). */
  pos?: { x: number; y: number } | null;
  /** Take the focus (false when it opens by itself during the game). */
  focus?: boolean;
}

/** Open the window, or bring it forward when it is already open (`focus`). True when it was already open. */
export async function openFloating(label: string, o: FloatingOptions): Promise<boolean> {
  const open = await WebviewWindow.getByLabel(label);
  if (open) {
    if (o.focus ?? true) await open.setFocus();
    return true;
  }
  new WebviewWindow(label, {
    url: o.url,
    title: o.title,
    width: o.width,
    height: o.height,
    ...(o.pos ? { x: o.pos.x, y: o.pos.y } : {}),
    decorations: false,
    transparent: true,
    shadow: false,
    alwaysOnTop: true,
    skipTaskbar: true,
    resizable: false,
    focus: o.focus ?? true,
  });
  return false;
}

export async function closeFloating(label: string) {
  await (await WebviewWindow.getByLabel(label))?.close();
}

/** Where the user left a floating window (logical px; `bottom` = its bottom edge), saved by the window when it moves. */
export interface FloatingPos {
  x: number;
  y: number;
  bottom?: number;
}
export const savedPos = (key: string) => readJSON<FloatingPos | null>(key, null);
