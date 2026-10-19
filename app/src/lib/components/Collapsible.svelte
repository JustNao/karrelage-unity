<script lang="ts">
  import { untrack, type Snippet } from "svelte";

  /**
   * A header button with a chevron that folds the content below it. The open state is either the caller's (`open` +
   * `ontoggle`, or `bind:open`) or remembered in localStorage under `storageKey`. The header look comes from `head`
   * (a class the caller styles with :global, e.g. the sticky type rows of the HDV list).
   */
  let {
    title,
    count = null,
    open = $bindable(true),
    storageKey,
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
    storageKey?: string;
    /** Called instead of flipping `open` (the caller owns the state). */
    ontoggle?: () => void;
    head?: string;
    /** Header content instead of `title` (after the chevron). */
    label?: Snippet;
    /** Extra header content after the count. */
    extra?: Snippet;
    children: Snippet;
  } = $props();

  const key = untrack(() => storageKey); // fixed for the component's life
  if (key) {
    try {
      const v = localStorage.getItem(key);
      if (v !== null) open = v === "1";
    } catch {}
  }

  function toggle() {
    if (ontoggle) return ontoggle();
    open = !open;
    if (key) {
      try {
        localStorage.setItem(key, open ? "1" : "0");
      } catch {}
    }
  }
</script>

<button class="chd {head}" aria-expanded={open} onclick={toggle}>
  <span class="chev" class:shut={!open}>▾</span>
  {#if label}{@render label()}{:else}{title}{/if}
  {#if count != null}<span class="mut">{count}</span>{/if}
  {@render extra?.()}
</button>
{#if open}{@render children()}{/if}

