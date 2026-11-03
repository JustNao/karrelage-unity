/**
 * Global keyboard shortcuts (crates/modules/src/shortcuts.rs): bound with Windows, so they work while the game has the focus
 * (the game does not get them). Each has a name, a binding in `settings.shortcuts` (Paramètres « Raccourcis ») and a handler
 * registered with `onShortcut` (lib/stores/shortcuts.svelte.ts).
 */
import type { Binding, Settings } from "./stores/settings.svelte";

export type ShortcutName = keyof Settings["shortcuts"];

/** What each shortcut does, in the order of the Paramètres tab. */
export const SHORTCUTS: { name: ShortcutName; title: string; desc: string }[] = [
  {
    name: "los",
    title: "Ligne de vue en jeu",
    desc:
      "Pendant un combat, tant que la touche est enfoncée : survolez une case du jeu pour voir les cases en ligne de vue. " +
      "Un clic fige la case : la vue reste après avoir lâché la touche (vous pouvez alors jouer), jusqu'au raccourci, Échap ou la fin de votre tour. " +
      "Au premier usage (ou si la fenêtre du jeu change de taille), cliquez sur les deux combattants indiqués pour calibrer la grille.",
  },
  {
    name: "classHelp",
    title: "Aide de classe en jeu",
    desc:
      "Pendant un combat, tant que la touche est enfoncée. Steamer : la zone de chaque tourelle (7 PO et en ligne de vue) " +
      "où votre personnage reçoit son bonus ; la souris sur une tourelle met sa zone en avant. " +
      "Éliotrope : vos portails numérotés, et avec 4 portails les deux qui partiront en premier en couleur. Même calibration que la ligne de vue.",
  },
  {
    name: "portals",
    title: "Simulation de portail en jeu",
    desc:
      "Éliotrope, pendant un combat, tant que la touche est enfoncée : un portail est simulé sur la case sous la souris. " +
      "Avec 4 portails, le plus ancien disparaît (barré) ; la redirection depuis le nouveau portail est tracée jusqu'à sa sortie, " +
      "avec la distance parcourue. Sur un de vos portails : la redirection depuis celui-ci.",
  },
];

export const NO_BINDING: Binding = { mods: 0, vk: 0, label: "" };

/** Keys that are only modifiers: never a shortcut on their own. */
const MODIFIERS = new Set(["Alt", "Control", "Shift", "Meta", "AltGraph", "CapsLock"]);

/** The binding of a key press (null: a modifier alone). `keyCode` is the Windows virtual key code of the pressed key in the
 *  current keyboard layout (what RegisterHotKey expects: Z on AZERTY is VK_Z). */
export function bindingOf(e: KeyboardEvent): Binding | null {
  if (MODIFIERS.has(e.key) || !e.keyCode) return null;
  const mods = (e.altKey ? 1 : 0) | (e.ctrlKey ? 2 : 0) | (e.shiftKey ? 4 : 0) | (e.metaKey ? 8 : 0);
  const key = e.key === " " ? "Espace" : e.key.length === 1 ? e.key.toUpperCase() : e.code.startsWith("Numpad") ? `Pavé ${e.code.slice(6)}` : e.key;
  const names = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Maj", e.metaKey && "Win", key].filter(Boolean);
  return { mods, vk: e.keyCode, label: names.join("+") };
}

export const sameBinding = (a: Binding, b: Binding) => a.mods === b.mods && a.vk === b.vk;
