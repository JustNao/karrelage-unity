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
