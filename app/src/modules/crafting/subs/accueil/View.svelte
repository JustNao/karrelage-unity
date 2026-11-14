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
   * another one; pointer events: the window's file drop takes HTML5 drag and drop) and narrow or wide (⇔). Order and widths
   * in localStorage `karrelage.crafting.home`.
   */
  interface Card { id: string; title: string; component: Component<any>; props?: Record<string, unknown>; wide?: boolean }
  const CARDS: Card[] = [
    { id: "life", title: "Vie au meilleur prix", component: Restore, props: { kind: "life" } },
    { id: "energy", title: "Énergie au meilleur prix", component: Restore, props: { kind: "energy" } },
    { id: "scrolls", title: "Parchemins contre avitons", component: Scrolls },
    { id: "ocre", title: "Dofus Ocre", component: Ocre, wide: true },
  ];
  const ui = persisted("karrelage.crafting.home", { order: [] as string[], wide: {} as Record<string, boolean> });
  /** saved order, then the cards added since */
  const order = $derived([...ui.order.filter((id) => CARDS.some((c) => c.id === id)), ...CARDS.map((c) => c.id).filter((id) => !ui.order.includes(id))]);
  const cards = $derived(order.map((id) => CARDS.find((c) => c.id === id)!));
  const isWide = (c: Card) => ui.wide[c.id] ?? !!c.wide;

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
    <div class="grid">
      {#each cards as c (c.id)}
        <section class="card" class:wide={isWide(c)} class:moving={moving === c.id} data-card={c.id}>
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
            <button class="ghost" onclick={() => (ui.wide[c.id] = !isWide(c))} title={isWide(c) ? "Carte étroite" : "Carte large"}>⇔</button>
          </header>
          <div class="body"><c.component {...c.props ?? {}} /></div>
        </section>
      {/each}
    </div>
  </SmoothScroll>
</div>

<style>
  .home { flex: 1; min-height: 0; display: flex; flex-direction: column; padding: 8px 10px 0; font-family: var(--font); }
  .home > :global(.wrap) { flex: 1; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(360px, 1fr)); gap: 12px; align-items: start; padding-bottom: 12px; }
  .card { background: var(--panel); border: 1px solid var(--line); border-radius: 8px; min-width: 0; }
  .card.wide { grid-column: span 2; }
  @media (max-width: 900px) { .card.wide { grid-column: auto; } }
  .card.moving { outline: 2px solid var(--acc); opacity: 0.85; }
  .hd { display: flex; align-items: center; gap: 8px; padding: 7px 10px; border-bottom: 1px solid var(--line); cursor: grab; user-select: none; touch-action: none; }
  .card.moving .hd { cursor: grabbing; }
  .hd b { flex: 1; }
  .grip { color: var(--mut); }
  .ghost { border: 0; background: none; color: var(--mut); padding: 0 4px; }
  .ghost:hover { color: var(--fg); }
  .body { padding: 8px 10px 10px; font-size: 13px; }
</style>
