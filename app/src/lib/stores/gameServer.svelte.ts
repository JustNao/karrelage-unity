/**
 * The player's game server (Paramètres « Général », first-start modal): prices are stored per server (prices.rs), the backend
 * is told the server at startup and on every change.
 *
 *   startGameServer();             // App.svelte, once the backend is ready
 *   await gameServers()            // [{id, name, type}] of the data pack (empty with an older pack)
 *   await confirmGameServer(id)    // the modal: also moves the prices saved before servers existed to that server
 */
import { cached, call } from "../api";
import { settings } from "./settings.svelte";

export interface GameServer { id: number; name: string | null; type: string | null }

/** The pack's game servers, the playable ones first (Classique / Épique / Héroïque...), by name. */
export async function gameServers(): Promise<GameServer[]> {
  const all = await cached<GameServer[]>("prices", "servers").catch(() => [] as GameServer[]);
  return [...all].sort((a, b) => (a.name ?? "").localeCompare(b.name ?? "", "fr"));
}

/** Label of a server in a list: "Draconiros (Classique)". */
export const serverLabel = (s: GameServer) => (s.type ? `${s.name ?? s.id} (${s.type})` : `${s.name ?? s.id}`);

/** The modal's choice: the server is kept, and prices saved before servers existed move to it. */
export async function confirmGameServer(id: number): Promise<number> {
  const r = await call<{ moved: number }>("prices", "set_server", { id, confirm: true });
  settings.general.gameServer = id;
  settings.general.gameServerAsked = true;
  return r.moved;
}

let started = false;
export function startGameServer() {
  if (started) return;
  started = true;
  $effect.root(() => {
    $effect(() => {
      const id = settings.general.gameServer;
      call("prices", "set_server", { id }, true).catch(() => {});
    });
  });
}
