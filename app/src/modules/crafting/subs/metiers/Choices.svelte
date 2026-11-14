<script lang="ts">
  import SearchBar, { matcher } from "../../../../lib/components/SearchBar.svelte";
  import type { JobRecipe } from "../../../../lib/jobXp";

  /** Under the plan: recipes to add whatever their cost (wished, with a quantity), the ones left out (restorable), and the
   *  jobs whose recipes may make ingredients (« Sous-crafts »: an ingredient of another job is bought otherwise). */
  let {
    recipes,
    name,
    excluded,
    forced,
    jobs,
    subJobs,
    mine,
    onrestore,
    onwish,
    onunwish,
    ontoggle,
  }: {
    recipes: JobRecipe[];
    name: (gid: number) => string;
    excluded: Set<number>;
    forced: Map<number, number>;
    jobs: { id: number; name: string }[];
    subJobs: Set<number>;
    mine: Record<string, { level: number; xp: number }>;
    onrestore: (gid: number) => void;
    onwish: (gid: number, qty: number) => void;
    onunwish: (gid: number) => void;
    ontoggle: (job: number) => void;
  } = $props();

  let q = $state("");
  const found = $derived.by(() => {
    if (!q.trim()) return [];
    const m = matcher(q);
    return recipes.filter((r) => !forced.has(r.gid) && m(name(r.gid))).sort((a, b) => a.level - b.level).slice(0, 12);
  });
</script>

<div class="box">
  <div class="h">Crafts souhaités <span class="mut">faits dès que le niveau du métier le permet, même s'ils ne sont pas les moins chers</span></div>
  <SearchBar bind:value={q} placeholder="Ajouter un objet du métier…" />
  {#if found.length}
    <div class="found">
      {#each found as r (r.gid)}
        <button class="mini" onclick={() => (onwish(r.gid, 1), (q = ""))}>{name(r.gid)} <span class="mut">niv. {r.level}</span></button>
      {/each}
    </div>
  {/if}
  {#if forced.size}
    <div class="chips">
      {#each [...forced] as [gid, qty] (gid)}
        <span class="chip">
          <input type="number" min="1" value={qty} onchange={(e) => onwish(gid, Number(e.currentTarget.value))} /> × {name(gid)}
          <button class="x" onclick={() => onunwish(gid)} title="Retirer">✕</button>
        </span>
      {/each}
    </div>
  {/if}
</div>

{#if excluded.size}
  <div class="box">
    <div class="h">Objets écartés <span class="mut">cliquer pour les remettre</span></div>
    <div class="chips">
      {#each [...excluded] as gid (gid)}
        <button class="chip off" onclick={() => onrestore(gid)} title="Remettre dans le calcul">{name(gid)} ↺</button>
      {/each}
    </div>
  </div>
{/if}

<div class="box">
  <div class="h">Sous-crafts <span class="mut">un ingrédient d'un métier coché est crafté s'il revient moins cher (à son niveau, s'il est connu), sinon acheté</span></div>
  <div class="chips">
    {#each jobs as j (j.id)}
      <label class="chip" class:on={subJobs.has(j.id)}>
        <input type="checkbox" checked={subJobs.has(j.id)} onchange={() => ontoggle(j.id)} />
        {j.name}{#if mine[j.id]} <span class="mut">{mine[j.id].level}</span>{/if}
      </label>
    {/each}
  </div>
</div>

<style>
  .box { margin: 12px 0; padding: 8px 10px; border: 1px solid var(--line); background: var(--panel); }
  .h { font-weight: 600; margin-bottom: 6px; }
  .h .mut { font-weight: 400; font-size: 12px; margin-left: 6px; }
  .found, .chips { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 6px; }
  .chip { display: inline-flex; align-items: center; gap: 5px; padding: 2px 8px; border: 1px solid var(--line); border-radius: 12px; font-size: 12px; background: var(--bg); }
  .chip.on { border-color: var(--acc); }
  .chip.off { color: var(--mut); cursor: pointer; }
  .chip input[type="number"] { width: 54px; padding: 0 4px; text-align: right; }
  .x { border: 0; background: none; color: var(--mut); padding: 0 2px; cursor: pointer; }
  .x:hover { color: var(--bad); }
</style>
