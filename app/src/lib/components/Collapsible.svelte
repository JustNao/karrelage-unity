<script lang="ts">
  import type { Snippet } from "svelte";

  /**
   * A header button with a chevron that folds the content below it. The open state is the caller's (`open` + `ontoggle`,
   * or `bind:open`; persist it with lib/persist.svelte.ts when it must be remembered). The header look comes from `head`
   * (a class the caller styles with :global, e.g. the sticky type rows of the HDV list).
   */
  let {
    title,
    count = null,
    open = $bindable(true),
    ontoggle,
    head = "",
    label,
    extra,
    children,
  }: {
    title?: string;
    /** Shown dimmed after the title. */
    count?: number | string | null;
    open?: boolean;
    /** Called instead of flipping `open` (the caller owns the state). */
    ontoggle?: () => void;
    head?: string;
    /** Header content instead of `title` (after the chevron). */
    label?: Snippet;
    /** Extra header content after the count. */
    extra?: Snippet;
    children: Snippet;
  } = $props();

  function toggle() {
    if (ontoggle) return ontoggle();
    open = !open;
  }
</script>

<button class="chd {head}" aria-expanded={open} onclick={toggle}>
  <span class="chev" class:shut={!open}>▾</span>
  {#if label}{@render label()}{:else}{title}{/if}
  {#if count != null}<span class="mut">{count}</span>{/if}
  {@render extra?.()}
</button>
{#if open}{@render children()}{/if}

