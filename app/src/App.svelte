<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "./lib/app.svelte";
  import { call, captureSet, on, quitApp, restartApp } from "./lib/api";
  import { modules } from "./lib/registry";
  import Banner from "./lib/components/Banner.svelte";
  import ProgressBar from "./lib/components/ProgressBar.svelte";
  import Notifications from "./lib/components/Notifications.svelte";
  import { startAutomation } from "./lib/automation";
  import { startCombat } from "./lib/stores/combat.svelte";

  const tabs = $derived(modules.filter((m) => app.dev || !m.devOnly));
  const active = $derived(tabs.find((m) => m.id === app.tab) ?? tabs[0]);
  const s = $derived(app.status);
  let err = $state("");

  /** Data pack update (see crates/modules/src/update.rs): checked once at startup. */
  type Upd = { state: "none" | "available" | "installing" | "ready" | "error"; pct: number; label: string };
  let upd = $state<Upd>({ state: "none", pct: 0, label: "" });

  async function install() {
    upd = { state: "installing", pct: 0, label: "Téléchargement…" };
    try {
      await call("update", "install");
    } catch (e) {
      upd = { state: "error", pct: 0, label: String(e) };
    }
  }

  /** App update: a newer GitHub release; its installer is downloaded, then run while the app quits (it relaunches it). */
  type AppUpd = { state: "none" | "available" | "downloading" | "ready" | "installing" | "error"; version: string; notes: string; pct: number; label: string };
  let appUpd = $state<AppUpd>({ state: "none", version: "", notes: "", pct: 0, label: "" });

  async function downloadApp() {
    appUpd = { ...appUpd, state: "downloading", pct: 0, label: "Téléchargement…" };
    try {
      await call("update", "app_download");
    } catch (e) {
      appUpd = { ...appUpd, state: "error", label: String(e) };
    }
  }

  async function installApp() {
    appUpd = { ...appUpd, state: "installing", label: "" };
    try {
      await call("update", "app_install");
      await quitApp();
    } catch (e) {
      appUpd = { ...appUpd, state: "error", label: String(e) };
    }
  }

  onMount(() => {
    void app.init();
    call<{ available: boolean; version?: string; notes?: string }>("update", "app_check", {}, true)
      .then((r) => r.available && (appUpd = { state: "available", version: r.version ?? "", notes: r.notes ?? "", pct: 0, label: "" }))
      .catch(() => {});
    const offApp = on<{ phase: string; done?: number; total?: number; message?: string }>("update:app", (p) => {
      if (p.phase === "error") appUpd = { ...appUpd, state: "error", label: p.message ?? "échec" };
      else if (p.phase === "ready") appUpd = { ...appUpd, state: "ready", pct: 100 };
      else {
        const pct = Math.round(((p.done ?? 0) / Math.max(p.total ?? 1, 1)) * 100);
        appUpd = { ...appUpd, state: "downloading", pct, label: `Téléchargement ${pct} %` };
      }
    });
    call<{ available: boolean }>("update", "check", {}, true)
      .then((r) => r.available && (upd = { state: "available", pct: 0, label: "" }))
      .catch(() => {}); // offline or no GitHub: nothing to show
    const off = on<{ phase: string; done: number; total: number; message?: string }>("update:progress", (p) => {
      if (p.phase === "error") upd = { state: "error", pct: 0, label: p.message ?? "échec" };
      else if (p.phase === "done") upd = { state: "ready", pct: 100, label: "" };
      else if (p.phase === "download") upd = { state: "installing", pct: Math.round((p.done / Math.max(p.total, 1)) * 80), label: `Téléchargement ${Math.round((p.done / Math.max(p.total, 1)) * 100)} %` };
      else upd = { state: "installing", pct: 80 + Math.round((p.done / Math.max(p.total, 1)) * 20), label: "Installation…" };
    });
    return () => {
      void off.then((f) => f());
      void offApp.then((f) => f());
    };
  });

  // restore the capture state the user left at the last close (once, as soon as the backend is ready)
  let restored = false;
  $effect(() => {
    if (restored || !s.ready) return;
    restored = true;
    void startAutomation(); // automation rules watch every scan, whatever tab is open (needs the data pack)
    void startCombat(); // the current fight is followed whatever tab is open
    if (app.capture && !s.on && !s.npcapError) captureSet(true).catch((e) => (err = String(e)));
  });

  async function toggle() {
    err = "";
    const want = !s.on; // read before the call: the status event flips s.on
    try {
      await captureSet(want);
      app.setCapture(want);
    } catch (e) {
      err = String(e);
    }
  }
</script>

