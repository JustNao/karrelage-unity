<script lang="ts" generics="R, K extends string">
  import type { Snippet } from "svelte";
  import type { Sort } from "../sort.svelte";

  /**
   * A table with sortable column headers (global `.tbl` look from app.css). The caller sorts the rows with `sort.apply`
   * and renders each row's cells:
   *
   *   <SortTable {sort} rows={sort.apply(view)} key={(r) => r.gid} columns={[{ key: "name", label: "Rune" }, { key: "price", label: "Prix", num: true }]}>
   *     {#snippet row(r)}<td>{r.name}</td><td class="n">{kamas(r.price)}</td>{/snippet}
   *     {#snippet empty()}Nothing yet{/snippet}
   *   </SortTable>
   */
  let {
    columns,
    rows,
    sort,
    key,
    row,
    empty,
    rowProps,
  }: {
    columns: { key: K; label: string; num?: boolean; title?: string }[];
    rows: R[];
    sort: Sort<K>;
    key: (r: R) => string | number;
    row: Snippet<[R]>;
    /** Content of the single row shown when there are no rows. */
    empty?: Snippet;
    /** Extra attributes of each <tr> (id, class, onclick...). */
    rowProps?: (r: R) => Record<string, any>;
  } = $props();
</script>

<table class="tbl">
  <thead>
    <tr>
      {#each columns as c (c.key)}
        <th class:n={c.num} title={c.title}><button onclick={() => sort.set(c.key)}>{c.label}{sort.arrow(c.key)}</button></th>
      {/each}
    </tr>
  </thead>
  <tbody>
    {#each rows as r (key(r))}
      <tr {...rowProps?.(r)}>{@render row(r)}</tr>
    {:else}
      {#if empty}<tr><td colspan={columns.length} class="mut">{@render empty()}</td></tr>{/if}
    {/each}
  </tbody>
</table>
