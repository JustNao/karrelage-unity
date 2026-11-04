/**
 * Packets stored locally for analysis (crates/modules/src/recorder.rs). A developer tool: a category records only while its
 * setting is on and developer mode too, so a hidden setting never keeps writing to disk; the setting is kept for when
 * developer mode comes back. The backend starts with everything off and is told here.
 */
import { call } from "../api";
import { app } from "../app.svelte";
import { settings, type Settings } from "./settings.svelte";

export type RecordCategory = keyof Settings["record"];

/** One row of the Paramètres tab per backend category (`CATEGORIES`). */
export const RECORD_CATEGORIES: { id: RecordCategory; title: string; desc: string; unit: string }[] = [
  {
    id: "fight",
    title: "Enregistrer les données de combat",
    desc: "Tous les paquets de chaque combat, du début à la fin, pour de l'analyse de données (ni les discussions, ni le groupe). Un fichier par combat, de 0,5 à 3,5 Mo.",
    unit: "combat(s)",
  },
];

export interface RecorderStatus {
  dir: string;
  error: string | null;
  categories: { id: string; on: boolean; files: number; bytes: number; written: number; dropped: number }[];
}

let started = false;
export function startRecorder() {
  if (started) return;
  started = true;
  $effect.root(() => {
    for (const { id } of RECORD_CATEGORIES) {
      $effect(() => {
        const on = app.dev && settings.record[id];
        call("recorder", "set_category", { id, on }, true).catch(() => {});
      });
    }
  });
}
