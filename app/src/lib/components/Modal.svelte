<script lang="ts">
  import type { Snippet } from "svelte";
  import { clickOutside } from "../actions/clickOutside";

  /**
   * A small dialog in the centre of the window over a dimmed backdrop; a press outside it or Escape calls `onclose`.
   *
   *   {#if adding}<Modal title="Ajouter un stuff" onclose={() => (adding = false)}>...</Modal>{/if}
   */
  let { title, width = "420px", onclose, children }: { title: string; width?: string; onclose: () => void; children: Snippet } = $props();
</script>

<div class="back">
  <div class="box" role="dialog" aria-modal="true" aria-label={title} style:width use:clickOutside={onclose}>
    <div class="hd"><b>{title}</b><button class="x" title="Fermer" onclick={onclose}>✕</button></div>
    {@render children()}
  </div>
</div>

<style>
  .back { position: fixed; inset: 0; z-index: 100; display: grid; place-items: center; background: rgba(0, 0, 0, 0.45); }
  .box { max-width: calc(100vw - 32px); box-sizing: border-box; padding: 12px 14px; display: flex; flex-direction: column; gap: 10px; background: var(--panel); border: 1px solid var(--line); border-radius: 8px; box-shadow: 0 6px 24px rgba(0, 0, 0, 0.45); font-family: var(--font); }
  .hd { display: flex; align-items: center; justify-content: space-between; }
  .x { background: none; border: none; color: var(--mut); cursor: pointer; font: inherit; }
  .x:hover { color: var(--fg); }
</style>
