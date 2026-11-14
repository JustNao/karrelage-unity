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
  const saved = $derived([...ui.order.filter((id) => CARDS.some((c) => c.id === id)), ...CARDS.map((c) => c.id).filter((id) => !ui.order.includes(id))]);
  /** while a card is moved: where the cards would be if it were let go now (saved on release) */
  let preview = $state<string[] | null>(null);
  const order = $derived(preview ?? saved);
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

  // moving a card: a faded copy follows the pointer, its place in the grid (a dashed outline) goes before or after the card
  // under the pointer (its left / right half); the order is saved on release
  let moving = $state<string | null>(null);
  let ghost: HTMLElement | null = null;
  let held = { x: 0, y: 0 };
  function grab(e: PointerEvent, id: string) {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    const card = (e.currentTarget as HTMLElement).closest<HTMLElement>("[data-card]");
    if (!card) return;
    e.preventDefault();
    const box = card.getBoundingClientRect();
    held = { x: e.clientX - box.left, y: e.clientY - box.top };
    ghost = card.cloneNode(true) as HTMLElement;
    delete ghost.dataset.card;
    ghost.classList.add("ghost");
    Object.assign(ghost.style, { position: "fixed", left: `${box.left}px`, top: `${box.top}px`, width: `${box.width}px`, height: `${box.height}px`, margin: "0" });
    document.body.appendChild(ghost);
    document.body.style.cursor = "grabbing";
    preview = [...saved];
    moving = id;
    follow(move, drop);
  }
  function move(e: PointerEvent) {
    if (!moving || !preview) return;
    if (ghost) Object.assign(ghost.style, { left: `${e.clientX - held.x}px`, top: `${e.clientY - held.y}px` });
    const el = (document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null)?.closest<HTMLElement>("[data-card]");
    const over = el?.dataset.card;
    if (!el || !over || over === moving) return;
    const box = el.getBoundingClientRect();
    const next = preview.filter((id) => id !== moving);
    next.splice(next.indexOf(over) + (e.clientX < box.left + box.width / 2 ? 0 : 1), 0, moving);
    if (next.join() !== preview.join()) preview = next;
  }
  function drop() {
    if (preview) ui.order = preview;
    preview = null;
    moving = null;
    ghost?.remove();
    ghost = null;
    document.body.style.cursor = "";
  }
  $effect(() => () => ghost?.remove());
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
  /* the moved card's place if let go now: a dashed outline (its copy follows the pointer: .ghost, in the body) */
  .card.moving { background: color-mix(in srgb, var(--acc) 6%, transparent); border: 2px dashed var(--acc); }
  .card.moving > * { visibility: hidden; }
  :global(.card.ghost) { opacity: 0.6; pointer-events: none; z-index: 1000; box-shadow: 0 8px 28px rgba(0, 0, 0, 0.45); }
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
  .hd b { flex: 1; }
  .grip { color: var(--mut); }
  /* the card's component fills it (its lists show the rows that fit: fitRows) */
  .body { flex: 1; min-height: 0; overflow: hidden; display: flex; flex-direction: column; padding: 8px 10px 10px; font-size: 13px; }
</style>
