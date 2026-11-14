<script lang="ts">
  import { onMount } from "svelte";
  import SubView from "../../../../lib/components/SubView.svelte";
  import Segmented from "../../../../lib/components/Segmented.svelte";
  import Switch from "../../../../lib/components/Switch.svelte";
  import SearchBar, { matcher } from "../../../../lib/components/SearchBar.svelte";
  import SortTable from "../../../../lib/components/SortTable.svelte";
  import { cached, call, listenAll } from "../../../../lib/api";
  import { persisted } from "../../../../lib/persist.svelte";
  import { benefit, type Recipes } from "../../../../lib/craft";
  import { effectivePrice } from "../../../../lib/price";
  import { prices } from "../../../../lib/stores/prices.svelte";
  import { kamasOr, pct, signedKamas } from "../../../../lib/fmt";
  import { useSort } from "../../../../lib/sort.svelte";
  import { Costs, MAX_JOB_LEVEL, type JobCatalog, type JobLevels } from "../../../../lib/jobXp";

  /**
   * Crafting « Crafts rentables »: every craftable item of a category (equipment / resource / consumable), all jobs or one,
   * with its craft cost (ingredients bought, or crafted when cheaper: lib/jobXp.ts `Costs`), its auction house price (manual,
   * else the cheapest read in the auction house, else the weekly average) and the profit after the AH tax. « Craftable par
   * moi »: only what the character's jobs (read at login, jobs.rs) can craft at their level, ingredients crafted by them only.
   */
  type Category = "0" | "2" | "1";
  const CATEGORIES: { id: Category; label: string }[] = [
    { id: "0", label: "Équipement" },
    { id: "2", label: "Ressource" },
    { id: "1", label: "Consommable" },
  ];
  /** Job 1 « Base »: recipes made without a job. */
  const BASE_JOB = 1;
  const ui = persisted("karrelage.crafting.rentables", { category: "0" as Category, job: 0, mine: false, priced: true });

  type SortKey = "name" | "type" | "level" | "job" | "craft" | "sale" | "profit" | "pct";
  interface Row { gid: number; name: string; type: string; level: number; job: string; craft: number | null; sale: number | null; avg: boolean; profit: number | null; pct: number | null }

  let catalog = $state.raw<JobCatalog>({ jobs: {}, crafts: {} });
  let recipes = $state.raw<Recipes>({ recipes: {}, names: {} });
  let levels = $state.raw<JobLevels>({ current: 0, characters: {} });
  let q = $state("");
  let err = $state("");
  const sort = useSort<SortKey>("pct", true, (k) => k !== "name" && k !== "type" && k !== "job");

  onMount(() => {
    cached<JobCatalog>("jobs", "catalog").then((c) => (catalog = c)).catch((e) => (err = String(e)));
    cached<Recipes>("auction", "recipes").then((r) => (recipes = r)).catch((e) => (err = String(e)));
    const off = listenAll({ "jobs:levels": (l: JobLevels) => (levels = l) });
    off.ready.then(() => call<JobLevels>("jobs", "levels")).then((l) => (levels = l)).catch((e) => (err = String(e)));
    void prices.ensure();
    return off;
  });

  // the character's jobs: the last one logged in, else the first one seen
  const mine = $derived.by(() => {
    const has = (k: string | number) => Object.keys(levels.characters[k] ?? {}).length > 0;
    const c = has(levels.current) ? String(levels.current) : Object.keys(levels.characters).find(has);
    return c !== undefined ? levels.characters[c] : {};
  });
  const known = $derived(Object.keys(mine).length > 0);
  const levelOfJob = (j: number) => (j === BASE_JOB ? MAX_JOB_LEVEL : (mine[j]?.level ?? 0));
  const mineOnly = $derived(ui.mine && known);
  /** an item the planner may craft (the listed one, or an ingredient on the way): any job, or the character's at its level */
  const canMake = (gid: number) => {
    const c = catalog.crafts[gid];
    return !!c && (!mineOnly || levelOfJob(c[0]) >= c[1]);
  };

  /** the category's items (recipe known) */
  const items = $derived(
    Object.entries(catalog.crafts)
      .filter(([g, c]) => String(c[3]) === ui.category && recipes.recipes[Number(g)])
      .map(([g, c]) => ({ gid: Number(g), c })),
  );
  const jobs = $derived.by(() => {
    const ids = new Set(items.map((i) => i.c[0]));
    const list = [...ids].map((id) => ({ id, name: catalog.jobs[id] ?? `#${id}` })).sort((a, b) => a.name.localeCompare(b.name));
    return [{ id: 0, name: "Tous" }, ...list];
  });
  const job = $derived(jobs.some((j) => j.id === ui.job) ? ui.job : 0);

  // rebuilt whenever a price changes (each price read inside is tracked)
  const rows = $derived.by<Row[]>(() => {
    const costs = new Costs(recipes, prices.map, canMake);
    return items
      .filter((i) => (job === 0 || i.c[0] === job) && canMake(i.gid))
      .map(({ gid, c }) => {
        const craft = costs.crafted(gid).unit;
        const p = effectivePrice(prices.map.get(gid));
        const b = craft !== null && p ? benefit(p.value, craft) : null;
        return {
          gid,
          name: recipes.names[gid] ?? `#${gid}`,
          type: catalog.types?.[c[4]] ?? "",
          level: c[1],
          job: catalog.jobs[c[0]] ?? "",
          craft: craft === null ? null : Math.round(craft),
          sale: p?.value ?? null,
          avg: p?.source === "moyenne",
          profit: b?.profit ?? null,
          pct: b?.pct ?? null,
        };
      });
  });
  const view = $derived.by(() => {
    const m = matcher(q);
    return sort.apply(rows.filter((r) => m(r.name, r.type) && (!ui.priced || r.pct !== null)));
  });

  const COLUMNS = $derived<{ key: SortKey; label: string; num?: boolean; title?: string }[]>([
    { key: "name", label: "Objet" },
    { key: "type", label: "Type" },
    { key: "level", label: "Niv.", num: true },
    ...(job === 0 ? [{ key: "job" as const, label: "Métier" }] : []),
    { key: "craft", label: "Prix de craft", num: true, title: "Ingrédients achetés, ou craftés quand c'est moins cher" },
    { key: "sale", label: "Prix HDV", num: true, title: "Prix relevé à l'hôtel de vente (le moins cher), sinon moyenne de la semaine (« moy. »)" },
    { key: "profit", label: "Bénéfice", num: true, title: "Prix HDV moins la taxe de 2 %, moins le prix de craft" },
    { key: "pct", label: "Rentabilité", num: true },
  ]);

  let copied = $state<number | null>(null);
  function copy(r: Row) {
    navigator.clipboard?.writeText(r.name).then(() => {
      copied = r.gid;
      setTimeout(() => copied === r.gid && (copied = null), 900);
    }, () => {});
  }
