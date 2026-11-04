<script lang="ts">
  import { call } from "../lib/api";
  import { settings } from "../lib/stores/settings.svelte";
  import { screenCentre, type Calibration, type FightMapState } from "../lib/fightMap";
  import { PORTAL_BONUS, playerPortals, portalAuthor, type PortalSims } from "../lib/classHelp";

  /**
   * Portal simulation in the in-game view (FightMapOverlay.svelte, opened by the game's Portail key; the mouse let through): the
   * Portail spell simulated on the cell under the mouse. On a free cell, a new portal there; on one of the portals, the spell
   * goes through the network and the new portal comes out past the exit. Only what changes is drawn, the game shows the portals
   * and their numbers: the X on the portal that goes when there were four, the new network's way from the new portal and its
   * damage bonus (PORTAL_BONUS % per cell).
   * The mouse on one of the player's open portals when the view opens (`polled`: its first cursor read): that portal is the
   * `start`, the way shown is from it through each simulated network. The simulations of every cell come at once (asked again
   * when portals or fighters change): hovering is a lookup.
   */
  let { cal, live, mine, dpr, pointed, polled }: {
    cal: Calibration;
    live: FightMapState;
    mine: Set<number>;
    dpr: number;
    pointed: number | null;
    polled: boolean;
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

  let all = $state.raw<PortalSims | null>(null);
  let asked = 0;
  $effect(() => {
    const [who, from] = [author, start];
    void live.portals;
    void live.fighters;
    if (who === null || from === undefined) return void (all = null);
    const n = ++asked;
    call<PortalSims>("fightmap", "portals", { author: who, all: true, ...(from !== null && { start: from }) }, true)
      .then((r) => n === asked && (all = r))
      .catch(() => {});
  });

  // the cell under the mouse: its simulation (none for an obstacle or a fighter's cell: the network as it is)
  const sim = $derived(all && ((pointed !== null && all.sims[pointed]) || all.base));
  const exit = $derived(sim && sim.path.length > 1 ? sim.path[sim.path.length - 1] : null);
  const points = $derived((sim?.path ?? []).map((c) => screenCentre(cal, c, dpr).join(",")).join(" "));
  /** the start portal's number in the simulated network, null when it is the one that goes */
  const startNumber = $derived.by(() => {
    const number = portals.find((p) => p.cell === start)?.number;
    if (number === undefined || sim?.dropped === start) return null;
    return sim?.dropped != null ? number - 1 : number;
  });
  /** size of a cell on screen (CSS px) */
  const cell = $derived([cal.sx / dpr, cal.sy / dpr]);
  const key = $derived(settings.classHelp.eliotrope.portalKey);
</script>

<svg width="100%" height="100%">
  {#if sim}
    {#if sim.dropped !== null}
      {@const [x, y] = screenCentre(cal, sim.dropped, dpr)}
      <path d="M{x - cell[0] / 5} {y - cell[1] / 10}L{x + cell[0] / 5} {y + cell[1] / 10}M{x + cell[0] / 5} {y - cell[1] / 10}L{x - cell[0] / 5} {y + cell[1] / 10}" class="cross" />
    {/if}
    {#if exit !== null}
      {@const [x, y] = screenCentre(cal, exit, dpr)}
      {@const size = Math.max(10, cell[1] / 7)}
      <polyline {points} class="way under" />
      <polyline {points} class="way" />
      <text x={x} y={y + cell[1] / 2.6} font-size={size} class="bonus under">+{sim.distance * PORTAL_BONUS} %</text>
      <text x={x} y={y + cell[1] / 2.6} font-size={size} class="bonus">+{sim.distance * PORTAL_BONUS} %</text>
    {/if}
  {/if}
</svg>

<div class="pill">
  Portail simulé sous la souris
  {#if start != null}· départ : {startNumber === null ? "votre portail disparaît" : `portail n° ${startNumber}`}{/if}
  {#if sim && sim.path.length > 1}· sortie à {sim.distance} cases : <b>+{sim.distance * PORTAL_BONUS} % de dommages</b>{/if}
  {#if sim?.via}· lancé dans un portail : {sim.added === null ? "la case de sortie est prise ou hors de la carte" : "le nouveau portail sort après le dernier de la redirection"}{/if}
  {#if sim?.dropped != null}· <span class="chip" style:color="#ff4d4d">✕ le n° 1 disparaît</span>
  {:else if !sim?.added}· survolez une case libre{/if}
  · {key.vk ? key.label : "le raccourci"}, un clic ou Échap pour fermer
</div>

<style>
  .cross { stroke: #ff4d4d; stroke-width: 4; stroke-linecap: round; paint-order: stroke; }
  .way { fill: none; stroke: #ffd23c; stroke-opacity: 0.75; stroke-width: 3; stroke-linejoin: round; }
  .way.under { stroke: rgba(0, 0, 0, 0.35); stroke-opacity: 1; stroke-width: 5; }
  .bonus { fill: #fff; font-weight: 700; text-anchor: middle; dominant-baseline: central; }
  .bonus.under { stroke: rgba(0, 0, 0, 0.75); stroke-width: 6; }
</style>
