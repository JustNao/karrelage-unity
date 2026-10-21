<script lang="ts">
  import type { Snippet } from "svelte";
  import { cubicOut } from "svelte/easing";
  import { kamasFull } from "../fmt";
  import { HIDDEN_EFFECTS, isNegative, perfection } from "../effects";
  import type { Opportunity } from "../transcend";
  import { unitPrice } from "../price";
  import { useSort } from "../sort.svelte";
  import { iconOfElement, iconOfStat } from "../stores/statIcons.svelte";
  import ItemCard from "./ItemCard.svelte";
  import SortTable from "./SortTable.svelte";

  /**
   * Equipment lots as a condensed table (the HDV "Tableau" view): good deal, price, perfect damage / resistances /
   * characteristics, transcendance, over-max and EXO effects; good deals get a green line. Clicking a row unfolds the
   * lot's card under it, clicking the card folds it again. Clicking an over / EXO chip highlights the lots with that stat
   * (like "Mettre en avant" on the cards) without unfolding; the pin button sits left of the unfolded card.
   * Also used for lots of different items (Stuff tab): a leading `lead` column (item), no deal / transcendance columns.
   */
  let {
    lots,
    good = new Set(),
    opps = new Map(),
    deals = true,
    transcend = true,
    lead,
    picked = null,
    open,
    faded = () => false,
    ontoggle,
    fade = null,
    onfade,
    pinned = () => false,
    onpin,
    versus = () => null,
    stickTop = 0,
  }: {
    /** `meta`: whatever the `lead` column shows (e.g. the slot and item of a Stuff lot). */
    lots: { row: any; key: string; meta?: any }[];
    /** Keys of the lots well above the price trend (green line). */
    good?: Set<string>;
    /** Transcendance candidates of the listing (unlocked lots a rune makes profitable), by row. */
    opps?: Map<any, Opportunity>;
    /** Show the "Bonne affaire" / "Transcendance" columns (only meaningful within one item's listing). */
    deals?: boolean;
    transcend?: boolean;
    /** A first column (label, cell, sort key), e.g. the item of each lot. */
    lead?: { label: string; cell: Snippet<[any]>; sortKey?: (meta: any) => string };
    picked?: string | number | null;
    open: (key: string) => boolean;
    faded?: (r: any) => boolean;
    ontoggle: (key: string) => void;
    fade?: number | null;
    onfade?: (action: number) => void;
    pinned?: (r: any) => boolean;
    onpin?: (r: any) => void;
    versus?: (r: any) => { pin: any; name: string } | null;
    /** Height of what sticks above the table (the graph): the header sticks right under it. */
    stickTop?: number;
  } = $props();

  /** The unfolded card starts where the price of its row starts (re-measured when the table changes size). */
  function alignToPrice(node: HTMLElement) {
    const tr = node.closest("tr");
    const table = node.closest("table");
    const set = () => {
      const price = tr?.previousElementSibling?.querySelector("td.pc b");
      if (price && table) node.style.paddingLeft = `${price.getBoundingClientRect().left - table.getBoundingClientRect().left}px`;
    };
    set();
    const ro = new ResizeObserver(set);
    if (table) ro.observe(table);
    return { destroy: () => ro.disconnect() };
  }

  /** What an over-max line has above its default max ("+12 Vitalité"), or the whole line for an EXO ("+6 Dommage Critique"). */
  const extra = (e: any): string => {
    const t = String(e.text ?? "");
    if (!/^\d+/.test(t) || isNegative(e)) return t;
    return e.status === "above" && e.range && e.hi != null ? t.replace(/^\d+/, `+${e.hi - e.range[1]}`) : "+" + t;
  };

  /**
   * Unfold / fold of a lot's card: the grid row goes from 0fr to 1fr, so the height always follows the card's real layout
   * (a measured height, like `slide`, goes wrong when the table reflows during the animation).
   */
  const unfold = (_: Element, { duration = 240 } = {}) => ({ duration, easing: cubicOut, css: (t: number) => `grid-template-rows: ${t}fr; opacity: ${Math.min(1, t * 1.5)}` });

  const rows = $derived(
    lots.map(({ row, key, meta }) => {
      const p = perfection(row.effects);
      const shown = row.effects.filter((e: any) => !HIDDEN_EFFECTS.has(e.action));
      // EXO (blue) first, then over-max (green)
      const extras = [...shown.filter((e: any) => e.status === "added"), ...shown.filter((e: any) => e.status === "above")].map((e: any) => ({
        text: extra(e),
        action: e.action,
        icon: e.group === "weapon" ? iconOfElement(e.text) : iconOfStat(e.stat),
        over: e.status === "above",
      }));
      const missing = shown.filter((e: any) => e.status === "removed").length;
      const o = opps.get(row);
      return {
        row, key, meta, item: lead?.sortKey?.(meta) ?? "", price: unitPrice(row), damage: p.damage, resist: p.resist, main: p.main, extras, missing,
        locked: !!row.locked, opp: o?.best ?? null, good: good.has(key),
        // sort keys
        extra: extras.length, trans: o?.best ? o.best.profit : row.locked ? -1e12 : -2e12,
      };
    }),
  );

  type SortKey = "item" | "price" | "damage" | "resist" | "main" | "extra" | "trans" | "good";
  const sort = useSort<SortKey>(null, false, (k) => k !== "price" && k !== "item"); // no column = the order of the list
  const allColumns: { key: SortKey; label: string; num?: boolean; title?: string }[] = $derived([
    ...(lead ? [{ key: "item" as const, label: lead.label }] : []),
    { key: "good", label: "Bonne affaire", title: "Lot bien au-dessus de la tendance prix / densité" },
    { key: "price", label: "Prix", num: true },
    { key: "damage", label: "Dommages parfaits", title: "Tous les effets de dommages de base à leur maximum" },
    { key: "resist", label: "Résistances parfaites", title: "Résistances de base à leur maximum (seulement les % de résistance quand l'objet en a)" },
    { key: "main", label: "Caractéristiques parfaites", title: "Caractéristiques principales au maximum" },
    { key: "trans", label: "Transcendance", title: "🛡 lot transcendé · +X : candidat rentable avec une rune (Do Cri +8, % Do, % Ré, Tacle)" },
    { key: "extra", label: "Over / EXO", title: "Effets ajoutés (bleu) et ce qui dépasse le max de base (vert)" },
  ]);
  const columns = $derived(allColumns.filter((c) => (deals || c.key !== "good") && (transcend || c.key !== "trans")));
  const sorted = $derived(sort.apply(rows, (r, k) => (k === "damage" || k === "resist" || k === "main" || k === "good" ? +r[k] : r[k])));
