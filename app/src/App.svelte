<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "./lib/app.svelte";
  import { captureSet, devInject } from "./lib/api";
  import { modules } from "./lib/registry";

  let current = $state(modules.find((m) => !m.devOnly)?.id ?? modules[0]?.id);
  const tabs = $derived(modules.filter((m) => app.dev || !m.devOnly));
  const active = $derived(tabs.find((m) => m.id === current) ?? tabs[0]);
  const s = $derived(app.status);
  let err = $state("");

  onMount(() => void app.init());

  async function toggle() {
    err = "";
    try {
      await captureSet(!s.on);
    } catch (e) {
      err = String(e);
    }
  }
</script>

<div class="shell">
  <header>
    <h1>Loutil</h1>
    <nav>
      {#each tabs as m (m.id)}
        <button class="tab" class:on={active?.id === m.id} onclick={() => (current = m.id)}>{m.title}</button>
      {/each}
    </nav>
    <button class="sw" class:on={s.on} role="switch" aria-checked={!!s.on} aria-label="Toggle capture" disabled={!s.ready} onclick={toggle}></button>
    <span class="state" class:on={s.on}>{s.on ? "ON" : "OFF"}</span>
    <span class="mut">{s.on ? (s.tracking?.length ? "Connected to Dofus" : "Waiting for Dofus.exe…") : ""}</span>
    <span class="grow"></span>
    {#if app.dev}<button onclick={() => devInject()} title="Push the bundled synthetic auction packet through the pipeline">Inject demo</button>{/if}
    <label class="mut"><input type="checkbox" checked={app.dev} onchange={(e) => app.setDev(e.currentTarget.checked)} /> Developer</label>
  </header>

  {#if !s.ready && s.error}
    <div class="banner bad">Data pack problem: {s.error}</div>
  {/if}
  {#if s.npcapError}
    <div class="banner warn">
      Capture needs <b>Npcap</b> ({s.npcapError}). Install it from <b>npcap.com</b>, then restart Loutil.
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
  h1 { font: 600 14px system-ui; margin: 0 6px 0 0; }
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
  .banner { padding: 6px 12px; font-family: system-ui, sans-serif; }
  .banner.bad { background: color-mix(in srgb, var(--bad) 20%, transparent); }
  .banner.warn { background: color-mix(in srgb, var(--warn) 20%, transparent); }
  main { flex: 1; min-height: 0; display: flex; flex-direction: column; }
</style>
