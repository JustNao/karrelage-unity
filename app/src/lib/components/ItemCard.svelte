<script lang="ts">
  import { kamas } from "../api";
  import { fmtDelta, pairRows } from "../compare";
  import { isBelowMin, isNegative, isPerfectLine, perfection, rangeLabel, sortEffects } from "../effects";

  /** One auction lot (see crates/modules/src/auction.rs for the shape). */
  let {
    row,
    collapsible = false,
    open = true,
    ontoggle,
    fade = null,
    onfade,
    pinned = false,
    onpin,
    versus = null,
  }: {
    row: any;
    collapsible?: boolean;
    open?: boolean;
    ontoggle?: () => void;
    fade?: number | null;
    onfade?: (action: number) => void;
    pinned?: boolean;
    onpin?: () => void;
    /** The pinned lot to compare with (null = no pin): its stats are mirrored on the right, differences in the middle. */
    versus?: { pin: any; name: string; unitPrice: (r: any) => number } | null;
  } = $props();

  const effects = $derived(sortEffects<any>(row.effects)); // new first, then stats/main, damage, resistances, utility
  const perfect = $derived(perfection(row.effects));
  const shown = $derived(open ? effects : effects.filter((e: any) => e.status === "added" || e.status === "above"));
  // equipment is sold per unit: only show the lot sizes (x10, x100, x1000) that actually have a price
  const prices = $derived(row.prices.filter((p: any) => p.price > 0));
  /** Row being hovered in the comparison: its effect on both lots and the difference light up together. */
  let hov = $state<number | "price" | null>(null);
  const hover = (k: number | "price") => ({ onmouseenter: () => (hov = k), onmouseleave: () => (hov = null) });
  const pairs = $derived(versus ? pairRows(versus.pin, row) : []);
  const myPrice = $derived(versus ? versus.unitPrice(row) : Infinity);
  const pinPrice = $derived(versus ? versus.unitPrice(versus.pin) : Infinity);
  const priceDelta = $derived(myPrice - pinPrice);
  const hint = (e: any) =>
    `effect ${e.action}` +
    (e.range ? ` · default ${rangeLabel(e.range).slice(1, -1)}` : "") +
    (e.status === "added" ? " · added (not a default effect)" : e.status === "above" ? " · above default max" : e.status === "removed" ? " · removed (default effect missing)" : "");
</script>

