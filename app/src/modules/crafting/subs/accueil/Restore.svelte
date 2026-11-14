<script lang="ts">
  import { onMount } from "svelte";
  import { cached } from "../../../../lib/api";
  import { persisted } from "../../../../lib/persist.svelte";
  import type { Recipes } from "../../../../lib/craft";
  import { effectivePrice } from "../../../../lib/price";
  import { prices } from "../../../../lib/stores/prices.svelte";
  import { fmtInt, fmtNum, kamas } from "../../../../lib/fmt";
  import { Costs } from "../../../../lib/jobXp";
  import { copyName, homeItems, type Consumable } from "./home";

  /**
   * Accueil card: the consumables restoring the most life (or energy) per kamas, bought (auction house price: cheapest read,
   * else weekly average) and crafted (ingredients bought, or crafted when cheaper), from a minimum item level (low items give
   * too little per use).
   */
  let { kind }: { kind: "life" | "energy" } = $props();
  const SHOWN = 3;
  const unit = $derived(kind === "life" ? "PV" : "énergie");
  // svelte-ignore state_referenced_locally
  const ui = persisted(`karrelage.crafting.home.${kind}`, { minLevel: 1 });

  let items = $state.raw<Consumable[]>([]);
  let recipes = $state.raw<Recipes>({ recipes: {}, names: {} });
  let err = $state("");
  onMount(() => {
    homeItems().then((h) => (items = h.consumables)).catch((e) => (err = String(e)));
    cached<Recipes>("auction", "recipes").then((r) => (recipes = r)).catch((e) => (err = String(e)));
  });

  interface Row { gid: number; name: string; level: number; gain: number; price: number; per: number; avg: boolean }
  const usable = $derived(items.filter((i) => i[kind] > 0 && i.level >= (ui.minLevel || 1)));
  const bought = $derived.by<Row[]>(() =>
    usable
      .flatMap((i) => {
        const p = effectivePrice(prices.map.get(i.gid));
        return p ? [{ gid: i.gid, name: i.name, level: i.level, gain: i[kind], price: p.value, per: p.value / i[kind], avg: p.source === "moyenne" }] : [];
      })
      .sort((a, b) => a.per - b.per)
      .slice(0, SHOWN),
  );
  const crafted = $derived.by<Row[]>(() => {
    const costs = new Costs(recipes, prices.map, (g) => !!recipes.recipes[g]);
    return usable
      .filter((i) => recipes.recipes[i.gid])
      .flatMap((i) => {
        const c = costs.crafted(i.gid).unit;
        return c !== null ? [{ gid: i.gid, name: i.name, level: i.level, gain: i[kind], price: c, per: c / i[kind], avg: false }] : [];
      })
      .sort((a, b) => a.per - b.per)
      .slice(0, SHOWN);
  });
</script>

{#snippet list(title: string, rows: Row[], how: string)}
  <div class="sec">{title}</div>
  {#each rows as r (r.gid)}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="row" onclick={() => copyName(r.name)} title="Cliquer pour copier le nom">
      <span class="n"><b>{r.name}</b> <span class="mut">niv. {r.level} · {fmtInt(r.gain)} {unit}</span></span>
      <span class="p">{kamas(Math.round(r.price))}{#if r.avg} <span class="mut avg">moy.</span>{/if}</span>
      <span class="per"><b>{fmtNum(r.per, r.per < 10 ? 2 : 1)}</b> <span class="mut">K / {unit}</span></span>
    </div>
  {:else}
    <div class="mut">Aucun prix connu : ouvrez {how} à l'hôtel de vente.</div>
  {/each}
{/snippet}

{#if err}<div class="bad">{err}</div>{/if}
<label class="lvl">Niveau minimum de l'objet <input type="number" min="1" max="200" bind:value={ui.minLevel} /></label>
{@render list("À acheter", bought, "ces consommables")}
{@render list("À crafter", crafted, "leurs ingrédients")}

<style>
  .lvl { display: flex; align-items: center; gap: 6px; margin-bottom: 6px; }
  .lvl input { width: 64px; padding: 2px 6px; text-align: right; }
  .sec { margin: 8px 0 2px; font-size: 11px; font-weight: 600; letter-spacing: 0.04em; text-transform: uppercase; color: var(--mut); }
  .row { display: flex; align-items: baseline; gap: 10px; padding: 3px 0; border-top: 1px solid var(--line); cursor: pointer; }
  .row:hover .n b { text-decoration: underline; }
  .n { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .p, .per { flex: none; font-variant-numeric: tabular-nums; }
  .per { min-width: 92px; text-align: right; }
  .avg { font-size: 10px; }
</style>
