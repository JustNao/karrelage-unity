<script lang="ts">
  import { onMount } from "svelte";
  import { FOODS, foodGids, levelBase } from "../../../../lib/familiar";
  import { prices } from "../../../../lib/stores/prices.svelte";
  import { fmtNum, kamas } from "../../../../lib/fmt";
  import { useSort } from "../../../../lib/sort.svelte";
  import Banner from "../../../../lib/components/Banner.svelte";
  import SearchBar, { matcher } from "../../../../lib/components/SearchBar.svelte";
  import SmoothScroll from "../../../../lib/components/SmoothScroll.svelte";
  import SortTable from "../../../../lib/components/SortTable.svelte";

  type SortKey = "name" | "xp" | "qty" | "unit" | "meal" | "perXp" | "meals" | "total";
  interface Row { gid: number; name: string; xp: number; qty: number; unit: number | null; meal: number | null; perXp: number | null; meals: number; total: number | null }

  let gids = $state.raw<Record<string, number>>({});
  let level = $state(1);
  let xp = $state(0);
  let target = $state(100);
  let q = $state("");
  let err = $state("");
  let onlyPriced = $state(false);
  const sort = useSort<SortKey>("total", false);

  const clampLevel = (l: number) => Math.min(Math.max(l, 1), 100);
  const need = $derived(Math.max(0, levelBase(clampLevel(target)) - levelBase(clampLevel(level)) - Math.max(0, xp || 0)));

  const rows = $derived.by<Row[]>(() =>
    FOODS.flatMap((f) => {
      const gid = gids[f.name];
      if (gid == null) return [];
      const unit = prices.price(gid);
      const meal = unit != null ? unit * f.qty : null;
      const perXp = meal != null ? meal / f.xp : null;
      const meals = Math.ceil(need / f.xp);
      return [{ gid, name: f.name, xp: f.xp, qty: f.qty, unit, meal, perXp, meals, total: meal != null ? meal * meals : null }];
    }),
  );

  const view = $derived.by(() => {
    const m = matcher(q);
    return sort.apply(rows.filter((r) => m(r.name) && (!onlyPriced || r.unit != null)));
  });
  const best = $derived(rows.filter((r) => r.total != null).sort((a, b) => a.total! - b.total!)[0] ?? null);

  const COLUMNS: { key: SortKey; label: string; num?: boolean }[] = [
    { key: "name", label: "Ressource" },
    { key: "xp", label: "XP / repas", num: true },
    { key: "qty", label: "Quantité / repas", num: true },
    { key: "unit", label: "Prix unitaire", num: true },
    { key: "meal", label: "Coût / repas", num: true },
    { key: "perXp", label: "Coût / XP", num: true },
    { key: "meals", label: "Repas nécessaires", num: true },
    { key: "total", label: "Prix estimé", num: true },
  ];
  const int = (e: Event) => Number((e.currentTarget as HTMLInputElement).value);

  onMount(() => {
    foodGids().then((m) => (gids = m)).catch((e) => (err = String(e)));
    void prices.ensure();
  });
</script>

<div class="rv">
  <div class="head">
    <div class="fh">Familiers</div>
  </div>
  <div class="form">
    <label>Niveau actuel <input type="number" min="1" max="100" value={level} oninput={(e) => (level = int(e))} /></label>
    <label>XP dans ce niveau <input type="number" min="0" value={xp} oninput={(e) => (xp = int(e))} /></label>
    <label>Niveau désiré <input type="number" min="1" max="100" value={target} oninput={(e) => (target = int(e))} /></label>
    <div class="res">
      <b>{fmtNum(need)}</b> XP à gagner
      {#if best && need > 0}· avec <b>{best.name}</b> : <b>{best.meals}</b> repas ({fmtNum(best.meals * best.qty)} ressource{best.meals * best.qty > 1 ? "s" : ""}) pour <b class="good">{kamas(Math.round(best.total!))}</b>
      {:else if need > 0}<span class="mut">· prix inconnus</span>{/if}
    </div>
  </div>
  <SearchBar bind:value={q} bind:priced={onlyPriced} placeholder="Rechercher une ressource…" />
  {#if err}<Banner inset>Erreur : {err} (relancez l'application pour charger la dernière version)</Banner>{/if}
  <SmoothScroll>
    <SortTable columns={COLUMNS} rows={view} {sort} key={(r) => r.name}>
      {#snippet row(r)}
        <td><b>{r.name}</b></td>
        <td class="n">{fmtNum(r.xp)}</td>
        <td class="n">{r.qty}</td>
        <td class="n">{r.unit != null ? kamas(r.unit) : "—"}</td>
        <td class="n">{r.meal != null ? kamas(Math.round(r.meal)) : "—"}</td>
        <td class="n">{r.perXp != null ? fmtNum(r.perXp) + " K" : "—"}</td>
        <td class="n">{need > 0 ? r.meals : "—"}</td>
        <td class="n"><b>{r.total != null && need > 0 ? kamas(Math.round(r.total)) : "—"}</b></td>
      {/snippet}
      {#snippet empty()}Aucune ressource à afficher : ouvrez-les à l'hôtel de vente pour enregistrer leur prix (ou désactivez « Prix connus seulement »).{/snippet}
    </SortTable>
  </SmoothScroll>
</div>

<style>
  .rv { flex: 1; min-height: 0; display: flex; flex-direction: column; font-family: var(--font); padding: 8px 10px 0; }
  .rv > :global(.wrap) { flex: 1; }
  .head { display: flex; align-items: center; gap: 10px; margin-bottom: 10px; flex-wrap: wrap; }
  .head .fh { margin: 0; flex: 1; }
  .form { display: flex; align-items: center; gap: 14px; flex-wrap: wrap; margin-bottom: 10px; padding: 8px 10px; background: var(--panel); border: 1px solid var(--line); }
  .form label { display: flex; align-items: center; gap: 6px; }
  .form input { width: 90px; padding: 3px 6px; text-align: right; }
  .res { flex: 1; min-width: 260px; }
</style>
