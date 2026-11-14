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
   * another one; pointer events: the window's file drop takes HTML5 drag and drop) and sized as Grafana's panels: their right
   * edge snaps to the columns of a 12-column grid, their bottom edge to rows of ROW px (the corner does both). A card shows
   * the rows its height holds, with no scroll bar (lib/actions/fitRows.ts in the cards). Order and sizes in localStorage
   * `karrelage.crafting.home`.
   */
  interface Card { id: string; title: string; component: Component<any>; props?: Record<string, unknown>; span?: number; rows: number }
  const COLUMNS = 12;
  const MIN_SPAN = 3;
  const SPAN = 4;
  const ROW = 30;
  const MIN_ROWS = 6;
  const MAX_ROWS = 40;
  const CARDS: Card[] = [
    { id: "life", title: "Vie au meilleur prix", component: Restore, props: { kind: "life" }, rows: 12 },
    { id: "energy", title: "Énergie au meilleur prix", component: Restore, props: { kind: "energy" }, rows: 12 },
    { id: "scrolls", title: "Parchemins contre avitons", component: Scrolls, rows: 9 },
    { id: "ocre", title: "Dofus Ocre", component: Ocre, span: 8, rows: 11 },
  ];
  const ui = persisted("karrelage.crafting.home", {
    order: [] as string[],
    span: {} as Record<string, number>,
    rows: {} as Record<string, number>,
    /** narrow / wide of an older save */
    wide: {} as Record<string, boolean>,
  });
  /** saved order, then the cards added since */
  const order = $derived([...ui.order.filter((id) => CARDS.some((c) => c.id === id)), ...CARDS.map((c) => c.id).filter((id) => !ui.order.includes(id))]);
  const cards = $derived(order.map((id) => CARDS.find((c) => c.id === id)!));
  /** columns a card takes (the narrow / wide of an older save: 4 / 8) */
  const spanOf = (c: Card) => ui.span[c.id] ?? (ui.wide[c.id] === undefined ? (c.span ?? SPAN) : ui.wide[c.id] ? 8 : SPAN);
  const rowsOf = (c: Card) => ui.rows[c.id] ?? c.rows;

  // resizing a card: the edge held (right: width, bottom: height, corner: both) follows the pointer, snapped to the grid
  type Axes = "x" | "y" | "xy";
  let grid = $state<HTMLElement>();
  let resizing = $state<{ id: string; axes: Axes } | null>(null);
  function grabEdge(e: PointerEvent, id: string, axes: Axes) {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    resizing = { id, axes };
    follow(resize, () => (resizing = null));
  }
  function resize(e: PointerEvent) {
    if (!resizing || !grid) return;
    const { id, axes } = resizing;
    const box = grid.querySelector<HTMLElement>(`[data-card="${id}"]`)?.getBoundingClientRect();
    if (!box) return;
    if (axes !== "y") {
      const gap = parseFloat(getComputedStyle(grid).columnGap) || 0;
      const col = (grid.clientWidth + gap) / COLUMNS;
      ui.span[id] = Math.max(MIN_SPAN, Math.min(COLUMNS, Math.round((e.clientX - box.left + gap) / col)));
    }
    if (axes !== "x") ui.rows[id] = Math.max(MIN_ROWS, Math.min(MAX_ROWS, Math.round((e.clientY - box.top) / ROW)));
  }
  /** double click on an edge: that size back to the default */
  function reset(id: string, axes: Axes) {
    if (axes !== "y") delete ui.span[id];
    if (axes !== "x") delete ui.rows[id];
  }
  const EDGES: { axes: Axes; cls: string; title: string }[] = [
    { axes: "x", cls: "ex", title: "Glisser pour élargir ou rétrécir la carte · double-clic : largeur par défaut" },
    { axes: "y", cls: "ey", title: "Glisser pour allonger ou raccourcir la carte · double-clic : hauteur par défaut" },
    { axes: "xy", cls: "exy", title: "Glisser pour redimensionner la carte · double-clic : taille par défaut" },
  ];

  $effect(() => void prices.ensure());

  /** a drag followed on the whole window until the button is released (pointer capture would not do: moving a card moves
   *  its element in the page, which drops the capture and the release with it) */
  function follow(onmove: (e: PointerEvent) => void, onend: () => void) {
    const end = (e: Event) => {
      if (e.type === "pointerup" && (e as PointerEvent).button !== 0) return;
      window.removeEventListener("pointermove", onmove);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
      window.removeEventListener("blur", end);
      onend();
    };
    window.addEventListener("pointermove", onmove);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
    window.addEventListener("blur", end);
  }

  // moving a card: it takes the place of the card under the pointer as soon as it is over it
  let moving = $state<string | null>(null);
  function grab(e: PointerEvent, id: string) {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    e.preventDefault();
    moving = id;
    follow(move, () => (moving = null));
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
        <section
          class="card"
          class:moving={moving === c.id}
          class:resizing={resizing?.id === c.id}
          data-card={c.id}
          style:--span={spanOf(c)}
          style:height="{rowsOf(c) * ROW}px"
        >
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <header
            class="hd"
            onpointerdown={(e) => grab(e, c.id)}
            title="Glisser pour déplacer la carte"
          >
            <span class="grip">⠿</span>
            <b>{c.title}</b>
          </header>
          <div class="body"><c.component {...c.props ?? {}} /></div>
          {#each EDGES as edge (edge.axes)}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="edge {edge.cls}"
              onpointerdown={(e) => grabEdge(e, c.id, edge.axes)}
              ondblclick={() => reset(c.id, edge.axes)}
              title={edge.title}
            ></div>
          {/each}
        </section>
      {/each}
    </div>
  </SmoothScroll>
</div>

<style>
  .home { flex: 1; min-height: 0; display: flex; flex-direction: column; padding: 8px 10px 0; font-family: var(--font); }
  .home > :global(.wrap) { flex: 1; }
  .grid { display: grid; grid-template-columns: repeat(12, minmax(0, 1fr)); gap: 12px; align-items: start; padding-bottom: 12px; }
  .card { position: relative; grid-column: span var(--span); display: flex; flex-direction: column; box-sizing: border-box; background: var(--panel); border: 1px solid var(--line); border-radius: 8px; min-width: 0; }
  @media (max-width: 900px) { .card { grid-column: 1 / -1; } }
  .card.resizing { outline: 2px dashed var(--acc); }
  .card.moving { outline: 2px solid var(--acc); opacity: 0.85; }
  .edge { position: absolute; touch-action: none; z-index: 1; }
  .ex { top: 0; right: -7px; width: 12px; height: 100%; cursor: ew-resize; }
  .ey { left: 0; bottom: -7px; width: 100%; height: 12px; cursor: ns-resize; }
  .exy { right: -7px; bottom: -7px; width: 16px; height: 16px; cursor: nwse-resize; z-index: 2; }
  .edge::after { content: ""; position: absolute; border-radius: 2px; background: var(--line); opacity: 0; transition: opacity 0.15s; }
  .ex::after { top: 50%; right: 5px; width: 3px; height: 28px; margin-top: -14px; }
  .ey::after { left: 50%; bottom: 5px; width: 28px; height: 3px; margin-left: -14px; }
  .exy::after { right: 6px; bottom: 6px; width: 7px; height: 7px; border-radius: 0 0 3px 0; background: none; border-right: 2px solid var(--line); border-bottom: 2px solid var(--line); }
  .card:hover .edge::after, .card.resizing .edge::after { opacity: 1; }
  .hd { flex: none; display: flex; align-items: center; gap: 8px; padding: 7px 10px; border-bottom: 1px solid var(--line); cursor: grab; user-select: none; touch-action: none; }
  .card.moving .hd { cursor: grabbing; }
  .hd b { flex: 1; }
  .grip { color: var(--mut); }
  /* the card's component fills it (its lists show the rows that fit: fitRows) */
  .body { flex: 1; min-height: 0; overflow: hidden; display: flex; flex-direction: column; padding: 8px 10px 10px; font-size: 13px; }
</style>
