<script lang="ts" generics="T">
  import { onMount, tick, type Snippet } from "svelte";

  /**
   * Renders only the rows of a long list that are on screen (plus `overscan` rows on each side). Rows may have any height:
   * each rendered row is measured (ResizeObserver) and its height remembered by key; rows never seen use `estimate`.
   * Scrolls with its nearest scrollable ancestor (e.g. SmoothScroll), so it can sit below other content in the same scroller.
   *
   *   <VirtualList items={rows} key={(r) => r.id} estimate={90}>{#snippet row(r)}<Card {r} />{/snippet}</VirtualList>
   *
   * `scrollToKey(key)` (via bind:this) brings a row into view even when it is not rendered yet.
   */
  let {
    items,
    key,
    row,
    estimate = 120,
    overscan = 6,
    id,
  }: {
    items: T[];
    key: (item: T) => string | number;
    row: Snippet<[T]>;
    estimate?: number;
    overscan?: number;
    /** Optional DOM id of each row wrapper (e.g. so `document.getElementById` finds it once rendered). */
    id?: (item: T) => string;
  } = $props();

  let root: HTMLDivElement;
  let scroller: HTMLElement | null = null;
  /** Heights by key; not reactive by itself, `version` is bumped when it changes. */
  const heights = new Map<string | number, number>();
  let version = $state(0);
  let view = $state({ top: 0, height: 800 });

  const keys = $derived(items.map(key));
  /** offsets[i] = top of row i, offsets[n] = total height. */
  const offsets = $derived.by(() => {
    version;
    const o = new Array<number>(keys.length + 1);
    o[0] = 0;
    for (let i = 0; i < keys.length; i++) o[i + 1] = o[i] + (heights.get(keys[i]) ?? estimate);
    return o;
  });
  /** First row whose bottom is below y. */
  const find = (y: number) => {
    let lo = 0, hi = keys.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (offsets[mid + 1] <= y) lo = mid + 1;
      else hi = mid;
    }
    return lo;
  };
  const range = $derived.by(() => {
    const a = Math.max(0, find(view.top) - overscan);
    const b = Math.min(keys.length, find(view.top + view.height) + 1 + overscan);
    return { a, b };
  });
  const slice = $derived(items.slice(range.a, range.b));

  /** Where the visible window of the scroller falls inside this list. */
  function measureView() {
    if (!scroller || !root) return;
    const top = scroller.scrollTop - (root.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop);
    const height = scroller.clientHeight;
    if (Math.abs(top - view.top) > 1 || height !== view.height) view = { top, height };
  }
  let frame = 0;
  const schedule = () => {
    if (!frame) frame = requestAnimationFrame(() => ((frame = 0), measureView()));
  };

  const ro = new ResizeObserver((entries) => {
    let changed = false;
    for (const e of entries) {
      const el = e.target as HTMLElement;
      const k = (el as any).__vk;
      const h = el.offsetHeight;
      if (k != null && h > 0 && Math.abs((heights.get(k) ?? -1) - h) > 0.5) {
        heights.set(k, h);
        changed = true;
      }
    }
    if (changed) version++;
  });
  /** Measures a rendered row (its key is kept on the element for the observer). */
  function measure(el: HTMLElement, k: string | number) {
    (el as any).__vk = k;
    ro.observe(el);
    return {
      update(nk: string | number) {
        (el as any).__vk = nk;
      },
      destroy() {
        ro.unobserve(el);
      },
    };
  }

  // the list moves inside the scroller when its rows or the content above it change, without any scroll event
  $effect(() => {
    offsets;
    schedule();
  });

  onMount(() => {
    let p = root.parentElement;
    while (p && !/(auto|scroll)/.test(getComputedStyle(p).overflowY)) p = p.parentElement;
    scroller = p ?? document.documentElement;
    scroller.addEventListener("scroll", schedule, { passive: true });
    const sro = new ResizeObserver(schedule);
    sro.observe(scroller);
    for (const c of scroller.children) sro.observe(c); // content above the list growing / shrinking moves it
    measureView();
    return () => {
      cancelAnimationFrame(frame);
      scroller?.removeEventListener("scroll", schedule);
      sro.disconnect();
      ro.disconnect();
    };
  });

  /** Scroll so the row with this key is centred (rendering it first if needed). */
  export async function scrollToKey(k: string | number, behavior: ScrollBehavior = "smooth") {
    const i = keys.indexOf(k);
    if (i < 0 || !scroller) return;
    const sc = scroller;
    const el = () => root.querySelector<HTMLElement>(`[data-vk="${CSS.escape(String(k))}"]`);
    // already rendered: every row in between is measured, a smooth scroll lands right
    const near = el();
    if (near) return near.scrollIntoView({ behavior, block: "center" });
    // far away: jump to the estimated position, let the rows rendered there be measured, aim again (a few rounds converge)
    for (let round = 0; round < 5; round++) {
      const e = el();
      const top = e
        ? sc.scrollTop + e.getBoundingClientRect().top - sc.getBoundingClientRect().top - (sc.clientHeight - e.offsetHeight) / 2
        : root.getBoundingClientRect().top - sc.getBoundingClientRect().top + sc.scrollTop + offsets[i] - (sc.clientHeight - (offsets[i + 1] - offsets[i])) / 2;
      if (e && Math.abs(top - sc.scrollTop) < 2) break;
      sc.scrollTop = top;
      measureView();
      await tick();
      // two frames: ResizeObserver measures the new rows after the first frame's layout (timeout: frames are paused in a hidden window)
      for (let f = 0; f < 2; f++) await new Promise((r) => (requestAnimationFrame(r), setTimeout(r, 50)));
    }
  }
</script>

<!-- in-flow spacers (not a transform): when rows above the viewport get measured, the browser's scroll anchoring keeps what is on screen in place -->
<div class="vlist" bind:this={root}>
  <div class="vpad" style:height="{offsets[range.a]}px"></div>
  {#each slice as it (key(it))}
    <div class="vrow" data-vk={key(it)} id={id?.(it)} use:measure={key(it)}>{@render row(it)}</div>
  {/each}
  <div class="vpad" style:height="{offsets[keys.length] - offsets[range.b]}px"></div>
</div>

<style>
  /* the spacers must never be picked as scroll anchors */
  .vpad { overflow-anchor: none; }
  /* flow-root: a child's margins stay inside the row, so the measured height is the real step between rows */
  .vrow { display: flow-root; }
</style>
