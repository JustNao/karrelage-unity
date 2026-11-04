<script lang="ts">
  import { onMount } from "svelte";
  import { call, listenAll } from "../../lib/api";
  import { fmtInt, fmtNum, fmtSize } from "../../lib/fmt";
  import { settings } from "../../lib/stores/settings.svelte";
  import { RECORD_CATEGORIES, type RecorderStatus } from "../../lib/stores/recorder.svelte";
  import Switch from "../../lib/components/Switch.svelte";
  import Banner from "../../lib/components/Banner.svelte";

  /** Paramètres « Développeur »: the packet categories stored for analysis, with what they weigh on disk. */
  let status = $state<RecorderStatus | null>(null);
  const refresh = () => call<RecorderStatus>("recorder", "status", {}, true).then((s) => (status = s)).catch(() => {});
  onMount(() => {
    const off = listenAll({ "recorder:changed": refresh });
    void off.ready.then(refresh);
    return off;
  });
  const size = (n: number) => (n < 1024 * 1024 ? fmtSize(n) : `${fmtNum(n / (1024 * 1024), 1)} Mo`);
  const stats = (id: string) => status?.categories.find((c) => c.id === id);
</script>

<section>
  <h2 class="with-action">
    Développeur
    <button class="mini" onclick={() => call("recorder", "open_folder").catch(() => {})} title={status?.dir}>Ouvrir le dossier</button>
  </h2>
  {#each RECORD_CATEGORIES as c (c.id)}
    {@const s = stats(c.id)}
    <div class="row">
      <div class="lb">
        <div>{c.title}</div>
        <div class="mut">{c.desc}</div>
        {#if s?.files}
          <div class="mut">{fmtInt(s.files)} {c.unit} · {size(s.bytes)}</div>
        {/if}
        {#if s?.dropped}
          <Banner tone="warn" inset>{fmtInt(s.dropped)} paquets perdus depuis le lancement (marqués dans les fichiers) : l'écriture ne suivait pas.</Banner>
        {/if}
      </div>
      <div class="ctl">
        <Switch bind:checked={settings.record[c.id]} label={c.title} />
      </div>
    </div>
  {/each}
  {#if status?.error}
    <Banner tone="bad" inset>Écriture impossible : {status.error}</Banner>
  {/if}
</section>

<style>
  section { margin-bottom: 20px; }
  h2 { font: 600 12px var(--font); letter-spacing: 0.08em; text-transform: uppercase; color: var(--mut); margin: 4px 0 8px; padding-bottom: 4px; border-bottom: 1px solid var(--line); }
  h2.with-action { display: flex; align-items: center; justify-content: space-between; }
  h2 .mini { text-transform: none; letter-spacing: normal; font-weight: 400; }
  .lb :global(.banner) { margin-top: 6px; }
  .row { display: flex; gap: 20px; align-items: flex-start; justify-content: space-between; padding: 8px 0; }
  .lb .mut { font-size: 12px; margin-top: 2px; }
  .ctl { display: flex; gap: 6px; align-items: center; flex: none; }
</style>
