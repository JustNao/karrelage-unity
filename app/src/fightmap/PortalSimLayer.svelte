<script lang="ts">
  import { call } from "../lib/api";
  import { occupiedCells, screenCentre, type Calibration, type FightMapState } from "../lib/fightMap";
  import { playerPortals, portalAuthor, type PortalSims } from "../lib/classHelp";
  import PortalWay from "./PortalWay.svelte";

  /**
   * Portal simulation in the in-game view (FightMapOverlay.svelte, opened by the game's Portail key; the mouse let through): the
   * Portail spell simulated on the cell under the mouse. On a free cell, a new portal there; on one of the portals, the spell
   * goes through the network and the new portal comes out past the exit. Only what changes is drawn, the game shows the portals
   * and their numbers: the X on the portal that goes when there were four, the new network's way from the new portal and its
   * damage bonus (PORTAL_BONUS % per cell), in yellow the cell a spell sent through that way ends up on.
   * `exil`: the Exil spell instead (opened by its own key), the same on a fighter's cell: its portal goes under the fighter.
   * The mouse on one of the player's open portals when the view opens (`polled`: its first cursor read): that portal is the
   * `start`, the way shown is from it through each simulated network. The simulations of every cell come at once (asked again
   * when what they depend on changes): hovering is a lookup.
   */
  let { cal, live, mine, dpr, pointed, polled, exil = false }: {
    cal: Calibration;
    live: FightMapState;
    mine: Set<number>;
    dpr: number;
    pointed: number | null;
    polled: boolean;
    exil?: boolean;
  } = $props();

  const author = $derived(portalAuthor(live, mine));
  const portals = $derived(playerPortals(live, mine));

  /** undefined: not chosen yet; null: none */
  let start = $state<number | null | undefined>(undefined);
  $effect(() => {
    if (!polled || start !== undefined) return;
    const at = pointed;
    start = portals.some((p) => p.cell === at && p.open) ? at : null;
  });

  /** what the simulations depend on */
  const inputs = $derived(JSON.stringify([live.map, live.portals, occupiedCells(live), live.fighters.find((f) => f.id === author)?.cell]));
  let all = $state.raw<PortalSims | null>(null);
  let asked = 0;
  $effect(() => {
    void inputs;
    const [who, from] = [author, start];
    if (who === null || from === undefined) return void (all = null);
    const n = ++asked;
    call<PortalSims>("fightmap", "portals", { author: who, exil, ...(from !== null && { start: from }) }, true)
      .then((r) => n === asked && (all = r))
      .catch(() => {});
  });

  // the cell under the mouse: its simulation; anywhere the spell puts no portal (off the grid, a wall, the game's bars): the
  // network as it is, so the key shows something wherever the mouse is
  const aimed = $derived(all && pointed !== null ? all.sims[pointed] : null);
  const sim = $derived(aimed?.added != null ? aimed : all?.base);
  /** size of a cell on screen (CSS px) */
  const cell = $derived([cal.sx / dpr, cal.sy / dpr]);
</script>

<svg width="100%" height="100%">
  {#if sim}
    {#if sim.dropped !== null}
      {@const [x, y] = screenCentre(cal, sim.dropped, dpr)}
      <path d="M{x - cell[0] / 5} {y - cell[1] / 10}L{x + cell[0] / 5} {y + cell[1] / 10}M{x + cell[0] / 5} {y - cell[1] / 10}L{x - cell[0] / 5} {y + cell[1] / 10}" class="cross" />
    {/if}
    <PortalWay {cal} {dpr} {sim} />
  {/if}
</svg>

<style>
  .cross { stroke: #ff4d4d; stroke-width: 4; stroke-linecap: round; paint-order: stroke; }
</style>
