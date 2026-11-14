<script lang="ts">
  import { settings } from "../../lib/stores/settings.svelte";
  import { INTERVALS, remoteStatus, syncNow, testRemote } from "../../lib/stores/remote.svelte";
  import Switch from "../../lib/components/Switch.svelte";
  import { ago } from "../../lib/fmt";

  /** Paramètres « Général »: the self-hosted sync server (prices shared with its other clients). Rows of View.svelte. */
  let test = $state<{ ok: boolean; text: string } | null>(null);
  let testing = $state(false);

  async function runTest() {
    testing = true;
    test = null;
    try {
      const t = await testRemote(settings.general.remote.url, settings.general.remote.password);
      test = { ok: true, text: `Connecté (serveur ${t.version}, ${t.ms} ms)` };
    } catch (e) {
      test = { ok: false, text: String(e).replace(/^Error: /, "") };
    } finally {
      testing = false;
    }
  }
  const kb = (b?: number) => (b == null ? "?" : b < 1024 ? `${b} o` : `${(b / 1024).toFixed(1)} Ko`);
  const st = $derived(remoteStatus.value);
</script>

<div class="row">
  <div class="lb">
    <div>Serveur de synchronisation</div>
    <div class="mut">
      Partage les prix relevés à l'hôtel de vente et les moyennes avec les autres joueurs du même serveur Karrelage (auto-hébergé).
      Seuls les changements sont échangés ; les prix manuels restent sur ce PC. Le plus récent des prix (le vôtre ou celui reçu) est utilisé.
    </div>
  </div>
  <div class="ctl">
    <Switch bind:checked={settings.general.remote.enabled} label="Serveur de synchronisation" />
  </div>
</div>
{#if settings.general.remote.enabled}
  <div class="sub">
    <label>Adresse <input type="url" placeholder="http://mon-serveur:8787" bind:value={settings.general.remote.url} spellcheck="false" /></label>
    <label>Mot de passe <input type="password" bind:value={settings.general.remote.password} autocomplete="off" /></label>
    <label>
      Fréquence
      <select value={settings.general.remote.interval} onchange={(e) => (settings.general.remote.interval = +e.currentTarget.value)}>
        {#each INTERVALS as i (i.id)}<option value={i.id}>{i.label}</option>{/each}
      </select>
    </label>
    <div class="actions">
      <button class="mini" onclick={runTest} disabled={testing || !settings.general.remote.url.trim()}>{testing ? "Test…" : "Tester la connexion"}</button>
      <button class="mini" onclick={() => void syncNow()} disabled={!settings.general.remote.url.trim()} title="Synchroniser maintenant sans attendre">Synchroniser</button>
      {#if test}<span class:ok={test.ok} class:bad={!test.ok}>{test.text}</span>{/if}
    </div>
    {#if st}
      <div class="mut status">
        {#if st.ok}
          Dernière synchro {ago(st.last)} : {st.sent ?? 0} prix envoyés, {st.received ?? 0} reçus ({kb(st.bytes_up)} ↑, {kb(st.bytes_down)} ↓)
        {:else}
          <span class="bad">Échec de la synchro {ago(st.last)} : {st.error}</span> (les prix seront renvoyés à la suivante)
        {/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  .row { display: flex; gap: 20px; align-items: flex-start; justify-content: space-between; padding: 8px 0; }
  .lb .mut { font-size: 12px; margin-top: 2px; }
  .ctl { display: flex; gap: 6px; align-items: center; flex: none; }
  .sub { display: flex; flex-direction: column; gap: 6px; padding: 0 0 8px 12px; border-left: 2px solid var(--line); margin-bottom: 4px; }
  .sub label { display: flex; align-items: center; gap: 8px; font-size: 12px; }
  .sub label input { flex: 1; max-width: 320px; }
  .actions { display: flex; gap: 8px; align-items: center; font-size: 12px; flex-wrap: wrap; }
  .status { font-size: 12px; }
  .ok { color: var(--ok); }
</style>
