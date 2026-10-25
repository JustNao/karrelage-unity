<script lang="ts">
  import { kamas, pct, signedKamas } from "../fmt";
  import { benefit } from "../craft";
  import { isLegendary, levelOf } from "../companion";
  import { isSenile, neuronsOf } from "../mounts";
  import { unitPrice } from "../price";
  import { useSort } from "../sort.svelte";
  import SortTable from "./SortTable.svelte";

  /** Familier / montilier / monture lots as a table (same look as the Crafting tab tables). `neurons` + `neuroneUnit` = mount extraction. */
  let {
    lots,
    neurons = null,
    neuroneUnit = null,
    resource = "Neurones",
    picked = null,
    faded = () => false,
    onpick,
    changes = null,
  }: {
    lots: { row: any; key: string | number }[];
    /** Resources obtained by extracting this mount (null = not a mount from the tables). */
    neurons?: number | null;
    neuroneUnit?: number | null;
    /** Plural name of the extracted resource (column headers). */
    resource?: string;
    picked?: string | number | null;
    faded?: (r: any) => boolean;
    onpick?: (key: string | number) => void;
    /** Lot key -> what changed since the previous scan (as in LotTable). */
    changes?: Map<string, { kind: "added" | "removed" | "price"; was?: number }> | null;
  } = $props();

  const mount = $derived(neurons != null);

  /** Per lot: price, level and, for mounts, the profit of extracting it (senile lots give 1 resource). */
  const rows = $derived(
    lots.map(({ row, key }) => {
      const price = unitPrice(row);
      const senile = mount && isSenile(row);
      const n = neurons != null ? neuronsOf(row, neurons) : null;
      const sale = n != null && neuroneUnit != null ? neuroneUnit * n : null;
      const b = sale != null && Number.isFinite(price) ? benefit(sale, price) : null;
      const legendary = isLegendary(row);
      const profit = b?.profit ?? null;
      // status sort key: profitable first, then legendary, then senile
      const status = ((profit ?? 0) > 0 ? 4 : 0) + (legendary ? 2 : 0) + (senile ? 1 : 0);
      return { row, key, price, level: levelOf(row), legendary, senile, n, sale, profit, pct: b?.pct ?? null, status };
    }),
  );

  type SortKey = "level" | "status" | "price" | "n" | "sale" | "profit" | "pct";
  const sort = useSort<SortKey>(null, false, (k) => k === "status" || k === "profit" || k === "pct"); // no column = the order of the list
  const columns = $derived<{ key: SortKey; label: string; num?: boolean }[]>([
    { key: "level", label: "Niveau", num: true },
    { key: "status", label: "Statut" },
    { key: "price", label: "Prix", num: true },
    ...(mount
      ? ([
          { key: "n", label: resource, num: true },
          { key: "sale", label: `Valeur des ${resource.toLowerCase()}`, num: true },
          { key: "profit", label: "Bénéfice", num: true },
          { key: "pct", label: "Rentabilité", num: true },
        ] as const)
      : []),
  ]);
</script>

<div class="ctable">
  <SortTable
    {columns}
    rows={sort.apply(rows)}
    {sort}
    key={(r) => r.key}
    rowProps={(r) => ({ id: `lot-${r.key}`, class: [picked === r.key && "picked", faded(r.row) && "faded", (r.profit ?? 0) > 0 && "hot", changes?.get(String(r.key))?.kind].filter(Boolean).join(" "), onclick: () => onpick?.(r.key) })}
  >
    {#snippet row(r)}
      <td class="n"><b>{r.level ?? "?"}</b></td>
      <td>
        {#if r.legendary}<span class="tag leg">★ Légendaire</span>{/if}
        {#if r.senile}<span class="tag mut">Sénile</span>{/if}
        {#if (r.profit ?? 0) > 0}<span class="tag good" title="Extraire cette monture rapporte plus que son prix d'achat">⚠ Rentable en {resource.toLowerCase()}</span>{/if}
      </td>
      {@const was = changes?.get(String(r.key))?.was}
      <td class="n">{Number.isFinite(r.price) ? kamas(r.price) : "—"}{#if was != null}<br /><s class="was" title="Prix au relevé précédent">{Number.isFinite(was) ? kamas(was) : "—"}</s>{/if}</td>
      {#if mount}
        <td class="n">{r.n}</td>
        <td class="n">{r.sale != null ? kamas(r.sale) : "—"}</td>
        <td class="n" class:good={(r.profit ?? 0) > 0} class:bad={(r.profit ?? 0) < 0}>{r.profit != null ? signedKamas(r.profit) : "—"}</td>
        <td class="n" class:good={(r.pct ?? 0) > 0} class:bad={(r.pct ?? 0) < 0}><b>{r.pct != null ? pct(r.pct) : "—"}</b></td>
      {/if}
    {/snippet}
    {#snippet empty()}Aucun lot ne correspond au filtre{/snippet}
  </SortTable>
</div>

<style>
  /* lot rows are clickable (jump to the lot); the hot (profitable) tint is lighter than in the Crafting tables */
  .ctable :global(tbody tr) { cursor: pointer; transition: opacity 0.25s; }
  .ctable :global(tbody tr:hover) { background: var(--panel); }
  .ctable :global(tr.picked) { outline: 1px solid var(--acc); outline-offset: -1px; }
  .ctable :global(tr.faded) { opacity: 0.2; }
  .ctable :global(tr.hot) { background: color-mix(in srgb, var(--ok) 8%, transparent); }
  .ctable :global(tr.hot td) { background: none; }
  .ctable :global(tr.added) { background: color-mix(in srgb, var(--ok) 14%, transparent); }
  .ctable :global(tr.removed) { background: color-mix(in srgb, var(--bad) 14%, transparent); }
  .ctable :global(tr.added td), .ctable :global(tr.removed td) { background: none; }
  .ctable :global(tr.removed td) { opacity: 0.45; }
  .was { color: var(--mut); font-size: 11px; }
  .tag { margin-right: 8px; white-space: nowrap; }
  .leg { color: var(--acc); }
</style>
