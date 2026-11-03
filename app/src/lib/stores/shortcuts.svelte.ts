import { SvelteMap } from "svelte/reactivity";
import { call, on } from "../api";
import { settings } from "./settings.svelte";
import type { PassiveName, ShortcutName } from "../shortcuts";

/**
 * Global shortcuts (main window only): the bindings of `settings.shortcuts` are pushed to the backend whenever they change,
 * `shortcuts:pressed` runs the handler registered for that name. While a binding is being typed in Paramètres (`capturing`)
 * every shortcut is released, so the keys reach the page. Passive shortcuts (`passiveBinds`) are only watched: the game still
 * gets their keys (the Eliotrope's portal key is the game's own spell key).
 */
const handlers = new Map<string, () => void>();
/** Names another program already holds (their binding could not be registered). */
export const taken = new SvelteMap<string, boolean>();
export const shortcuts = $state({ capturing: false });

export const onShortcut = (name: ShortcutName | PassiveName, fn: () => void) => void handlers.set(name, fn);

/** The passive shortcuts' bindings (null: not watched). */
const passiveBinds = (): Record<PassiveName, { mods: number; vk: number } | null> => {
  const e = settings.classHelp.eliotrope;
  return { portalKey: e.enabled && e.portalKey.vk ? { mods: e.portalKey.mods, vk: e.portalKey.vk } : null };
};

let started = false;
export async function startShortcuts() {
  if (started) return;
  started = true;
  await on<{ name: string }>("shortcuts:pressed", ({ name }) => handlers.get(name)?.());
  await on<{ name: string; ok: boolean }>("shortcuts:status", ({ name, ok }) => taken.set(name, !ok));
  $effect.root(() => {
    $effect(() => {
      const free = shortcuts.capturing;
      const binds = Object.fromEntries(Object.entries(settings.shortcuts).map(([n, b]) => [n, free ? null : { mods: b.mods, vk: b.vk }]));
      call("shortcuts", "set", { binds }, true).catch(() => {});
    });
    $effect(() => {
      const binds = shortcuts.capturing ? {} : passiveBinds();
      call("shortcuts", "passive", { binds }, true).catch(() => {});
    });
  });
}
