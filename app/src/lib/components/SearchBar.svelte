<script lang="ts" module>
  import { norm } from "../effects";

  /** A matcher for what was typed: accent / case-insensitive, true for every text when the box is empty. */
  export function matcher(q: string) {
    const n = norm(q.trim());
    return (...texts: (string | null | undefined)[]) => !n || texts.some((t) => !!t && norm(t).includes(n));
  }
</script>

<script lang="ts">
  import type { Snippet } from "svelte";

  /**
   * Search box of the tabs (always matched with `matcher`, i.e. accent / case-insensitive), with an optional
   * "Prix connus seulement" checkbox and room for extra controls before the box (e.g. a family select).
   */
  let {
    value = $bindable(""),
    placeholder = "Rechercher…",
    priced = $bindable(undefined),
    width = "200px",
    grow = false,
    compact = false,
    before,
    after,
  }: {
    value?: string;
    placeholder?: string;
    /** Bind it to show the "Prix connus seulement" checkbox. */
    priced?: boolean;
    width?: string;
    /** The bar and its box take the free width (side panels). */
    grow?: boolean;
    /** No margin below (when the caller lays it out). */
    compact?: boolean;
    before?: Snippet;
    after?: Snippet;
  } = $props();
</script>

<div class="sbar" class:compact class:grow>
  {@render before?.()}
  <input type="search" {placeholder} bind:value style:width={grow ? null : width} class:grow />
  {#if priced !== undefined}<label><input type="checkbox" bind:checked={priced} /> Prix connus seulement</label>{/if}
  {@render after?.()}
</div>

<style>
  .sbar { display: flex; align-items: center; gap: 10px; margin-bottom: 10px; flex-wrap: wrap; }
  .sbar.compact { margin-bottom: 0; gap: 6px; flex-wrap: nowrap; }
  .sbar.grow { flex: 1; min-width: 0; }
  input[type="search"] { padding: 3px 6px; min-width: 0; }
  input.grow { flex: 1; }
</style>
