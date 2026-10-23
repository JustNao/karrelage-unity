import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

let pending = 0;
let settled: (() => void)[] = [];

/** Call a backend module method (crates/modules). `background` calls (e.g. the update check) are not waited for by `idle()`. */
export function call<T = any>(module: string, method: string, args: Record<string, unknown> = {}, background = false): Promise<T> {
  const p = invoke<T>("module_call", { module, method, args });
  if (background) return p;
  pending++;
  return p.finally(() => {
    if (--pending === 0) settled.splice(0).forEach((f) => f());
  });
}

/** Resolves once no foreground `call` is in flight (the splash waits for the first view's data). */
export const idle = () => (pending === 0 ? Promise.resolve() : new Promise<void>((r) => settled.push(r)));

/** Subscribe to a backend module event ("<module>:<event>"). Returns the unsubscribe function. */
export async function on<T = any>(event: string, fn: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (e) => fn(e.payload));
}

/**
 * Several backend events at once, for a view: returns one synchronous cleanup (what `onMount` returns), which also drops
 * the listeners still being registered; `.ready` resolves once they all listen (load a snapshot after it, so nothing
 * falls between the snapshot and the live events).
 *
 *   onMount(() => listenAll({ "brisage:coef": (e) => ..., "brisage:pending": (l) => (pending = l) }));
 */
export function listenAll(handlers: Record<string, (payload: any) => void>): (() => void) & { ready: Promise<void> } {
  const offs = Object.entries(handlers).map(([event, fn]) => on(event, fn));
  const off = () => offs.forEach((o) => void o.then((f) => f()));
  return Object.assign(off, { ready: Promise.all(offs).then(() => {}) });
}

const cache = new Map<string, Promise<unknown>>();
/**
 * A backend answer that never changes during the session (pack-derived: recipes, runes, breakable items...), fetched once
 * and shared by every view. A failed call is not kept (asked again next time).
 */
export function cached<T>(module: string, method: string): Promise<T> {
  const k = `${module}.${method}`;
  let p = cache.get(k) as Promise<T> | undefined;
  if (!p) {
    p = call<T>(module, method);
    cache.set(k, p);
    p.catch(() => cache.delete(k));
  }
  return p;
}

export interface Status {
  ready: boolean;
  /** the data pack is still being loaded (startup) */
  loading?: boolean;
  on?: boolean;
  tracking?: string[];
  error?: string | null;
  npcapError?: string | null;
  log: string[];
  modules?: string[];
  pack?: string;
  packGenerated?: number;
  mapped?: number;
}

export const getStatus = () => invoke<Status>("status");
export const captureSet = (on: boolean) => invoke<Status>("capture_set", { on });
export const restartApp = () => invoke("restart_app");
export const quitApp = () => invoke("quit_app");
