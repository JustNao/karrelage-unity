<script lang="ts">
  import { fmtInt, kamasFull } from "../fmt";
  import { unitPrice } from "../price";
  import { app } from "../app.svelte";
  import { iconOfElement, iconOfStat } from "../stores/statIcons.svelte";
  import { fmtDelta, pairRows } from "../compare";
  import { isLegendary, levelOf, xpOfLevel } from "../companion";
  import { HIDDEN_EFFECTS, LEGENDARY_EFFECT, isBelowMin, isNegative, isPerfectLine, isWeaponEffect, perfection, rangeLabel, sortEffects } from "../effects";

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
    companion = false,
    flags = true,
    priced = true,
    inTable = false,
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
    /** Familier / montilier: only the level and the legendary status are shown (no stat flags, no pin, no transcendance). */
    companion?: boolean;
    versus?: { pin: any; name: string } | null;
    /** Perfection flags (damage / resistances / characteristics) and the transcendance shield in the header. */
    flags?: boolean;
    /** False for an item that is not for sale (inventory): no price row. */
    priced?: boolean;
    /** Unfolded under its row of the HDV table: the row already shows the price and the perfection flags. */
    inTable?: boolean;
  } = $props();

  const level = $derived(companion ? levelOf(row) : null);
  const legendary = $derived(companion && isLegendary(row));
  const effects = $derived(sortEffects<any>(row.effects.filter((e: any) => !HIDDEN_EFFECTS.has(e.action) && !/^(Exp[ée]rience\s*:|Attitude)/i.test(e.text ?? "")))); // new first, then stats/main, damage, resistances, utility
  const perfect = $derived(perfection(row.effects));
  const shown = $derived(open ? effects : effects.filter((e: any) => e.status === "added" || e.status === "above"));
  // the weapon's own effects (hits, -PA, pushback...) are not stats: listed apart, plain, without range
  const weaponEffs = $derived(shown.filter(isWeaponEffect));
  const statEffs = $derived(shown.filter((e: any) => !isWeaponEffect(e)));
  // equipment is sold per unit: only show the lot sizes (x10, x100, x1000) that actually have a price
  const prices = $derived(row.prices.filter((p: any) => p.price > 0));
  /** Row being hovered in the comparison: its effect on both lots and the difference light up together. */
  let hov = $state<number | "price" | null>(null);
  const hover = (k: number | "price") => ({ onmouseenter: () => (hov = k), onmouseleave: () => (hov = null) });
  const allPairs = $derived(versus ? pairRows(versus.pin, row) : []);
  // like the plain list: a folded lot only shows its new / above-max stats, the rest comes with the unfolding
  const pairs = $derived(open ? allPairs : allPairs.filter((p: any) => [p.left, p.right].some((x: any) => x && (x.status === "added" || x.status === "above"))));
  const myPrice = $derived(versus ? unitPrice(row) : Infinity);
  const pinPrice = $derived(versus ? unitPrice(versus.pin) : Infinity);
  const priceDelta = $derived(myPrice - pinPrice);
  /** Folded lot: an over-max line only shows what it has above its default max ("+12 Vitalité" instead of "412 Vitalité"). */
  const excessText = (e: any): string | null =>
    !open && !versus && e.status === "above" && e.range && e.hi != null && !isNegative(e) && /^\d+/.test(e.text ?? "") ? String(e.text).replace(/^\d+/, `+${e.hi - e.range[1]}`) : null;
  const hint = (e: any) =>
    `effet ${e.action}` +
    (e.range ? ` · de base ${rangeLabel(e.range).slice(1, -1)}` : "") +
    (e.status === "added" ? " · EXO (pas un effet de base)" : e.status === "above" ? ` · au-dessus du max de base (${e.text})` : e.status === "removed" ? " · retiré (effet de base manquant)" : "");
</script>

