/** One-time move of the browser-storage keys from the former app name ("loutil.*") to "karrelage.*".
 *  Imported first by main.ts so it runs before any module reads its preferences. */
try {
  for (const k of Object.keys(localStorage)) {
    if (!k.startsWith("loutil.")) continue;
    const next = "karrelage." + k.slice("loutil.".length);
    if (localStorage.getItem(next) === null) localStorage.setItem(next, localStorage.getItem(k)!);
    localStorage.removeItem(k);
  }
} catch {}

/** The held portal simulation shortcut (F5, never released) became the Eliotrope's passive portal key (Combat tab). */
try {
  const saved = JSON.parse(localStorage.getItem("karrelage.settings") ?? "null");
  if (saved?.shortcuts?.portals) {
    delete saved.shortcuts.portals;
    localStorage.setItem("karrelage.settings", JSON.stringify(saved));
  }
} catch {}

/** Saved settings merge section by section, so a saved `classHelp.eliotrope` hides the new `exilKey` default. */
try {
  const saved = JSON.parse(localStorage.getItem("karrelage.settings") ?? "null");
  const eliotrope = saved?.classHelp?.eliotrope;
  if (eliotrope && !eliotrope.exilKey) {
    eliotrope.exilKey = { mods: 0, vk: 0, label: "" };
    localStorage.setItem("karrelage.settings", JSON.stringify(saved));
  }
} catch {}

/** 1.10.0's line of sight shortcut Ctrl+Maj+T, never changed by the user, becomes the new default (F3, held). */
try {
  const saved = JSON.parse(localStorage.getItem("karrelage.settings") ?? "null");
  const los = saved?.shortcuts?.los;
  if (los && los.mods === 6 && los.vk === 0x54) {
    delete saved.shortcuts.los;
    localStorage.setItem("karrelage.settings", JSON.stringify(saved));
  }
} catch {}
