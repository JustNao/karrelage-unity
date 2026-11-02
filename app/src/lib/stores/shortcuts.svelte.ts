import { SvelteMap } from "svelte/reactivity";
import { call, on } from "../api";
import { settings } from "./settings.svelte";
import type { ShortcutName } from "../shortcuts";

/**
 * Global shortcuts (main window only): the bindings of `settings.shortcuts` are pushed to the backend whenever they change,
 * `shortcuts:pressed` runs the handler registered for that name. While a binding is being typed in Paramètres (`capturing`)
 * every shortcut is released, so the keys reach the page.
 */
const handlers = new Map<string, () => void>();
/** Names another program already holds (their binding could not be registered). */
export const taken = new SvelteMap<string, boolean>();
export const shortcuts = $state({ capturing: false });

export const onShortcut = (name: ShortcutName, fn: () => void) => void handlers.set(name, fn);

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
  });
}
