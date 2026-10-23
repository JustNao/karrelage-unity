<script lang="ts">
  import { onMount } from "svelte";
  import { call, cached } from "../../../../lib/api";
  import { readFlag, writeFlag } from "../../../../lib/persist.svelte";
  import { benefit, craftCost, type Recipes } from "../../../../lib/craft";
  import { prices } from "../../../../lib/stores/prices.svelte";
  import { kamas, pct, signedKamas } from "../../../../lib/fmt";
  import { useSort } from "../../../../lib/sort.svelte";
  import Banner from "../../../../lib/components/Banner.svelte";
  import SearchBar, { matcher } from "../../../../lib/components/SearchBar.svelte";
  import SmoothScroll from "../../../../lib/components/SmoothScroll.svelte";
  import SortTable from "../../../../lib/components/SortTable.svelte";

  const ASTRAL = ["mineure", "moyenne", "majeure", "épatante", "merveilleuse", "légendaire"]; // weakest to strongest
  type SortKey = "name" | "effect" | "astral" | "craft" | "cumul" | "sale" | "profit" | "pct";
  interface Row { gid: number; name: string; effect: string | null; transcend: boolean; recipe: string; astral: string | null; astralRank: number | null; craft: number | null; cumul: number | null; sale: number | null; profit: number | null; pct: number | null; missing: string[] }

  let list = $state.raw<{ gid: number; name: string; effect: string | null; transcend: boolean }[]>([]);
  let recipes = $state.raw<Recipes>({ recipes: {}, names: {} });
  let q = $state("");
  let err = $state("");
  let onlyPriced = $state(false);
  // transcendance runes (type 211) shown or not, remembered
  const TRANSCEND_KEY = "karrelage.crafting.runes.transcend";
  let showTranscend = $state(readFlag(TRANSCEND_KEY, true));
  $effect(() => writeFlag(TRANSCEND_KEY, showTranscend));
  const sort = useSort<SortKey>("pct", true, (k) => k !== "name");

  const rows = $derived.by<Row[]>(() =>
    list.map((r) => {
      const ing = recipes.recipes[r.gid] ?? [];
      // craft: the ingredients bought as they are; cumul: each ingredient bought or crafted, whichever is cheaper (recursive)
      const costs = ing.map(([g, n]) => [prices.price(g), n] as const);
      const craft = costs.every(([p]) => p != null) ? costs.reduce((s, [p, n]) => s + p! * n, 0) : null;
      const c = craftCost(r.gid, recipes, prices.map);
      const cumul = c?.cost ?? null;
      const sale = prices.price(r.gid);
      const b = cumul != null && sale != null ? benefit(sale, cumul) : null;
      const recipe = ing.map(([g, n]) => `${n} × ${recipes.names[g] ?? g}`).join(" + ");
      // astral rune ingredient ("Rune astrale mineure" -> "mineure"); forgemagie Pa / Ra runes have none
      const astral = ing.map(([g]) => (recipes.names[g] ?? "").replace(/ /g, " ")).find((n) => n.startsWith("Rune astrale "))?.slice("Rune astrale ".length) ?? null;
      return { ...r, recipe, astral, astralRank: astral ? Math.max(0, ASTRAL.indexOf(astral)) : null, craft, cumul, sale, profit: b?.profit ?? null, pct: b?.pct ?? null, missing: c?.missing ?? [] };
    }),
  );

  const view = $derived.by(() => {
    const m = matcher(q);
    return sort.apply(
      rows.filter((r) => m(r.name) && (!onlyPriced || r.pct != null) && (showTranscend || !r.transcend)),
      (r, k) => (k === "astral" ? r.astralRank : r[k]),
    );
  });

  const COLUMNS: { key: SortKey; label: string; num?: boolean }[] = [
    { key: "name", label: "Rune" },
    { key: "effect", label: "Effet" },
    { key: "astral", label: "Rune astrale" },
    { key: "craft", label: "Prix de craft", num: true },
    { key: "cumul", label: "Prix cumulé", num: true },
    { key: "sale", label: "Prix de vente", num: true },
    { key: "profit", label: "Bénéfice", num: true },
    { key: "pct", label: "Rentabilité", num: true },
  ];

  onMount(() => {
    cached<{ gid: number; name: string; effect: string | null; transcend: boolean }[]>("prices", "craft_runes").then((l) => (list = l)).catch((e) => (err = String(e)));
    cached<Recipes>("auction", "recipes").then((r) => (recipes = r)).catch((e) => (err = String(e)));
    void prices.ensure();
  });
</script>

<div class="rv">
  <div class="head">
    <div class="fh">Runes</div>
  </div>
  <SearchBar bind:value={q} bind:priced={onlyPriced} placeholder="Rechercher une rune…" width="180px">
    {#snippet after()}<label><input type="checkbox" bind:checked={showTranscend} /> Runes de transcendance</label>{/snippet}
  </SearchBar>
  {#if err}<Banner inset>Erreur : {err} (relancez l'application pour charger la dernière version)</Banner>{/if}
  <SmoothScroll>
    <SortTable columns={COLUMNS} rows={view} {sort} key={(r) => r.gid}>
      {#snippet row(r)}
        <td title={r.recipe}><b>{r.name}</b></td>
        <td>{r.effect ?? "—"}</td>
        <td>{r.astral ?? "—"}</td>
        <td class="n">{r.craft != null ? kamas(r.craft) : "—"}</td>
        <td class="n" title={r.missing.length ? "Prix manquant : " + r.missing.join(", ") : ""}>{r.cumul != null ? kamas(r.cumul) : "—"}{#if r.missing.length} <span class="warn">⚠</span>{/if}</td>
        <td class="n">{r.sale != null ? kamas(r.sale) : "—"}</td>
        <td class="n" class:good={(r.profit ?? 0) > 0} class:bad={(r.profit ?? 0) < 0}>{r.profit != null ? signedKamas(r.profit) : "—"}</td>
        <td class="n" class:good={(r.pct ?? 0) > 0} class:bad={(r.pct ?? 0) < 0}><b>{r.pct != null ? pct(r.pct) : "—"}</b></td>
      {/snippet}
      {#snippet empty()}Aucune rune à afficher : ouvrez les runes et leurs ingrédients à l'hôtel de vente pour enregistrer leurs prix (ou désactivez « Prix connus seulement »).{/snippet}
    </SortTable>
  </SmoothScroll>
</div>

<style>
  .rv { flex: 1; min-height: 0; display: flex; flex-direction: column; font-family: var(--font); padding: 8px 10px 0; }
  .rv > :global(.wrap) { flex: 1; }
  .head { display: flex; align-items: center; gap: 10px; margin-bottom: 10px; flex-wrap: wrap; }
  .head .fh { margin: 0; flex: 1; min-width: 260px; }
</style>
