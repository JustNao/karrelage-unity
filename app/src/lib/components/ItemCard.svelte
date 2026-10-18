<script lang="ts">
  import { kamas } from "../api";
  import type { Chip } from "../compare";
  import { isBelowMin, isNegative, isPerfectLine, perfection, sortEffects } from "../effects";

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
    chips = null,
  }: {
    row: any;
    collapsible?: boolean;
    open?: boolean;
    ontoggle?: () => void;
    fade?: number | null;
    onfade?: (action: number) => void;
    pinned?: boolean;
    onpin?: () => void;
    /** Differences against the pinned lot (null = no pin). */
    chips?: Chip[] | null;
  } = $props();

  const effects = $derived(sortEffects<any>(row.effects)); // new first, then stats/main, damage, resistances, utility
  const perfect = $derived(perfection(row.effects));
  const shown = $derived(open ? effects : effects.filter((e: any) => e.status === "added" || e.status === "above"));
  // equipment is sold per unit: only show the lot sizes (x10, x100, x1000) that actually have a price
  const prices = $derived(row.prices.filter((p: any) => p.price > 0));
  const hint = (e: any) =>
    `effect ${e.action}` +
    (e.range ? ` · default ${e.range[0]}–${e.range[1]}` : "") +
    (e.status === "added" ? " · added (not a default effect)" : e.status === "above" ? " · above default max" : e.status === "removed" ? " · removed (default effect missing)" : "");
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="card" class:col={collapsible} onclick={() => collapsible && ontoggle?.()}>
  <h4>
    {#if collapsible}<span class="chev">{open ? "▾" : "▸"}</span>{/if}
    {row.name} <span class="mut">{row.level != null ? "niv. " + row.level : ""}</span>
    <span class="flag" class:on={perfect.damage} title="All default damage effects at their max">★ Perfect damage</span>
    <span class="flag" class:on={perfect.resist} title="All default resistances at their max">★ Perfect resistances</span>
    {#if onpin}
      <button class="pin" class:on={pinned} title="Compare every lot you look at with this one" onclick={(ev) => (ev.stopPropagation(), onpin())}>{pinned ? "Unpin" : "Pin item"}</button>
    {/if}
    <span class="flag" class:on={perfect.main} title="Main stats at max (within 3 for force/intelligence/agility/chance/wisdom, 10 for vitality)">★ Perfect main stats</span>
    {#if row.locked}<span class="shield" title="Empêche les futures forgemagies — cannot be modified anymore">🛡 Transcendance</span>{/if}
  </h4>
  <div class="sub">
    {row.type ?? ""} · uid {row.uid ?? "?"}
    {#if row.gid == null} · unresolved ints {JSON.stringify(row.ints)}{/if}
    {#if row.meta?.crafted_by} · fabriqué par {row.meta.crafted_by}{/if}
    {#if row.meta?.modified_by} · modifié par {row.meta.modified_by}{/if}
  </div>
  {#if chips}
    <div class="chips">
      {#each chips as c}<span class="chip {c.tone}">{c.text}</span>{:else}<span class="chip same">Same as the pinned item</span>{/each}
    </div>
  {/if}
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
        <li class={e.status} class:unk={!e.known} class:perfect={isPerfectLine(e)} class:low={isBelowMin(e)} class:neg={isNegative(e)} title={hint(e)}>
          {e.text}{#if e.range && e.status !== "removed"}<small>[{e.range[0]}–{e.range[1]}]</small>{/if}
          {#if e.status === "added" && onfade}
            <!-- stopPropagation: the card toggles / jumps on click, this button must not -->
            <button class="fade" class:on={fade === e.action} title="Highlight the lots that have this effect: the others fade out (list, graph, good deals)" onclick={(ev) => (ev.stopPropagation(), onfade(e.action))}>
              {fade === e.action ? "Show all" : "Highlight"}
            </button>
          {/if}
        </li>
      {/each}
    </ul>
  {:else if open}
    <div class="none">no effects</div>
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
  .eff li.unk { color: var(--warn); }
  .eff li.added { color: var(--acc); font-weight: 700; }
  .eff li.above { color: var(--ok); font-weight: 700; }
  .eff li.perfect { font-weight: 700; }
  .eff li.low { color: #f08a3c; }
  .eff li.neg { color: var(--bad); font-weight: 400; }
  .eff li.removed { color: var(--mut); opacity: 0.55; }
  .pin { margin-left: 8px; padding: 0 8px; font-size: 11px; font-weight: 400; border-radius: 8px; }
  .pin.on { background: color-mix(in srgb, var(--acc) 25%, transparent); border-color: var(--acc); color: var(--acc); }
  .chips { display: flex; flex-wrap: wrap; gap: 4px; margin: 6px 0 0; }
  .chip { padding: 0 7px; border-radius: 8px; font-size: 11px; border: 1px solid var(--line); }
  .chip.up { color: var(--ok); border-color: color-mix(in srgb, var(--ok) 45%, var(--line)); }
  .chip.down { color: var(--bad); border-color: color-mix(in srgb, var(--bad) 45%, var(--line)); }
  .chip.same { color: var(--mut); }
  .fade { margin-left: 10px; padding: 1px 11px; font-size: 12.5px; font-weight: 400; border-radius: 9px; }
  .fade.on { background: color-mix(in srgb, var(--acc) 25%, transparent); border-color: var(--acc); color: var(--acc); }
  .eff li small { font-weight: 400; color: var(--mut); margin-left: 6px; }
  .none { color: var(--mut); font-size: 12px; }
  .flag { display: inline-block; margin-left: 6px; padding: 0 6px; border-radius: 8px; background: transparent; color: var(--mut); opacity: 0.55; font-size: 11px; font-weight: 600; white-space: nowrap; }
  .flag.on { background: color-mix(in srgb, var(--ok) 20%, transparent); color: var(--ok); opacity: 1; }
  .shield { display: inline-block; margin-left: 6px; padding: 0 6px; border-radius: 8px; background: color-mix(in srgb, var(--warn) 25%, transparent); color: var(--warn); font-size: 12px; }
</style>
