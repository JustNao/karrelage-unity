<script lang="ts">
  import type { Columns } from "../columns.svelte";

  /**
   * Header row of a resizable panel: its title and the button that folds it into a thin strip (smoothly, the grid animates its
   * columns: see `.cgrid` in app.css). `dir` as in `Columns.drag`: 1 for a panel on the left of its grip, -1 on its right
   * (the button then sits on the outer edge, on the left of the title).
   */
  let { cols, name, title, dir = 1 }: { cols: Columns<any>; name: string; title: string; dir?: 1 | -1 } = $props();
  const shut = $derived(!!cols.collapsed[name]);
  const arrow = $derived((dir === 1) !== shut ? "«" : "»");
</script>

<div class="phead" class:rev={dir === -1} class:shut>
  <span class="ptitle fh">{title}</span>
  <button class="mini" onclick={() => cols.toggle(name)} title={shut ? "Afficher le panneau" : "Masquer le panneau"}>{arrow}</button>
</div>

<style>
  .phead { display: flex; align-items: center; justify-content: space-between; gap: 6px; padding: 6px 8px 0; margin-bottom: 8px; position: sticky; top: 0; z-index: 2; background: inherit; }
  .phead.rev { flex-direction: row-reverse; }
  .phead.shut { justify-content: center; padding: 6px 0 0; }
  .phead .fh { margin: 0; }
  .mini { border: 0; background: none; padding: 0 6px; color: var(--mut); font-size: 16px; line-height: 1.2; }
  .mini:hover { color: var(--fg); }
</style>
