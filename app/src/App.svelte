<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "./lib/app.svelte";
  import { call, captureSet, on, restartApp } from "./lib/api";
  import { modules } from "./lib/registry";

  let current = $state(modules.find((m) => !m.devOnly)?.id ?? modules[0]?.id);
  const tabs = $derived(modules.filter((m) => app.dev || !m.devOnly));
  const active = $derived(tabs.find((m) => m.id === current) ?? tabs[0]);
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

  onMount(() => {
    void app.init();
    call<{ available: boolean }>("update", "check", {}, true)
      .then((r) => r.available && (upd = { state: "available", pct: 0, label: "" }))
      .catch(() => {}); // offline or no GitHub: nothing to show
    const off = on<{ phase: string; done: number; total: number; message?: string }>("update:progress", (p) => {
      if (p.phase === "error") upd = { state: "error", pct: 0, label: p.message ?? "échec" };
      else if (p.phase === "done") upd = { state: "ready", pct: 100, label: "" };
      else if (p.phase === "download") upd = { state: "installing", pct: Math.round((p.done / Math.max(p.total, 1)) * 80), label: `Téléchargement ${Math.round((p.done / Math.max(p.total, 1)) * 100)} %` };
      else upd = { state: "installing", pct: 80 + Math.round((p.done / Math.max(p.total, 1)) * 20), label: "Installation…" };
    });
    return () => void off.then((f) => f());
  });

  // restore the capture state the user left at the last close (once, as soon as the backend is ready)
  let restored = false;
  $effect(() => {
    if (restored || !s.ready) return;
    restored = true;
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
        <button class="tab" class:on={active?.id === m.id} onclick={() => (current = m.id)}>{m.title}</button>
      {/each}
    </nav>
    <button class="sw" class:on={s.on} role="switch" aria-checked={!!s.on} aria-label="Activer la capture" disabled={!s.ready} onclick={toggle}></button>
    <span class="state" class:on={s.on}>{s.on ? "ACTIVE" : "INACTIVE"}</span>
    <span class="mut">{s.on ? (s.tracking?.length ? "Connecté à Dofus" : "En attente de Dofus.exe…") : ""}</span>
    <span class="grow"></span>
    {#if upd.state === "available"}
      <span class="upd warn" title="Un pack de données plus récent est publié sur GitHub">⚠ Nouvelles données du jeu disponibles</span>
      <button class="updbtn" onclick={install}>Télécharger et mettre à jour</button>
    {:else if upd.state === "installing"}
      <span class="upd"><span class="ubar"><span class="ufill" style:width="{upd.pct}%"></span></span>{upd.label}</span>
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
    <div class="banner bad">Problème avec le pack de données : {s.error}</div>
  {/if}
  {#if s.npcapError}
    <div class="banner warn">
      La capture nécessite <b>Npcap</b> ({s.npcapError}). Installez-le depuis <b>npcap.com</b>, puis relancez Karrelage.
    </div>
  {/if}
  {#if err}<div class="banner bad">{err}</div>{/if}

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
  .ubar { display: inline-block; width: 140px; height: 8px; border-radius: 4px; background: var(--line); overflow: hidden; }
  .ufill { display: block; height: 100%; background: var(--acc); transition: width 0.25s; }
  .banner { padding: 6px 12px; font-family: var(--font); }
  .banner.bad { background: color-mix(in srgb, var(--bad) 20%, transparent); }
  .banner.warn { background: color-mix(in srgb, var(--warn) 20%, transparent); }
  main { flex: 1; min-height: 0; display: flex; flex-direction: column; }
</style>
