import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Call a backend module method (crates/modules). */
export const call = <T = any>(module: string, method: string, args: Record<string, unknown> = {}) =>
  invoke<T>("module_call", { module, method, args });

/** Subscribe to a backend module event ("<module>:<event>"). Returns the unsubscribe function. */
export async function on<T = any>(event: string, fn: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (e) => fn(e.payload));
}

export interface Status {
  ready: boolean;
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
export const devInject = () => invoke<Status>("dev_inject");

/** A price with its unit: "12 345 kamas". */
export const kamas = (n: number) => `${Number(n).toLocaleString("fr-FR")} kamas`;
export const ago = (t: number) => {
  const d = Math.round(Date.now() / 1000 - t);
  return d < 60 ? `${d}s` : d < 3600 ? `${Math.round(d / 60)}min` : `${Math.round(d / 3600)}h`;
};
