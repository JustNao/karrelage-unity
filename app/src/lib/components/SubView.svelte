<script lang="ts">
  import type { Snippet } from "svelte";
  import Banner from "./Banner.svelte";
  import SmoothScroll from "./SmoothScroll.svelte";

  /**
   * Shell of a table page (Crafting sub-tabs): title, the controls (`top`: search bar, form, summary...), the load error
   * if any, then the scrolling content (usually a SortTable). `class` names the root for the page's :global styles.
   */
  let { title, err = "", top, children, class: cls = "" }: { title: string; err?: string; top?: Snippet; children: Snippet; class?: string } = $props();
</script>

<div class="rv {cls}">
  <div class="head"><div class="fh">{title}</div></div>
  {@render top?.()}
  {#if err}<Banner inset>Erreur : {err} (relancez l'application pour charger la dernière version)</Banner>{/if}
  <SmoothScroll>{@render children()}</SmoothScroll>
</div>

<style>
  .rv { flex: 1; min-height: 0; display: flex; flex-direction: column; font-family: var(--font); padding: 8px 10px 0; }
  .rv > :global(.wrap) { flex: 1; }
  .head { display: flex; align-items: center; gap: 10px; margin-bottom: 10px; flex-wrap: wrap; }
  .head .fh { margin: 0; flex: 1; min-width: 260px; }
</style>
