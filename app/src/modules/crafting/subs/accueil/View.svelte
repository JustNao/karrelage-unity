<script lang="ts">
  import type { Component } from "svelte";
  import SmoothScroll from "../../../../lib/components/SmoothScroll.svelte";
  import { persisted } from "../../../../lib/persist.svelte";
  import { prices } from "../../../../lib/stores/prices.svelte";
  import Restore from "./Restore.svelte";
  import Scrolls from "./Scrolls.svelte";
  import Ocre from "./Ocre.svelte";

  /**
   * Crafting « Accueil »: cards of market landmarks, in the order the user puts them (drag a card by its title bar over
   * another one; pointer events: the window's file drop takes HTML5 drag and drop) and as wide as the user drags their right
   * edge, in columns of a 12-column grid (as Grafana's panels). Order and widths in localStorage `karrelage.crafting.home`.
   */
  interface Card { id: string; title: string; component: Component<any>; props?: Record<string, unknown>; span?: number }
  const COLUMNS = 12;
  const MIN_SPAN = 3;
  const SPAN = 4;
  const CARDS: Card[] = [
    { id: "life", title: "Vie au meilleur prix", component: Restore, props: { kind: "life" } },
    { id: "energy", title: "Énergie au meilleur prix", component: Restore, props: { kind: "energy" } },
    { id: "scrolls", title: "Parchemins contre avitons", component: Scrolls },
    { id: "ocre", title: "Dofus Ocre", component: Ocre, span: 8 },
  ];
  const ui = persisted("karrelage.crafting.home", { order: [] as string[], span: {} as Record<string, number>, wide: {} as Record<string, boolean> });
  /** saved order, then the cards added since */
  const order = $derived([...ui.order.filter((id) => CARDS.some((c) => c.id === id)), ...CARDS.map((c) => c.id).filter((id) => !ui.order.includes(id))]);
  const cards = $derived(order.map((id) => CARDS.find((c) => c.id === id)!));
  /** columns a card takes (the narrow / wide of an older save: 4 / 8) */
  const spanOf = (c: Card) => ui.span[c.id] ?? (ui.wide[c.id] === undefined ? (c.span ?? SPAN) : ui.wide[c.id] ? 8 : SPAN);

  // resizing a card: its right edge follows the pointer, snapped to the grid's columns
  let grid = $state<HTMLElement>();
  let resizing = $state<string | null>(null);
  function grabEdge(e: PointerEvent, id: string) {
    if (e.button !== 0) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    resizing = id;
  }
  function resize(e: PointerEvent) {
    if (!resizing || !grid) return;
    const card = grid.querySelector<HTMLElement>(`[data-card="${resizing}"]`);
    if (!card) return;
    const gap = parseFloat(getComputedStyle(grid).columnGap) || 0;
    const col = (grid.clientWidth + gap) / COLUMNS;
    const span = Math.round((e.clientX - card.getBoundingClientRect().left + gap) / col);
    ui.span[resizing] = Math.max(MIN_SPAN, Math.min(COLUMNS, span));
  }

  $effect(() => void prices.ensure());

  // moving a card: it takes the place of the card under the pointer as soon as it is over it
  let moving = $state<string | null>(null);
  function grab(e: PointerEvent, id: string) {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    moving = id;
  }
  function move(e: PointerEvent) {
    if (!moving) return;
    const over = (document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null)?.closest<HTMLElement>("[data-card]")?.dataset.card;
    if (!over || over === moving) return;
    const next = order.filter((id) => id !== moving);
    const at = next.indexOf(over) + (order.indexOf(over) > order.indexOf(moving) ? 1 : 0);
    next.splice(at, 0, moving);
    ui.order = next;
  }
</script>

<div class="home">
  <SmoothScroll>
    <div class="grid" bind:this={grid}>
      {#each cards as c (c.id)}
        <section class="card" class:moving={moving === c.id} class:resizing={resizing === c.id} data-card={c.id} style:--span={spanOf(c)}>
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <header
            class="hd"
            onpointerdown={(e) => grab(e, c.id)}
            onpointermove={move}
            onpointerup={() => (moving = null)}
            onpointercancel={() => (moving = null)}
            title="Glisser pour déplacer la carte"
          >
            <span class="grip">⠿</span>
            <b>{c.title}</b>
          </header>
          <div class="body"><c.component {...c.props ?? {}} /></div>
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="edge"
            onpointerdown={(e) => grabEdge(e, c.id)}
            onpointermove={resize}
            onpointerup={() => (resizing = null)}
            onpointercancel={() => (resizing = null)}
            ondblclick={() => delete ui.span[c.id]}
            title="Glisser pour élargir ou rétrécir la carte · double-clic : largeur par défaut"
          ></div>
        </section>
      {/each}
    </div>
  </SmoothScroll>
</div>

<style>
  .home { flex: 1; min-height: 0; display: flex; flex-direction: column; padding: 8px 10px 0; font-family: var(--font); }
  .home > :global(.wrap) { flex: 1; }
  .grid { display: grid; grid-template-columns: repeat(12, minmax(0, 1fr)); gap: 12px; align-items: start; padding-bottom: 12px; }
  .card { position: relative; grid-column: span var(--span); background: var(--panel); border: 1px solid var(--line); border-radius: 8px; min-width: 0; }
  @media (max-width: 900px) { .card { grid-column: 1 / -1; } }
  .card.resizing { outline: 2px dashed var(--acc); }
  .edge { position: absolute; top: 0; right: -7px; width: 12px; height: 100%; cursor: ew-resize; touch-action: none; z-index: 1; }
  .edge::after { content: ""; position: absolute; top: 50%; right: 5px; width: 3px; height: 28px; margin-top: -14px; border-radius: 2px; background: var(--line); opacity: 0; transition: opacity 0.15s; }
  .card:hover .edge::after, .card.resizing .edge::after { opacity: 1; }
  .card.moving { outline: 2px solid var(--acc); opacity: 0.85; }
  .hd { display: flex; align-items: center; gap: 8px; padding: 7px 10px; border-bottom: 1px solid var(--line); cursor: grab; user-select: none; touch-action: none; }
  .card.moving .hd { cursor: grabbing; }
  .hd b { flex: 1; }
  .grip { color: var(--mut); }
  .body { padding: 8px 10px 10px; font-size: 13px; }
</style>
