<script lang="ts">
  import SubView from "../../../../lib/components/SubView.svelte";
  import { onMount } from "svelte";
  import { cached } from "../../../../lib/api";
  import { FOODS, MAX_LEVEL, foodNames, foodCosts, levelBase, unitsFor } from "../../../../lib/familiar";
  import type { Recipes } from "../../../../lib/craft";
  import { prices } from "../../../../lib/stores/prices.svelte";
  import { fmtInt, fmtNum, kamas, kamasOr, kamasFull } from "../../../../lib/fmt";
  import { useSort } from "../../../../lib/sort.svelte";
  import SearchBar, { matcher } from "../../../../lib/components/SearchBar.svelte";
  import SortTable from "../../../../lib/components/SortTable.svelte";

  type SortKey = "name" | "xp" | "buy" | "craft" | "perXp" | "units" | "total";
  interface Row { gid: number; name: string; xp: number; buy: number | null; craft: number | null; unit: number | null; how: "achat" | "craft" | null; perXp: number | null; units: number; total: number | null }

  let names = $state.raw<Record<string, string>>({});
  let recipes = $state.raw<Recipes>({ recipes: {}, names: {} });
  let level = $state(1);
  let xp = $state(0);
  let target = $state(MAX_LEVEL);
  let q = $state("");
  let err = $state("");
  let onlyPriced = $state(false);
  const sort = useSort<SortKey>("total", false);

  const need = $derived(Math.max(0, levelBase(target) - levelBase(level) - Math.max(0, xp || 0)));
  const costs = $derived(foodCosts(recipes, prices.map));

  const rows = $derived.by<Row[]>(() =>
    FOODS.flatMap((f) => {
      const name = names[f.gid];
      if (name == null || f.xp <= 0) return [];
      const c = costs.get(f.gid)!;
      const units = unitsFor(need, f.xp);
      return [{ gid: f.gid, name, xp: f.xp, buy: c.buy, craft: c.craft, unit: c.unit, how: c.how, perXp: c.unit != null ? c.unit / f.xp : null, units, total: c.unit != null ? c.unit * units : null }];
    }),
  );

  const view = $derived.by(() => {
    const m = matcher(q);
    return sort.apply(rows.filter((r) => m(r.name) && (!onlyPriced || r.unit != null)));
  });
  const best = $derived(rows.filter((r) => r.total != null).sort((a, b) => a.total! - b.total!)[0] ?? null);

  const COLUMNS: { key: SortKey; label: string; num?: boolean }[] = [
    { key: "name", label: "Ressource" },
    { key: "xp", label: "XP / unité", num: true },
    { key: "buy", label: "Prix d'achat", num: true },
    { key: "craft", label: "Prix de craft", num: true },
    { key: "perXp", label: "Coût / XP", num: true },
    { key: "units", label: "Unités nécessaires", num: true },
    { key: "total", label: "Prix estimé", num: true },
  ];
  const int = (e: Event) => Number((e.currentTarget as HTMLInputElement).value);

  /** Row clicked: copy the resource's name (to paste in the auction house search). */
  let copied = $state<number | null>(null);
  function copy(r: Row) {
    navigator.clipboard?.writeText(r.name).then(() => {
      copied = r.gid;
      setTimeout(() => copied === r.gid && (copied = null), 900);
    }, () => {});
  }

  onMount(() => {
    foodNames().then((m) => (names = m)).catch((e) => (err = String(e)));
    cached<Recipes>("auction", "recipes").then((r) => (recipes = r)).catch((e) => (err = String(e)));
    void prices.ensure();
  });
</script>

<SubView title="Familiers" {err}>
  {#snippet top()}
    <div class="form">
      <label>Niveau actuel <input type="number" min="1" max={MAX_LEVEL} value={level} oninput={(e) => (level = int(e))} /></label>
      <label>XP dans ce niveau <input type="number" min="0" value={xp} oninput={(e) => (xp = int(e))} /></label>
      <label>Niveau désiré <input type="number" min="1" max={MAX_LEVEL} value={target} oninput={(e) => (target = int(e))} /></label>
      <div class="res">
        <b>{fmtInt(need)}</b> XP à gagner
        {#if best && need > 0}· avec <b>{best.name}</b>{best.how === "craft" ? " (craftée)" : ""} : <b>{fmtInt(best.units)}</b> unité{best.units > 1 ? "s" : ""} pour <b class="good">{kamas(Math.round(best.total!))}</b>
        {:else if need > 0}<span class="mut">· prix inconnus</span>{/if}
      </div>
    </div>
    <SearchBar bind:value={q} bind:priced={onlyPriced} placeholder="Rechercher une ressource…" />
  {/snippet}
    <SortTable columns={COLUMNS} rows={view} {sort} key={(r) => r.gid} rowProps={(r) => ({ class: "food", title: "Cliquer pour copier le nom", onclick: () => copy(r) })}>
      {#snippet row(r)}
        <td><b>{r.name}</b>{#if copied === r.gid} <span class="copied">copié</span>{/if}</td>
        <td class="n">{fmtNum(r.xp, 4)}</td>
        <td class="n" class:used={r.how === "achat" && r.craft != null}>{kamasOr(r.buy)}</td>
        <td class="n" class:used={r.how === "craft"} title={r.how === "craft" ? "Moins cher à crafter (ingrédients achetés ou craftés)" : ""}>{kamasOr(r.craft)}</td>
        <td class="n">{r.perXp != null ? kamasFull(r.perXp) : "—"}</td>
        <td class="n">{need > 0 ? fmtInt(r.units) : "—"}</td>
        <td class="n"><b>{r.total != null && need > 0 ? kamas(Math.round(r.total)) : "—"}</b></td>
      {/snippet}
      {#snippet empty()}Aucune ressource à afficher : ouvrez-les (ou les ingrédients de leur craft) à l'hôtel de vente pour enregistrer leur prix (ou désactivez « Prix connus seulement »).{/snippet}
    </SortTable>
</SubView>

<style>
  .form { display: flex; align-items: center; gap: 14px; flex-wrap: wrap; margin-bottom: 10px; padding: 8px 10px; background: var(--panel); border: 1px solid var(--line); }
  .form label { display: flex; align-items: center; gap: 6px; }
  .form input { width: 90px; padding: 3px 6px; text-align: right; }
  .res { flex: 1; min-width: 260px; }
  .used { color: var(--ok); font-weight: 600; }
  :global(tr.food) { cursor: pointer; }
  :global(tr.food:hover td) { background: var(--panel); }
  .copied { color: var(--ok); font-size: 10px; }
</style>