{#snippet icon(e: any)}
  {@const src = isWeaponEffect(e) ? iconOfElement(e.text) : iconOfStat(e.stat)}
  {#if src}<img class="sic" {src} alt="" />{:else}<span class="sic"></span>{/if}
{/snippet}

{#snippet line(e: any, mirror: boolean)}
  {#if isWeaponEffect(e)}
    {#if mirror}{e.text}{@render icon(e)}{:else}{@render icon(e)}{e.text}{/if}
  {:else if mirror}
    {#if e.range && e.status !== "removed" && e.action !== LEGENDARY_EFFECT}<small class="rg">{rangeLabel(e.range)}</small>{/if}{e.text}{@render icon(e)}
  {:else if excessText(e)}
    {@render icon(e)}{excessText(e)}
  {:else}
    {@render icon(e)}{e.text}{#if e.range && e.status !== "removed" && e.action !== LEGENDARY_EFFECT}<small>{rangeLabel(e.range)}</small>{/if}
  {/if}
{/snippet}

{#snippet highlight(e: any)}
  {#if e.status === "added" && onfade}
    <!-- stopPropagation: the card toggles / jumps on click, this button must not -->
    <button class="fade" class:on={fade === e.action} title="Mettre en avant les lots qui ont cet effet : les autres sont estompés (liste, graphique, bonnes affaires)" onclick={(ev) => (ev.stopPropagation(), onfade(e.action))}>
      {fade === e.action ? "Tout afficher" : "Mettre en avant"}
    </button>
  {/if}
{/snippet}

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="card" class:col={collapsible} onclick={() => collapsible && ontoggle?.()}>
  {#if (!versus || onpin) && priced && !(inTable && !onpin)}
    <div class="prices">
      {#if !versus && !inTable}
        {#each prices as p (p.qty)}
          <span class="price">{#if p.qty > 1}×{p.qty} {/if}{kamasFull(p.price)}</span>
        {:else}
          <span class="price no">—</span>
        {/each}
      {/if}
      {#if onpin}
        <button class="pin" class:on={pinned} title="Comparer chaque lot consulté avec celui-ci" onclick={(ev) => (ev.stopPropagation(), onpin())}>{pinned ? "Désépingler" : "Épingler"}</button>
      {/if}
    </div>
  {/if}
  <!-- in the HDV table, an unlocked lot has nothing left for the title line -->
  {#if !(inTable && !row.locked)}
  <h4>
    {#if collapsible}<span class="chev">{open ? "▾" : "▸"}</span>{/if}
    {#if companion}
      <span class="flag on" title="Niveau (xp totale {level != null ? fmtInt(xpOfLevel(level)) : '?'})">{level != null ? "Niveau " + level : "Niveau ?"}</span>
      <span class="flag" class:on={legendary}>★ Statut légendaire</span>
    {:else if flags}
    {#if !inTable}
    <span class="flag" class:on={perfect.damage} title="Tous les effets de dommages de base à leur maximum">★ Dommages parfaits</span>
    <span class="flag" class:on={perfect.resist} title="Résistances de base à leur maximum (seulement les % de résistance quand l'objet en a)">★ Résistances parfaites</span>
    <span class="flag" class:on={perfect.main} title="Caractéristiques principales au maximum (à 3 près pour force/intelligence/agilité/chance/sagesse, 10 pour la vitalité)">★ Caractéristiques parfaites</span>
    {/if}
    {#if row.locked}<span class="shield" title="Empêche les futures forgemagies — ne peut plus être modifié">🛡 Transcendance</span>{/if}
    {/if}
  </h4>
  {/if}
  {#if app.dev}
    <div class="sub">
      uid {row.uid ?? "?"} · gid {row.gid ?? "?"}
      {#if row.gid == null} · entiers non résolus {JSON.stringify(row.ints)}{/if}
    </div>
  {/if}

  {#if versus}
    <div class="cmp">
      <div class="hd">Ce lot</div>
      <div class="hd"></div>
      <div class="hd r">Épinglé · {versus.name}</div>

      <div class="l" class:hot={hov === "price"} {...hover("price")}>{#if Number.isFinite(myPrice)}<span class="price">{kamasFull(myPrice)}</span>{:else}<span class="price no">—</span>{/if}</div>
      <div class="d" class:hot={hov === "price"} {...hover("price")} class:up={priceDelta < 0} class:down={priceDelta > 0}>{open && Number.isFinite(priceDelta) ? (priceDelta === 0 ? "=" : (priceDelta > 0 ? "+" : "") + kamasFull(priceDelta)) : ""}</div>
      <div class="r" class:hot={hov === "price"} {...hover("price")}>{#if Number.isFinite(pinPrice)}<span class="price">{kamasFull(pinPrice)}</span>{:else}<span class="price no">—</span>{/if}</div>

      {#each pairs as p (p.key)}
        <div class="l" class:hot={hov === p.key} {...hover(p.key)}>
          {#if p.left}
            <span class="ln {isWeaponEffect(p.left) && p.left.status !== 'added' && p.left.status !== 'removed' ? '' : p.left.status}" class:unk={!p.left.known} class:perfect={isPerfectLine(p.left)} class:low={isBelowMin(p.left)} class:neg={isNegative(p.left)} class:legend={p.left.action === LEGENDARY_EFFECT} title={hint(p.left)}>
              {@render line(p.left, false)}{@render highlight(p.left)}
            </span>
          {/if}
        </div>
        <div class="d" class:hot={hov === p.key} {...hover(p.key)} class:up={p.delta > 0} class:down={p.delta < 0}>{open ? fmtDelta(p.delta) : ""}</div>
        <div class="r" class:hot={hov === p.key} {...hover(p.key)}>
          {#if p.right}
            <span class="ln {isWeaponEffect(p.right) && p.right.status !== 'added' && p.right.status !== 'removed' ? '' : p.right.status}" class:unk={!p.right.known} class:perfect={isPerfectLine(p.right)} class:low={isBelowMin(p.right)} class:neg={isNegative(p.right)} class:legend={p.right.action === LEGENDARY_EFFECT} title={hint(p.right)}>
              {@render line(p.right, true)}
            </span>
          {/if}
        </div>
      {/each}
    </div>
  {:else}
    {#if companion}
      <!-- level and legendary status are in the title -->
    {:else if shown.length}
      {#if weaponEffs.length}
        <ul class="eff wpn">
          {#each weaponEffs as e}
            <li class="ln {e.status === 'added' || e.status === 'removed' ? e.status : ''}" title={hint(e)}>{@render icon(e)}{e.text}</li>
          {/each}
        </ul>
      {/if}
      <ul class="eff">
        {#each statEffs as e}
          <li class="ln {e.status}" class:unk={!e.known} class:perfect={isPerfectLine(e)} class:low={isBelowMin(e)} class:neg={isNegative(e)} class:legend={e.action === LEGENDARY_EFFECT} title={hint(e)}>
            {@render line(e, false)}{@render highlight(e)}
          </li>
        {/each}
      </ul>
    {:else if open}
      <div class="none">aucun effet</div>
    {/if}
  {/if}
</div>

<style>
  /* off-screen lots are not laid out / painted (long listings scroll much lighter); "auto" remembers the real height once seen */
  .card { content-visibility: auto; contain-intrinsic-size: auto 150px; border: 1px solid var(--line); border-radius: 8px; padding: 8px 10px; margin: 0 0 8px; background: var(--bg); font-family: var(--font); }
  .card.col { cursor: pointer; }
  h4 { margin: 0 0 8px; font-size: 13px; }
  .sub { color: var(--mut); font-size: 12px; }
  .chev { float: right; color: var(--mut); }
  .prices { display: flex; gap: 14px; flex-wrap: wrap; align-items: center; margin: 0 0 2px; }
  .price { font-weight: 700; color: var(--fg); font-size: 14px; }
  .price.no { opacity: 0.4; }
  .eff { margin: 0; padding-left: 0; list-style: none; }
  .eff.wpn { padding-left: 0; } /* the element icon (or an empty .sic) lines the hits up with the stats */
  /* stat icon (or an empty slot of the same width, so the texts stay aligned) */
  .sic { display: inline-block; width: 16px; height: 16px; vertical-align: -3px; margin-right: 5px; }
  .cmp .r .sic { margin: 0 0 0 5px; }
  .eff.wpn { margin-bottom: 4px; padding-bottom: 4px; border-bottom: 1px dashed var(--line); }
  .ln.unk { color: var(--warn); }
  .ln.added { color: var(--acc); font-weight: 700; }
  .ln.above { color: var(--ok); font-weight: 700; }
  .ln.perfect { font-weight: 700; }
  .ln.low { color: #f08a3c; }
  .ln.neg { color: var(--bad); font-weight: 400; }
  .ln.legend { color: #e6b800; font-weight: 700; }
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
  .pin { padding: 1px 11px; font-size: 12.5px; font-weight: 400; border-radius: 9px; }
  .pin.on { background: color-mix(in srgb, var(--acc) 25%, transparent); border-color: var(--acc); color: var(--acc); }
  .fade { margin-left: 10px; padding: 1px 11px; font-size: 12.5px; font-weight: 400; border-radius: 9px; }
  .fade.on { background: color-mix(in srgb, var(--acc) 25%, transparent); border-color: var(--acc); color: var(--acc); }
  .none { color: var(--mut); font-size: 12px; }
  .flag { display: inline-block; margin-right: 6px; padding: 0 6px; border-radius: 8px; background: transparent; color: var(--mut); opacity: 0.55; font-size: 11px; font-weight: 600; white-space: nowrap; }
  .flag.on { background: color-mix(in srgb, var(--ok) 20%, transparent); color: var(--ok); opacity: 1; }
  .shield { display: inline-block; margin-right: 6px; padding: 0 6px; border-radius: 8px; background: color-mix(in srgb, var(--warn) 25%, transparent); color: var(--warn); font-size: 12px; }
</style>
