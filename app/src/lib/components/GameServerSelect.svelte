<script lang="ts">
  import { onMount } from "svelte";
  import { gameServers, serverLabel, type GameServer } from "../stores/gameServer.svelte";

  /** A game server picker: the pack's servers, or a number (server id) with an older pack that has none. */
  let { value, onchange }: { value: number; onchange: (id: number) => void } = $props();
  let list = $state<GameServer[] | null>(null);
  onMount(() => void gameServers().then((l) => (list = l)));
</script>

{#if list === null}
  <span class="mut">…</span>
{:else if list.length}
  <select {value} onchange={(e) => onchange(+e.currentTarget.value)} aria-label="Serveur de jeu">
    {#each list as s (s.id)}
      <option value={s.id}>{serverLabel(s)}</option>
    {/each}
  </select>
{:else}
  <input type="number" min="1" {value} onchange={(e) => e.currentTarget.valueAsNumber > 0 && onchange(e.currentTarget.valueAsNumber)} aria-label="Numéro du serveur de jeu" title="Numéro du serveur (le pack de données ne liste pas encore les serveurs : 295 = Draconiros)" />
{/if}

<style>
  input { width: 72px; }
</style>
