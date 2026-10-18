<script lang="ts">
  import { kamas } from "../api";
  import { benefit } from "../craft";
  import { isLegendary, levelOf } from "../companion";
  import { isSenile } from "../dragodinde";

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
  const sale = $derived(neurons != null && neuroneUnit != null ? neuroneUnit * neurons : null);
  const sgn = (n: number) => (n > 0 ? "+" : "") + kamas(n);

  /** Per lot: price, level and, for mounts, the profit of extracting it (senile lots give nothing). */
  const rows = $derived(
    lots.map(({ row, key }) => {
      const price = unitPrice(row);
      const senile = mount && isSenile(row);
      const b = mount && !senile && sale != null && Number.isFinite(price) ? benefit(sale, price) : null;
      return { row, key, price, level: levelOf(row), legendary: isLegendary(row), senile, profit: b?.profit ?? null, pct: b?.pct ?? null };
    }),
  );
</script>

<table>
  <thead>
    <tr>
      <th class="n">Niveau</th>
      <th>Statut</th>
      <th class="n">Prix</th>
      {#if mount}
        <th class="n">Neurones</th>
        <th class="n">Valeur des neurones</th>
        <th class="n">Bénéfice</th>
        <th class="n">Rentabilité</th>
      {/if}
    </tr>
  </thead>
  <tbody>
    {#each rows as r (r.key)}
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
          <td class="n">{r.senile ? "—" : neurons}</td>
          <td class="n">{r.senile || sale == null ? "—" : kamas(sale)}</td>
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
