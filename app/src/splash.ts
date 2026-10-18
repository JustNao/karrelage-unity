import { listen } from "@tauri-apps/api/event";

/** Splash window: mirrors the main window's loading progress (src/main.ts emits "splash:progress"). */
const fill = document.querySelector<HTMLElement>(".fill")!;
void listen<number>("splash:progress", (e) => (fill.style.width = `${e.payload}%`));
