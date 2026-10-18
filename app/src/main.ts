import { mount } from "svelte";
import { invoke } from "@tauri-apps/api/core";
import { emitTo } from "@tauri-apps/api/event";
import "./app.css";
import App from "./App.svelte";
import { getStatus, idle, on, type Status } from "./lib/api";

/** This window starts hidden behind the splash window (splash.html); it is shown once the data pack is loaded
 *  and the first view has fetched its data. No requestAnimationFrame here: it does not fire in a hidden window. */
let pct = 0;
const step = (p: number) => {
  pct = Math.max(pct, p);
  void emitTo("splash", "splash:progress", pct);
};
const timeout = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** The backend loads the data pack in a thread; resolves once it is done (ready or failed). */
async function backendLoaded() {
  const done = (s: Status) => !s.loading;
  // a failing call (backend not ready yet) counts as "still loading", it must never abort the startup
  const status = () => getStatus().catch((): Status => ({ ready: false, loading: true, log: [] }));
  if (done(await status())) return;
  // the loading time is unknown: creep towards 70 % so the bar keeps moving
  const creep = setInterval(() => step(pct + (70 - pct) * 0.08), 120);
  await new Promise<void>((resolve) => {
    let off: (() => void) | undefined;
    const poll = setInterval(async () => done(await status()) && finish(), 500); // in case the event fired before we listened
    const finish = () => (clearInterval(poll), off?.(), resolve());
    void on<Status>("app:status", (s) => done(s) && finish()).then((f) => (off = f));
  });
  clearInterval(creep);
}

async function start() {
  try {
    step(15);
    await backendLoaded();
    step(80);
  } catch (e) {
    console.error("startup:", e);
  }
  // whatever happened above, show the UI and dismiss the splash: a black window helps nobody
  try {
    mount(App, { target: document.getElementById("app")! });
    await timeout(0); // let the views mount and issue their first calls
    await Promise.race([idle(), timeout(8000)]);
    step(100);
    await timeout(250); // let the bar reach the end
  } finally {
    await invoke("splash_done");
  }
}

void start();
