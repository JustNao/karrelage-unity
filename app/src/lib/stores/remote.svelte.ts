/**
 * The sync server (crates/modules/src/remote.rs, Paramètres « Serveur de synchronisation »): the backend is given the setting
 * at startup and on every change, and reports each sync in `remoteStatus`.
 *
 *   startRemote();                            // App.svelte, once the backend is ready
 *   remoteStatus.value                        // last sync: {ok, last, sent, received, bytes_up, bytes_down, error?} | null
 *   await testRemote(url, password)           // {ok, version, ms}, throws the reason
 */
import { call, on } from "../api";
import { settings } from "./settings.svelte";

export interface RemoteStatus { ok: boolean; last: number; server?: number; sent?: number; received?: number; bytes_up?: number; bytes_down?: number; error?: string }

export const remoteStatus = $state<{ value: RemoteStatus | null }>({ value: null });

/** Intervals offered in the settings (seconds). */
export const INTERVALS: { id: number; label: string }[] = [
  { id: 60, label: "1 min" },
  { id: 300, label: "5 min" },
  { id: 900, label: "15 min" },
  { id: 1800, label: "30 min" },
  { id: 3600, label: "1 h" },
  { id: 21600, label: "6 h" },
  { id: 43200, label: "12 h" },
  { id: 86400, label: "24 h" },
];

export const testRemote = (url: string, password: string) => call<{ ok: boolean; version: string; ms: number }>("remote", "test", { url, password }, true);
export const syncNow = () => call("remote", "sync_now", {}, true);

let started = false;
export function startRemote() {
  if (started) return;
  started = true;
  void on<RemoteStatus>("remote:status", (s) => (remoteStatus.value = s));
  void call<RemoteStatus | null>("remote", "status", {}, true).then((s) => s && (remoteStatus.value = s)).catch(() => {});
  $effect.root(() => {
    $effect(() => {
      const r = settings.general.remote;
      const args = { enabled: r.enabled && !!r.url.trim(), url: r.url, password: r.password, interval: r.interval };
      // sent once the typing stops (a change syncs at once: not on every key of the address)
      const t = setTimeout(() => call("remote", "configure", args, true).catch(() => {}), 800);
      return () => clearTimeout(t);
    });
  });
}