</script>

<SubView title="Crafts rentables" {err}>
  {#snippet top()}
    <div class="bar">
      <Segmented bind:value={ui.category} options={CATEGORIES} label="Catégorie" />
      <label
        class="sw"
        title={known ? "Seulement ce que vos métiers savent crafter à leur niveau (ingrédients craftés par eux seulement)" : "Vos métiers ne sont pas encore lus : connectez-vous, application ouverte"}
      >
        <Switch bind:checked={ui.mine} label="Craftable par moi" disabled={!known} /> Craftable par moi
      </label>
    </div>
    <div class="jobs">
      {#each jobs as j (j.id)}
        <button class="chip" class:on={job === j.id} onclick={() => (ui.job = j.id)}>
          {j.name}{#if j.id && mine[j.id]}<span class="mut lvl">{mine[j.id].level}</span>{/if}
        </button>
      {/each}
    </div>
    <SearchBar bind:value={q} bind:priced={ui.priced} placeholder="Rechercher un objet ou un type…" />
  {/snippet}
  <SortTable columns={COLUMNS} rows={view} {sort} key={(r) => r.gid} rowProps={(r) => ({ class: "craft", title: "Cliquer pour copier le nom", onclick: () => copy(r) })}>
    {#snippet row(r)}
      <td><b>{r.name}</b>{#if copied === r.gid} <span class="copied">copié</span>{/if}</td>
      <td class="mut">{r.type}</td>
      <td class="n">{r.level}</td>
      {#if job === 0}<td>{r.job}</td>{/if}
      <td class="n">{kamasOr(r.craft)}</td>
      <td class="n">{kamasOr(r.sale)}{#if r.avg} <span class="mut avg">moy.</span>{/if}</td>
      <td class="n" class:good={(r.profit ?? 0) > 0} class:bad={(r.profit ?? 0) < 0}>{r.profit !== null ? signedKamas(r.profit) : "—"}</td>
      <td class="n" class:good={(r.pct ?? 0) > 0} class:bad={(r.pct ?? 0) < 0}><b>{r.pct !== null ? pct(r.pct) : "—"}</b></td>
    {/snippet}
    {#snippet empty()}Aucun objet à afficher : ouvrez-les (ou leurs ingrédients) à l'hôtel de vente pour enregistrer leurs prix (ou désactivez « Prix connus seulement »).{/snippet}
  </SortTable>
</SubView>

<style>
  .bar { display: flex; align-items: center; gap: 14px; flex-wrap: wrap; margin-bottom: 8px; }
  .sw { display: inline-flex; align-items: center; gap: 6px; cursor: pointer; }
  .jobs { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 8px; }
  .chip { padding: 2px 10px; border: 1px solid var(--line); border-radius: 12px; font-size: 12px; background: none; color: var(--mut); }
  .chip.on { border-color: var(--acc); color: var(--fg); background: var(--panel); font-weight: 600; }
  .lvl { margin-left: 5px; }
  .avg { font-size: 10px; }
  .copied { color: var(--ok); font-size: 10px; }
  :global(tr.craft) { cursor: pointer; }
  :global(tr.craft:hover td) { background: var(--panel); }
</style>
