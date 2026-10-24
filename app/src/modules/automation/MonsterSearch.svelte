<script lang="ts">
  import { cached } from "../../lib/api";
  import { clickOutside } from "../../lib/actions/clickOutside";
  import { norm } from "../../lib/effects";
  import type { MonsterName } from "../../lib/automation";

  /** Searched monsters of the Monstres rule: a box suggesting the pack's monsters while typing (accents, case and
   *  punctuation ignored), the picked ones as removable chips. `gids` is replaced on every change (saved). */
  let { gids = $bindable() }: { gids: number[] } = $props();

  const MAX = 8;
  /** "Chef de Guerre Bouftou" / "chef-de-guerre bouftou" -> "chef de guerre bouftou". */
  const key = (s: string) => norm(s).replace(/[^a-z0-9]+/g, " ").trim();

  let all = $state<(MonsterName & { k: string })[]>([]);
  cached<MonsterName[]>("monsters", "list")
    .then((l) => (all = l.map((m) => ({ ...m, k: key(m.name) })).sort((a, b) => a.name.localeCompare(b.name, "fr"))))
    .catch(() => {});
  const names = $derived(new Map(all.map((m) => [m.gid, m.name])));

  let q = $state("");
  let open = $state(false);
  let active = $state(0);
  const hits = $derived.by(() => {
    const n = key(q);
    if (!n) return [];
    const picked = new Set(gids);
    const l = all.filter((m) => !picked.has(m.gid) && m.k.includes(n));
    // names starting with what was typed first
    return [...l.filter((m) => m.k.startsWith(n)), ...l.filter((m) => !m.k.startsWith(n))].slice(0, MAX);
  });

  function add(m: MonsterName) {
    gids = [...gids, m.gid];
    q = "";
    active = 0;
  }
  const remove = (g: number) => (gids = gids.filter((x) => x !== g));

  function keydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown") active = Math.min(active + 1, hits.length - 1);
    else if (e.key === "ArrowUp") active = Math.max(active - 1, 0);
    else if (e.key === "Enter" && hits[active]) add(hits[active]);
    else if (e.key === "Escape") open = false;
    else return;
    e.preventDefault();
  }
</script>

<div class="ms">
  <div class="box" use:clickOutside={() => (open = false)}>
    <input
      type="search"
      placeholder="Ajouter un monstre…"
      bind:value={q}
      oninput={() => ((open = true), (active = 0))}
      onfocus={() => (open = true)}
      onkeydown={keydown}
    />
    {#if open && hits.length}
      <ul class="sugg">
        {#each hits as m, i (m.gid)}
          <li><button class:on={i === active} onmouseenter={() => (active = i)} onclick={() => add(m)}>{m.name}</button></li>
        {/each}
      </ul>
    {/if}
  </div>
  {#each gids as g (g)}
    <span class="chip">{names.get(g) ?? `Monstre #${g}`}<button title="Retirer" onclick={() => remove(g)}>×</button></span>
  {:else}
    <span class="mut">Aucun monstre recherché</span>
  {/each}
</div>

<style>
  .ms { display: flex; flex-wrap: wrap; gap: 6px; align-items: center; }
  .box { position: relative; }
  input { width: 200px; padding: 3px 6px; }
  .sugg { position: absolute; z-index: 20; top: 100%; left: 0; min-width: 100%; margin: 2px 0 0; padding: 2px 0; list-style: none; background: var(--panel); border: 1px solid var(--line); border-radius: 4px; box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3); }
  .sugg button { display: block; width: 100%; text-align: left; padding: 3px 8px; background: none; border: 0; border-radius: 0; white-space: nowrap; color: inherit; font: inherit; }
  .sugg button.on { background: var(--line); }
  .chip { display: inline-flex; align-items: center; gap: 4px; padding: 1px 4px 1px 8px; border: 1px solid var(--line); border-radius: 10px; font-size: 12px; }
  .chip button { padding: 0 4px; background: none; border: 0; color: var(--mut); font: inherit; line-height: 1; }
  .chip button:hover { color: inherit; }
</style>
