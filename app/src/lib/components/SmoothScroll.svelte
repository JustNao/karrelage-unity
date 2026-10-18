<script lang="ts">
  import { onMount, type Snippet } from "svelte";

  /**
   * Scroll container with a custom bar: black track with a slightly lighter edge. When the content overflows, the track
   * slides in from the right edge (i.e. from under the neighbouring panel) while the content padding eases left,
   * instead of the native bar popping in and shifting everything instantly.
   */
  let { children, pad = "12px", maxHeight, toTop = false }: { children: Snippet; pad?: string; maxHeight?: string; /** Floating "back to top" button once scrolled down. */ toTop?: boolean } = $props();

  let scroller: HTMLElement;
  let inner: HTMLElement;
  let overflow = $state(false);
  let thumbH = $state(0);
  let thumbY = $state(0);
  let dragging = $state(false);
  let scrolled = $state(false);
  let grab = { y: 0, top: 0 };

  function measure() {
    if (!scroller) return;
    const { scrollHeight: sh, clientHeight: ch, scrollTop: st } = scroller;
    overflow = sh > ch + 1;
    scrolled = st > 400;
    thumbH = Math.max(32, (ch * ch) / sh);
    thumbY = sh > ch ? (st / (sh - ch)) * (ch - thumbH) : 0;
  }

  onMount(() => {
    const ro = new ResizeObserver(measure);
    ro.observe(scroller);
    ro.observe(inner);
    measure();
    return () => ro.disconnect();
  });

  function down(e: PointerEvent) {
    dragging = true;
    grab = { y: e.clientY, top: scroller.scrollTop };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    e.preventDefault();
  }
  function move(e: PointerEvent) {
    if (!dragging) return;
    const { scrollHeight: sh, clientHeight: ch } = scroller;
    scroller.scrollTop = grab.top + ((e.clientY - grab.y) * (sh - ch)) / (ch - thumbH);
  }
  function trackClick(e: MouseEvent) {
    if (e.target !== e.currentTarget) return;
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const { scrollHeight: sh, clientHeight: ch } = scroller;
    scroller.scrollTop = ((e.clientY - r.top - thumbH / 2) / (ch - thumbH)) * (sh - ch);
  }
</script>

<div class="wrap">
  <div class="scroller" class:over={overflow} bind:this={scroller} onscroll={measure} style:--pad={pad} style:height={maxHeight ? "auto" : "100%"} style:max-height={maxHeight}>
    <div bind:this={inner}>{@render children()}</div>
  </div>
  {#if toTop}
    <button class="top" class:show={scrolled} aria-label="Back to top" title="Back to top" tabindex={scrolled ? 0 : -1} onclick={() => scroller.scrollTo({ top: 0, behavior: "smooth" })}>
      <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 13V3M3.5 7.5 8 3l4.5 4.5" /></svg>
    </button>
  {/if}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="track" class:on={overflow} onclick={trackClick}>
    <div
      class="thumb"
      class:drag={dragging}
      style:height="{thumbH}px"
      style:transform="translateY({thumbY}px)"
      onpointerdown={down}
      onpointermove={move}
      onpointerup={() => (dragging = false)}
      onpointercancel={() => (dragging = false)}
    ></div>
  </div>
</div>

<style>
  .wrap { position: relative; overflow: hidden; min-height: 0; min-width: 0; }
  .scroller {
    height: 100%;
    overflow-y: auto;
    padding: var(--pad, 12px);
    scrollbar-width: none;
    transition: padding-right 0.28s cubic-bezier(0.3, 0.7, 0.3, 1);
  }
  .scroller::-webkit-scrollbar { display: none; }
  .scroller.over { padding-right: calc(var(--pad, 12px) + 14px); }
  .top { position: absolute; right: 28px; bottom: 16px; width: 36px; height: 36px; padding: 0; display: grid; place-items: center; border-radius: 50%; background: var(--panel); border: 1px solid var(--acc); color: var(--acc); box-shadow: 0 2px 10px #0008; opacity: 0; pointer-events: none; transform: translateY(8px); transition: opacity 0.2s, transform 0.2s; z-index: 5; }
  .top.show { opacity: 1; pointer-events: auto; transform: none; }
  .top:hover { background: var(--bg); }
  .track {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: 14px;
    background: #000;
    border-left: 1px solid #343944;
    transform: translateX(100%);
    transition: transform 0.28s cubic-bezier(0.3, 0.7, 0.3, 1);
  }
  .track.on { transform: translateX(0); }
  .thumb {
    position: absolute;
    top: 0;
    left: 2px;
    right: 2px;
    border-radius: 6px;
    background: #2b2f38;
    border: 1px solid #454b58;
    cursor: grab;
    transition: background 0.15s, border-color 0.15s;
  }
  .thumb:hover, .thumb.drag { background: #3a3f4b; border-color: #5a6272; }
  .thumb.drag { cursor: grabbing; }
</style>
