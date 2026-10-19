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
