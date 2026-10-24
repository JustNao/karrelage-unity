<script lang="ts">
  import type { Snippet } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";

  /**
   * Root of a floating window (lib/floating.ts): rounded translucent box, a title bar to drag the window by (the title's
   * content passes the drag through), action buttons, then the scrolling content. `up`: the title bar sits under the
   * content (a window anchored at the bottom of the screen). `top` / `body` give the elements to measure (height fitting).
   */
  let {
    title,
    actions,
    children,
    up = false,
    closable = true,
    shown = true,
    top = $bindable(),
    body = $bindable(),
  }: {
    title: Snippet;
    /** Buttons before the close button (class "x" for the title-bar look). */
    actions?: Snippet;
    children: Snippet;
    up?: boolean;
    closable?: boolean;
    /** False: nothing drawn yet (the window is transparent); true fades the box in. */
    shown?: boolean;
    top?: HTMLElement;
    body?: HTMLElement;
  } = $props();
</script>

<div class="box" class:up class:hide={!shown}>
  <div class="top" bind:this={top} data-tauri-drag-region>
    <span class="t" data-tauri-drag-region>{@render title()}</span>
    {@render actions?.()}
    {#if closable}<button class="x" onclick={() => getCurrentWindow().close()} title="Fermer">✕</button>{/if}
  </div>
  <div class="list"><div bind:this={body}>{@render children()}</div></div>
</div>

<style>
  .box { height: 100%; box-sizing: border-box; display: flex; flex-direction: column; background: rgba(28, 31, 38, 0.86); border: 1px solid rgba(90, 169, 255, 0.45); border-radius: 10px; overflow: hidden; font-family: var(--font); font-size: 12px; }
  .box.up { flex-direction: column-reverse; }
  .box { transition: opacity 0.25s ease-out; }
  .box.hide { opacity: 0; }
  .top { flex: none; display: flex; align-items: center; gap: 8px; padding: 7px 10px 7px 12px; background: rgba(20, 22, 26, 0.7); cursor: move; user-select: none; }
  .t { flex: 1; font-weight: 600; display: inline-flex; align-items: center; gap: 6px; }
  /* the title's parts let the drag through to the drag region */
  .t :global(*) { pointer-events: none; }
  .top :global(.x) { border: 0; background: none; color: var(--mut); padding: 0 4px; font-size: 13px; }
  .top :global(.x:hover) { color: var(--bad); }
  .list { flex: 1; min-height: 0; overflow: auto; }
  /* the measured content keeps its children's margins inside (else they collapse out of it and the fitted window is short) */
  .list > div { display: flow-root; }
</style>