</script>

<div class="ltable" style:--stick="{stickTop}px">
  <SortTable
    {columns}
    rows={sorted}
    {sort}
    key={(r) => r.key}
    rowProps={(r) => ({ id: `lot-${r.key}`, class: [picked === r.key && "picked", faded(r.row) && "faded", r.good && "hot", open(r.key) && "open"].filter(Boolean).join(" "), onclick: () => ontoggle(r.key) })}
  >
    {#snippet row(r)}
      {#if lead}<td class="lead">{@render lead.cell(r.meta)}</td>{/if}
      {#if deals}<td class="c">{#if r.good}<span class="good" title="Bien au-dessus de la tendance prix / densité">●</span>{/if}</td>{/if}
      <td class="n pc"><b>{Number.isFinite(r.price) ? kamasFull(r.price) : "—"}</b></td>
      <td class="c">{#if r.damage}<span class="star">★</span>{/if}</td>
      <td class="c">{#if r.resist}<span class="star">★</span>{/if}</td>
      <td class="c">{#if r.main}<span class="star">★</span>{/if}</td>
      {#if transcend}
        <td class="tr">
          {#if r.locked}<span title="Transcendé">🛡</span>
          {:else if r.opp}<span class="good" title="Ajouter {r.opp.target.rune?.name ?? 'la rune'} puis revendre (après taxe)">+{kamasFull(r.opp.profit)}</span> <span class="mut">{(r.opp.target.rune?.name ?? "").replace(/^Rune /, "")}</span>{/if}
        </td>
      {/if}
      <td class="fx">
        {#each r.extras as x}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <span
            class="chip"
            class:over={x.over}
            class:on={fade === x.action}
            class:act={!!onfade}
            title={(x.over ? "Au-dessus du max de base" : "EXO : pas un effet de base") + (!onfade ? "" : fade === x.action ? " · cliquer pour tout afficher" : " · cliquer pour mettre en avant les lots qui ont cet effet")}
            onclick={(ev) => onfade && (ev.stopPropagation(), onfade(x.action))}>{#if x.icon}<img src={x.icon} alt="" />{/if}{x.text}</span>
        {/each}
        {#if r.missing}<span class="chip miss" title="Effet(s) de base manquant(s)">−{r.missing} effet{r.missing > 1 ? "s" : ""}</span>{/if}
      </td>
    {/snippet}
    {#snippet sub(r)}
      {#if open(r.key)}
        <tr class="card-row" onclick={() => ontoggle(r.key)}><td colspan={columns.length}>
          <div class="unfold" transition:unfold>
            <div class="clip"><div class="pad" use:alignToPrice>
              {#if onpin}
                <button class="pin" class:on={pinned(r.row)} title={pinned(r.row) ? "Désépingler" : "Épingler : comparer chaque lot consulté avec celui-ci"} onclick={(ev) => (ev.stopPropagation(), onpin(r.row))}>📌</button>
              {/if}
              <ItemCard row={r.row} inTable versus={versus(r.row)} />
            </div></div>
          </div>
        </td></tr>
      {/if}
    {/snippet}
    {#snippet empty()}Aucun lot ne correspond au filtre{/snippet}
  </SortTable>
</div>

<style>
  .ltable { margin-top: 6px; container-type: inline-size; }
  .ltable :global(tbody tr) { cursor: pointer; transition: opacity 0.25s; }
  .ltable :global(tbody tr:not(.card-row):hover) { background: var(--panel); }
  .ltable :global(tr.picked) { outline: 1px solid var(--acc); outline-offset: -1px; }
  .ltable :global(tr.faded) { opacity: 0.2; }
  .ltable :global(tr.hot) { background: color-mix(in srgb, var(--ok) 14%, transparent); }
  .ltable :global(tr.hot td) { background: none; }
  /* the open row is marked by a background, not bold text: a change of width would reflow the table mid-animation */
  .ltable :global(tr.open:not(.hot) td) { background: var(--panel); }
  /* the sticky header sits flush with the top of the scroll area (over the scroller's padding), under the sticky graph */
  .ltable :global(th) { top: calc(var(--stick, 0px) - var(--pad, 12px)); z-index: 2; white-space: nowrap; padding: 2px 5px; }
  /* every column but the last (Over / EXO) as narrow as its content, on one line */
  .ltable :global(th:not(:last-child)), .ltable :global(td:not(:last-child)) { width: 1%; }
  .ltable :global(td) { white-space: nowrap; padding-left: 5px; padding-right: 5px; }
  /* not enough room for one-line headers (they need ~1000px): two lines, between words only (never inside one) */
  @container (max-width: 1000px) {
    .ltable :global(th) { white-space: normal; vertical-align: bottom; line-height: 1.15; }
  }
  .ltable :global(tr.card-row td) { padding: 0; border-top: 0; white-space: normal; }
  .unfold { display: grid; grid-template-rows: 1fr; }
  .clip { min-height: 0; overflow: hidden; }
  .pad { position: relative; padding: 14px 6px 16px; } /* padding-left: alignToPrice */
  /* in the empty space left of the card (under the "Bonne affaire" column) */
  .pin { position: absolute; left: 6px; top: 12px; padding: 1px 5px; font-size: 12px; line-height: 1.3; opacity: 0.55; filter: grayscale(1); }
  .pin:hover { opacity: 1; }
  .pin.on { opacity: 1; filter: none; border-color: var(--acc); }
  .ltable :global(tr.card-row .card) { margin-bottom: 0; border: 0; padding: 0; background: none; }
  .c { text-align: center; }
  .lead { white-space: nowrap; }
  /* profit and rune name each on one line, the rune going under the profit when the table is short of room */
  .ltable td.tr { white-space: normal; }
  .tr span { white-space: nowrap; }
  .star { color: var(--acc); }
  .ltable td.fx { white-space: normal; line-height: 1.8; }
  .chip { display: inline-flex; align-items: center; gap: 3px; margin: 0 4px 0 0; padding: 0 6px 0 4px; border-radius: 8px; font-size: 11px; white-space: nowrap; color: var(--acc); border: 1px solid color-mix(in srgb, var(--acc) 50%, var(--line)); }
  .chip img { width: 14px; height: 14px; }
  .chip.over { color: var(--ok); border-color: color-mix(in srgb, var(--ok) 50%, var(--line)); }
  .chip.act { cursor: pointer; }
  .chip.act:hover { background: color-mix(in srgb, currentColor 12%, transparent); }
  .chip.on { background: color-mix(in srgb, currentColor 22%, transparent); border-color: currentColor; }
  .chip.miss { color: var(--warn, orange); border-color: var(--line); padding-left: 6px; }
  .good { color: var(--ok); }
</style>
