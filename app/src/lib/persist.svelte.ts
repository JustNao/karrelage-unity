/**
 * Browser storage (localStorage) for UI preferences: every access goes through here, so none can throw (private mode,
 * blocked storage, thumbnails) and none parses by hand. Keys are named `karrelage.<module>.<what>`; renaming one needs a
 * step in lib/migrate.ts or the saved value is lost. State that must survive reliably or be shared with the backend
 * belongs in a module store (crates/modules `kit::Store`), not here.
 *
 *   readJSON("karrelage.stuff.autofilled", [] as string[])      // one-off read, with a fallback
 *   writeJSON("karrelage.stuff.autofilled", ids)
 *   readFlag("karrelage.dev", false) / writeFlag(key, on)        // "1" / "0" booleans
 *   const ui = persisted("karrelage.brisage.ui", { q: "", ... }); // reactive object, saved on every change
 *   const settings = persisted(KEY, DEFAULTS, { sections: true, sync: true });
 */
import { onDestroy } from "svelte";

export function readJSON<T>(key: string, fallback: T): T {
  try {
    const v = localStorage.getItem(key);
    return v === null ? fallback : (JSON.parse(v) as T);
  } catch {
    return fallback;
  }
}

export function writeJSON(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {}
}

export function removeKey(key: string) {
  try {
    localStorage.removeItem(key);
  } catch {}
}

export function readFlag(key: string, fallback: boolean): boolean {
  try {
    const v = localStorage.getItem(key);
    return v === null ? fallback : v === "1";
  } catch {
    return fallback;
  }
}

export function writeFlag(key: string, on: boolean) {
  try {
    localStorage.setItem(key, on ? "1" : "0");
  } catch {}
}

/** Called with the new value when another window of the app writes `key` (the `storage` event). Returns the unsubscribe. */
export function onStored<T>(key: string, fn: (value: T | null) => void): () => void {
  const h = (e: StorageEvent) => e.key === key && fn(e.newValue === null ? null : readJSON<T | null>(key, null));
  window.addEventListener("storage", h);
  return () => window.removeEventListener("storage", h);
}

interface PersistOptions {
  /** `defaults` is made of sections ({hdv: {...}, combat: {...}}): saved sections are merged into them one by one, so a key
   *  added to the defaults later still gets its default value with an older save. Without it, a flat merge. */
  sections?: boolean;
  /** Follow the changes other windows make to the same key (settings shared with the floating windows). */
  sync?: boolean;
}

/**
 * Reactive object stored under `key`: loaded once (saved values over `defaults`), written back whenever it changes (deep).
 * At module level it lives for the session; created while a component initialises, it stops saving with the component.
 */
export function persisted<T extends Record<string, any>>(key: string, defaults: T, opts: PersistOptions = {}): T {
  const merge = (into: T, saved: any) => {
    if (!saved || typeof saved !== "object") return;
    if (!opts.sections) return void Object.assign(into, saved);
    for (const s of Object.keys(into) as (keyof T)[]) if (saved[s] && typeof saved[s] === "object") Object.assign(into[s], saved[s]);
  };
  const initial = structuredClone(defaults);
  merge(initial, readJSON(key, null));
  const state = $state(initial);

  const stop = $effect.root(() => {
    $effect(() => {
      const json = JSON.stringify(state); // read deeply: every change saves
      try {
        localStorage.setItem(key, json);
      } catch {}
    });
    if (!opts.sync) return;
    return onStored(key, (saved) => {
      const next = structuredClone(defaults);
      merge(next, saved);
      merge(state, next);
    });
  });
  try {
    onDestroy(stop);
  } catch {
    // module level: kept for the session
  }
  return state;
}
