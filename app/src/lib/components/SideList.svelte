<script lang="ts">
  import type { Snippet } from "svelte";
  import type { Columns } from "../columns.svelte";
  import PanelHead from "./PanelHead.svelte";

  /**
   * Left panel of the tabs (HDV items, Crafting, Stuff builds, Paquets types): a collapsible panel with its title, an optional
   * sticky bar under it (search...) and rows. Rows use the global `.irow` class (`.sel` = selected, `.first` = top border),
   * so they look the same everywhere:
   *
   *   <SideList {cols} name="list" title="Objets">
   *     {#snippet bar()}<SearchBar ... />{/snippet}
   *     {#each items as i}<div class="irow" class:sel={...} onclick={...}>...</div>{/each}
   *   </SideList>
   */
  let { cols, name, title, bar, children }: { cols: Columns<any>; name: string; title: string; bar?: Snippet; children: Snippet } = $props();
</script>

<div class="ilist" class:cz={cols.collapsed[name]}>
  <PanelHead {cols} {name} {title} />
  {#if bar}<div class="ibar">{@render bar()}</div>{/if}
  {@render children()}
</div>
