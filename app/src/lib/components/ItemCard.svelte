<script lang="ts">
  import { kamas } from "../api";
  import { sortEffects } from "../effects";

  /** One auction lot (see crates/modules/src/auction.rs for the shape). */
  let { row, collapsible = false, open = true, ontoggle }: { row: any; collapsible?: boolean; open?: boolean; ontoggle?: () => void } = $props();

  const effects = $derived(sortEffects<any>(row.effects)); // new first, then stats/main, damage, resistances, utility
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
    {#if row.locked}<span class="shield" title="Empêche les futures forgemagies — cannot be modified anymore">🛡 Transcendance</span>{/if}
  </h4>
  <div class="sub">
    {row.type ?? ""} · uid {row.uid ?? "?"}
    {#if row.gid == null} · unresolved ints {JSON.stringify(row.ints)}{/if}
    {#if row.meta?.crafted_by} · fabriqué par {row.meta.crafted_by}{/if}
    {#if row.meta?.modified_by} · modifié par {row.meta.modified_by}{/if}
  </div>
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
        <li class={e.status} class:unk={!e.known} title={hint(e)}>
          {e.text}{#if e.range && e.status !== "removed"}<small>[{e.range[0]}–{e.range[1]}]</small>{/if}
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
  .eff li.removed { color: var(--mut); opacity: 0.55; }
  .eff li small { font-weight: 400; color: var(--mut); margin-left: 6px; }
  .none { color: var(--mut); font-size: 12px; }
  .shield { display: inline-block; margin-left: 6px; padding: 0 6px; border-radius: 8px; background: color-mix(in srgb, var(--warn) 25%, transparent); color: var(--warn); font-size: 12px; }
</style>
