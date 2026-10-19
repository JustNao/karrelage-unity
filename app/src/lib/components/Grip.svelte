<script lang="ts">
  import type { Columns } from "../columns.svelte";

  /**
   * Draggable divider between two panels, with a button that folds the panel on its side away (smoothly: the grid animates
   * its columns, see `.cgrid` in app.css). `dir` as in `Columns.drag`: 1 when the panel is on the left of the grip, -1 on its right.
   */
  let { cols, name, dir, collapsible = false }: { cols: Columns<any>; name: string; dir: 1 | -1; collapsible?: boolean } = $props();
  const shut = $derived(!!cols.collapsed[name]);
  /** Arrow points where the panel goes (folding) or comes from (unfolding). */
  const arrow = $derived((dir === 1) !== shut ? "‹" : "›");
  /** The button sits on the panel's side while it is open, and on the content's side once the panel has no width left. */
  const towardsPanel = $derived((dir === 1) !== shut);
</script>

<div class="grip" role="separator" aria-orientation="vertical" onpointerdown={(e) => cols.drag(name, dir, e)} style:cursor={shut ? "default" : null}>
  {#if collapsible}
    <button class="fold" class:left={towardsPanel} onpointerdown={(e) => e.stopPropagation()} onclick={() => cols.toggle(name)} title={shut ? "Afficher le panneau" : "Masquer le panneau"}>{arrow}</button>
  {/if}
</div>

<style>
  .grip { position: relative; z-index: 4; }
  .fold { position: absolute; top: 50%; transform: translateY(-50%); left: 0; width: 16px; height: 44px; padding: 0; border-radius: 0 8px 8px 0; border-left: 0; background: var(--panel); color: var(--mut); line-height: 1; opacity: 0.55; transition: opacity 0.15s; }
  .fold.left { left: auto; right: 0; border-radius: 8px 0 0 8px; border-left: 1px solid var(--line); border-right: 0; }
  .fold:hover { opacity: 1; color: var(--fg); }
</style>
