import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { call, on } from "../api";
import { openFloating } from "../floating";
import type { FmItem } from "../forgemagie";
import { settings } from "./settings.svelte";

/**
 * Main window side of the forgemagie overlay (ForgemagieOverlay.svelte, forgemagie.html): the backend follows the workshop
 * while the feature is on (Automation « Forgemagie »), and the overlay window is made (hidden: it shows itself once loaded)
 * when an equipment piece is put in a forgemagie workshop. The window closes itself when the item goes.
 */
const OVERLAY = "forgemagie";

async function ensure(item: FmItem | null) {
  if (!item || !settings.forgemagie.enabled || (await WebviewWindow.getByLabel(OVERLAY))) return;
  await openFloating(OVERLAY, { url: "forgemagie.html", title: "Forgemagie", width: 800, height: 600, focus: false, visible: false, clearBackground: true });
}

let started = false;
export async function startForgemagie() {
  if (started) return;
  started = true;
  $effect.root(() => {
    $effect(() => {
      const on = settings.forgemagie.enabled;
      call("forgemagie", "set_enabled", { on }, true).catch(() => {});
    });
  });
  await on<FmItem | null>("forgemagie:item", (i) => void ensure(i).catch(() => {}));
}
