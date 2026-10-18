<script lang="ts">
  import { kamas } from "../api";
  import { benefit } from "../craft";
  import { isLegendary, levelOf } from "../companion";
  import { isSenile, neuronsOf } from "../dragodinde";

  /** Familier / montilier / monture lots as a table (same look as the Crafting tab tables). `neurons` + `neuroneUnit` = mount extraction. */
  let {
    lots,
    unitPrice,
    neurons = null,
    neuroneUnit = null,
    picked = null,
    faded = () => false,
    onpick,
  }: {
    lots: { row: any; key: string | number }[];
    unitPrice: (r: any) => number;
    /** Neurones obtained by extracting this mount (null = not a dragodinde from the table). */
    neurons?: number | null;
    neuroneUnit?: number | null;
    picked?: string | number | null;
    faded?: (r: any) => boolean;
    onpick?: (key: string | number) => void;
  } = $props();

  const mount = $derived(neurons != null);
  const sgn = (n: number) => (n > 0 ? "+" : "") + kamas(n);

  /** Per lot: price, level and, for mounts, the profit of extracting it (senile lots give 1 neurone). */
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
  let sort = $state<SortKey | null>(null); // null = the order of the list
  let desc = $state(false);
  const setSort = (k: SortKey) => {
    if (sort === k) desc = !desc;
    else { sort = k; desc = k === "status" || k === "profit" || k === "pct"; }
  };
  const arrow = (k: SortKey) => (sort === k ? (desc ? " ▾" : " ▴") : "");
  const sorted = $derived.by(() => {
    const k = sort;
    if (!k) return rows;
    return [...rows].sort((a, b) => {
      const x = a[k], y = b[k];
      const bad = (v: number | null) => v == null || !Number.isFinite(v);
      if (bad(x) && bad(y)) return 0;
      if (bad(x)) return 1; // unknown values always last
      if (bad(y)) return -1;
      return desc ? (y as number) - (x as number) : (x as number) - (y as number);
    });
  });
</script>

<table>
  <thead>
    <tr>
      <th class="n"><button onclick={() => setSort("level")}>Niveau{arrow("level")}</button></th>
      <th><button onclick={() => setSort("status")}>Statut{arrow("status")}</button></th>
      <th class="n"><button onclick={() => setSort("price")}>Prix{arrow("price")}</button></th>
      {#if mount}
        <th class="n"><button onclick={() => setSort("n")}>Neurones{arrow("n")}</button></th>
        <th class="n"><button onclick={() => setSort("sale")}>Valeur des neurones{arrow("sale")}</button></th>
        <th class="n"><button onclick={() => setSort("profit")}>Bénéfice{arrow("profit")}</button></th>
        <th class="n"><button onclick={() => setSort("pct")}>Rentabilité{arrow("pct")}</button></th>
      {/if}
    </tr>
  </thead>
  <tbody>
    {#each sorted as r (r.key)}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <tr id="lot-{r.key}" class:picked={picked === r.key} class:faded={faded(r.row)} class:hot={(r.profit ?? 0) > 0} onclick={() => onpick?.(r.key)}>
        <td class="n"><b>{r.level ?? "?"}</b></td>
        <td>
          {#if r.legendary}<span class="tag leg">★ Légendaire</span>{/if}
          {#if r.senile}<span class="tag mut">Sénile</span>{/if}
          {#if (r.profit ?? 0) > 0}<span class="tag good" title="Extraire cette monture en neurones rapporte plus que son prix d'achat">⚠ Rentable en neurones</span>{/if}
        </td>
        <td class="n">{Number.isFinite(r.price) ? kamas(r.price) : "—"}</td>
        {#if mount}
          <td class="n">{r.n}</td>
          <td class="n">{r.sale != null ? kamas(r.sale) : "—"}</td>
          <td class="n" class:good={(r.profit ?? 0) > 0} class:bad={(r.profit ?? 0) < 0}>{r.profit != null ? sgn(r.profit) : "—"}</td>
          <td class="n" class:good={(r.pct ?? 0) > 0} class:bad={(r.pct ?? 0) < 0}><b>{r.pct != null ? (r.pct > 0 ? "+" : "") + r.pct.toFixed(1) + " %" : "—"}</b></td>
        {/if}
      </tr>
    {:else}
      <tr><td colspan={mount ? 7 : 3} class="mut">Aucun lot ne correspond au filtre</td></tr>
    {/each}
  </tbody>
</table>

<style>
  table { width: 100%; border-collapse: collapse; font-size: 12px; font-family: var(--font); }
  th { text-align: left; font-weight: 400; color: var(--mut); padding: 2px 6px; position: sticky; top: 0; background: var(--bg); }
  th button { border: 0; background: none; color: inherit; font: inherit; padding: 0; cursor: pointer; }
  td { padding: 4px 6px; border-top: 1px solid var(--line); }
  tbody tr { cursor: pointer; transition: opacity 0.25s; }
  tbody tr:hover { background: var(--panel); }
  tr.picked { outline: 1px solid var(--acc); outline-offset: -1px; }
  tr.faded { opacity: 0.2; }
  tr.hot { background: color-mix(in srgb, var(--ok, #4caf50) 8%, transparent); }
  .n { text-align: right; white-space: nowrap; }
  .tag { margin-right: 8px; white-space: nowrap; }
  .leg { color: var(--acc); }
  .good { color: var(--good, #4caf50); }
  .bad { color: var(--bad, #e05555); }
</style>