<div class="shell">
  <header>
    <h1>Karrelage</h1>
    <nav>
      {#each tabs as m (m.id)}
        <button class="tab" class:on={active?.id === m.id} onclick={() => (app.tab = m.id)}>{m.title}</button>
      {/each}
    </nav>
    <button class="sw" class:on={s.on} role="switch" aria-checked={!!s.on} aria-label="Activer la capture" disabled={!s.ready} onclick={toggle}></button>
    <span class="state" class:on={s.on}>{s.on ? "ACTIVE" : "INACTIVE"}</span>
    <span class="mut">{s.on ? (s.tracking?.length ? "Connecté à Dofus" : "En attente de Dofus.exe…") : ""}</span>
    <Notifications />
    <span class="grow"></span>
    {#if appUpd.state === "available"}
      <span class="upd warn" title={appUpd.notes || "Une nouvelle version est publiée sur GitHub"}>⬆ Karrelage {appUpd.version} disponible</span>
      <button class="updbtn" onclick={downloadApp}>Mettre à jour</button>
    {:else if appUpd.state === "downloading"}
      <span class="upd"><ProgressBar value={appUpd.pct / 100} width="140px" />{appUpd.label}</span>
    {:else if appUpd.state === "ready"}
      <span class="upd ok">✓ Karrelage {appUpd.version} téléchargé</span>
      <button class="updbtn" onclick={installApp} title="Ferme Karrelage, installe la nouvelle version puis la relance">Installer et redémarrer</button>
    {:else if appUpd.state === "installing"}
      <span class="upd">Installation…</span>
    {:else if appUpd.state === "error"}
      <span class="upd bad" title={appUpd.label}>⚠ Mise à jour de Karrelage impossible</span>
      <button class="updbtn" onclick={downloadApp}>Réessayer</button>
    {:else if upd.state === "available"}
      <span class="upd warn" title="Un pack de données plus récent est publié sur GitHub">⚠ Nouvelles données du jeu disponibles</span>
      <button class="updbtn" onclick={install}>Télécharger et mettre à jour</button>
    {:else if upd.state === "installing"}
      <span class="upd"><ProgressBar value={upd.pct / 100} width="140px" />{upd.label}</span>
    {:else if upd.state === "ready"}
      <span class="upd ok">✓ Mise à jour installée</span>
      <button class="updbtn" onclick={() => restartApp()}>Redémarrer</button>
    {:else if upd.state === "error"}
      <span class="upd bad" title={upd.label}>⚠ Mise à jour impossible</span>
      <button class="updbtn" onclick={install}>Réessayer</button>
    {/if}
    <label class="mut"><input type="checkbox" checked={app.dev} onchange={(e) => app.setDev(e.currentTarget.checked)} /> Développeur</label>
  </header>

  {#if !s.ready && s.error}
    <Banner>Problème avec le pack de données : {s.error}</Banner>
  {/if}
  {#if s.npcapError}
    <Banner tone="warn">La capture nécessite <b>Npcap</b> ({s.npcapError}). Installez-le depuis <b>npcap.com</b>, puis relancez Karrelage.</Banner>
  {/if}
  {#if err}<Banner>{err}</Banner>{/if}

  <main>
    {#if active}
      {#key active.id}
        <active.component />
      {/key}
    {/if}
  </main>
</div>

<style>
  .shell { display: flex; flex-direction: column; height: 100%; }
  header { display: flex; gap: 12px; align-items: center; padding: 8px 12px; background: var(--panel); border-bottom: 1px solid var(--line); flex-wrap: wrap; }
  /* same font family and line box as the tabs so the text centres line up (system-ui sat a pixel or two off the monospace tabs) */
  h1 { font: 600 14px/1.4 var(--font); margin: 0 6px 0 0; padding: 4px 0; }
  header label { display: flex; align-items: center; gap: 6px; line-height: 1.4; padding: 4px 0; }
  header label input { margin: 0; }
  nav { display: flex; gap: 2px; }
  .tab { border: 0; background: none; color: var(--mut); padding: 4px 12px; }
  .tab.on { background: var(--bg); color: var(--fg); font-weight: 600; outline: 1px solid var(--line); }
  .sw { position: relative; width: 44px; height: 24px; padding: 0; background: var(--line); border-radius: 12px; transition: 0.15s; flex: none; border: 0; }
  .sw::after { content: ""; position: absolute; top: 3px; left: 3px; width: 18px; height: 18px; border-radius: 50%; background: #fff; transition: 0.15s; }
  .sw.on { background: var(--ok); }
  .sw.on::after { left: 23px; }
  .state { font-weight: 600; color: var(--mut); }
  .state.on { color: var(--ok); }
  .grow { flex: 1; }
  .upd { display: inline-flex; align-items: center; gap: 8px; line-height: 1.4; }
  .upd.warn { color: var(--warn); }
  .upd.ok { color: var(--ok); }
  .upd.bad { color: var(--bad); }
  .updbtn { border-color: var(--acc); color: var(--acc); }
  main { flex: 1; min-height: 0; display: flex; flex-direction: column; }
</style>