{#snippet line(e: any, mirror: boolean)}
  {#if mirror}
    {#if e.range && e.status !== "removed"}<small class="rg">{rangeLabel(e.range)}</small>{/if}{e.text}
  {:else}
    {e.text}{#if e.range && e.status !== "removed"}<small>{rangeLabel(e.range)}</small>{/if}
  {/if}
{/snippet}

{#snippet highlight(e: any)}
  {#if e.status === "added" && onfade}
    <!-- stopPropagation: the card toggles / jumps on click, this button must not -->
    <button class="fade" class:on={fade === e.action} title="Highlight the lots that have this effect: the others fade out (list, graph, good deals)" onclick={(ev) => (ev.stopPropagation(), onfade(e.action))}>
      {fade === e.action ? "Show all" : "Highlight"}
    </button>
  {/if}
{/snippet}

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="card" class:col={collapsible} onclick={() => collapsible && ontoggle?.()}>
  <h4>
    {#if collapsible}<span class="chev">{open ? "▾" : "▸"}</span>{/if}
    {row.name} <span class="mut">{row.level != null ? "niv. " + row.level : ""}</span>
    {#if onpin}
      <button class="pin" class:on={pinned} title="Compare every lot you look at with this one" onclick={(ev) => (ev.stopPropagation(), onpin())}>{pinned ? "Unpin" : "Pin item"}</button>
    {/if}
    <span class="flag" class:on={perfect.damage} title="All default damage effects at their max">★ Perfect damage</span>
    <span class="flag" class:on={perfect.resist} title="All default resistances at their max">★ Perfect resistances</span>
    <span class="flag" class:on={perfect.main} title="Main stats at max (within 3 for force/intelligence/agility/chance/wisdom, 10 for vitality)">★ Perfect main stats</span>
    {#if row.locked}<span class="shield" title="Empêche les futures forgemagies — cannot be modified anymore">🛡 Transcendance</span>{/if}
  </h4>
  <div class="sub">
    {row.type ?? ""} · uid {row.uid ?? "?"}
    {#if row.gid == null} · unresolved ints {JSON.stringify(row.ints)}{/if}
    {#if row.meta?.crafted_by} · fabriqué par {row.meta.crafted_by}{/if}
    {#if row.meta?.modified_by} · modifié par {row.meta.modified_by}{/if}
  </div>

  {#if versus}
    <div class="cmp">
      <div class="hd">This lot</div>
      <div class="hd"></div>
      <div class="hd r">Pinned · {versus.name}</div>

      <div class="l" class:hot={hov === "price"} {...hover("price")}>{#if Number.isFinite(myPrice)}<span class="pr"><b>{kamas(myPrice)} kamas</b></span>{:else}<span class="pr no">—</span>{/if}</div>
      <div class="d" class:hot={hov === "price"} {...hover("price")} class:up={priceDelta < 0} class:down={priceDelta > 0}>{Number.isFinite(priceDelta) ? fmtDelta(priceDelta) : ""}</div>
      <div class="r" class:hot={hov === "price"} {...hover("price")}>{#if Number.isFinite(pinPrice)}<span class="pr"><b>{kamas(pinPrice)} kamas</b></span>{:else}<span class="pr no">—</span>{/if}</div>

      {#each pairs as p (p.key)}
        <div class="l" class:hot={hov === p.key} {...hover(p.key)}>
          {#if p.left}
            <span class="ln {p.left.status}" class:unk={!p.left.known} class:perfect={isPerfectLine(p.left)} class:low={isBelowMin(p.left)} class:neg={isNegative(p.left)} title={hint(p.left)}>
              {@render line(p.left, false)}{@render highlight(p.left)}
            </span>
          {/if}
        </div>
        <div class="d" class:hot={hov === p.key} {...hover(p.key)} class:up={p.delta > 0} class:down={p.delta < 0}>{fmtDelta(p.delta)}</div>
        <div class="r" class:hot={hov === p.key} {...hover(p.key)}>
          {#if p.right}
            <span class="ln {p.right.status}" class:unk={!p.right.known} class:perfect={isPerfectLine(p.right)} class:low={isBelowMin(p.right)} class:neg={isNegative(p.right)} title={hint(p.right)}>
              {@render line(p.right, true)}
            </span>
          {/if}
        </div>
      {/each}
    </div>
  {:else}
    <div class="prices">
      {#each prices as p (p.qty)}
        <span class="pr">×{p.qty} <b>{kamas(p.price)} kamas</b></span>
      {:else}
        <span class="pr no">—</span>
      {/each}
    </div>
    {#if shown.length}
      <ul class="eff">
        {#each shown as e}
          <li class="ln {e.status}" class:unk={!e.known} class:perfect={isPerfectLine(e)} class:low={isBelowMin(e)} class:neg={isNegative(e)} title={hint(e)}>
            {@render line(e, false)}{@render highlight(e)}
          </li>
        {/each}
      </ul>
    {:else if open}
      <div class="none">no effects</div>
    {/if}
  {/if}
</div>

<style>
  .card { border: 1px solid var(--line); border-radius: 8px; padding: 8px 10px; margin: 0 0 8px; background: var(--bg); font-family: system-ui, sans-serif; }
  .card.col { cursor: pointer; }
  h4 { margin: 0 0 2px; font-size: 13px; }
  .sub { color: var(--mut); font-size: 12px; }
  .chev { float: right; color: var(--mut); }
  .prices { display: flex; gap: 6px; flex-wrap: wrap; margin: 6px 0; }
  .pr { border: 1px solid var(--line); border-radius: 6px; padding: 1px 7px; font-variant-numeric: tabular-nums; }
  .pr b { color: var(--ok); }
  .pr.no { opacity: 0.4; }
  .eff { margin: 0; padding-left: 16px; }
  .ln.unk { color: var(--warn); }
  .ln.added { color: var(--acc); font-weight: 700; }
  .ln.above { color: var(--ok); font-weight: 700; }
  .ln.perfect { font-weight: 700; }
  .ln.low { color: #f08a3c; }
  .ln.neg { color: var(--bad); font-weight: 400; }
  .ln.removed { color: var(--mut); opacity: 0.55; }
  .ln small { font-weight: 400; color: var(--mut); margin-left: 6px; }
  .ln small.rg { margin: 0 6px 0 0; }
  /* compact: the pinned stats sit right next to the lot's own, columns hug their content */
  .cmp { display: grid; grid-template-columns: max-content max-content max-content; justify-content: start; row-gap: 1px; align-items: stretch; margin-top: 6px; }
  /* every cell fills the full row height (and centres its text), so the hover bar has no gaps above/below the difference */
  .cmp > .l, .cmp > .r, .cmp > .d { display: flex; align-items: center; }
  .cmp > .r { justify-content: flex-end; }
  .cmp > .d { justify-content: center; }
  .cmp > div { padding: 1px 12px; transition: background 0.12s; }
  .cmp > .hot { background: color-mix(in srgb, var(--acc) 18%, transparent); }
  .cmp > .l.hot { border-radius: 6px 0 0 6px; }
  .cmp > .r.hot { border-radius: 0 6px 6px 0; }
  .cmp .hd { color: var(--mut); font-size: 11px; }
  .cmp .r { text-align: right; }
  .cmp .d { text-align: center; min-width: 52px; cursor: default; font-size: 12px; font-weight: 600; color: var(--mut); font-variant-numeric: tabular-nums; }
  .cmp .d.up { color: var(--ok); }
  .cmp .d.down { color: var(--bad); }
  .pin { margin-left: 8px; padding: 0 8px; font-size: 11px; font-weight: 400; border-radius: 8px; }
  .pin.on { background: color-mix(in srgb, var(--acc) 25%, transparent); border-color: var(--acc); color: var(--acc); }
  .fade { margin-left: 10px; padding: 1px 11px; font-size: 12.5px; font-weight: 400; border-radius: 9px; }
  .fade.on { background: color-mix(in srgb, var(--acc) 25%, transparent); border-color: var(--acc); color: var(--acc); }
  .none { color: var(--mut); font-size: 12px; }
  .flag { display: inline-block; margin-left: 6px; padding: 0 6px; border-radius: 8px; background: transparent; color: var(--mut); opacity: 0.55; font-size: 11px; font-weight: 600; white-space: nowrap; }
  .flag.on { background: color-mix(in srgb, var(--ok) 20%, transparent); color: var(--ok); opacity: 1; }
  .shield { display: inline-block; margin-left: 6px; padding: 0 6px; border-radius: 8px; background: color-mix(in srgb, var(--warn) 25%, transparent); color: var(--warn); font-size: 12px; }
</style>
