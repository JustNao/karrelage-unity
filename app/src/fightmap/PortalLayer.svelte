<script lang="ts">
  import { call } from "../lib/api";
  import { screenCentre, screenDiamond, type Calibration, type FightMapState } from "../lib/fightMap";
  import { PORTAL_BONUS, portalAuthor, portalColour, type PortalNet } from "../lib/classHelp";

  /**
   * Eliotrope portals in the in-game view (inside ClassLayer.svelte): the player's portals numbered by age, the two that go
   * first in colour once there are four. `simulate` (view `portals`): a portal put down on the cell under the mouse (the
   * oldest one struck out when there were four) and the way from it through the network to its exit, with its length and
   * damage bonus (PORTAL_BONUS % per cell); on one of the portals, the spell goes through the network and the new portal
   * comes out past the exit (`cast`: the spell's way, thin); the main line is always the new network's way from the
   * new portal. The simulations of every
   * cell come at once (asked again when portals or fighters change): hovering is a lookup.
   * The mouse on one of the player's open portals when the simulation opens (`polled`: its first cursor read): that portal is
   * the `start`, the way shown is from it through each simulated network (cleared with the view).
   */
  let { cal, live, mine, dpr, pointed, polled, simulate, net = $bindable(null), start = $bindable(undefined) }: {
    cal: Calibration;
    live: FightMapState;
    mine: Set<number>;
    dpr: number;
    pointed: number | null;
    polled: boolean;
    simulate: boolean;
    net?: PortalNet | null;
    /** undefined: not chosen yet; null: none */
    start?: number | null;
  } = $props();

  const author = $derived(portalAuthor(live, mine));

  // the start: chosen once, where the mouse was when the simulation opened
  $effect(() => {
    if (!simulate) return void (start = undefined);
    if (!polled || start !== undefined) return;
    const at = pointed;
    start = at !== null && (live.portals ?? []).some((p) => p.author === author && p.cell === at && p.open) ? at : null;
  });

  let all = $state.raw<{ base: PortalNet; sims: Record<string, PortalNet> } | null>(null);
  let asked = 0;
  $effect(() => {
    const [who, from] = [author, start];
    void live.portals;
    void live.fighters;
    if (who === null || (simulate && from === undefined)) return void (all = null);
    const n = ++asked;
    call<{ base: PortalNet; sims: Record<string, PortalNet> }>("fightmap", "portals", { author: who, all: simulate, ...(from != null && { start: from }) }, true)
      .then((r) => n === asked && (all = simulate ? r : { base: r as unknown as PortalNet, sims: {} }))
      .catch(() => {});
  });
  // the cell under the mouse: its simulation (none for an obstacle or a fighter's cell: the network as it is)
  $effect(() => {
    net = all && ((simulate && pointed !== null && all.sims[pointed]) || all.base);
  });

  const full = $derived((net?.portals.length ?? 0) >= 4);
  const line = (cells: number[]) => cells.map((c) => screenCentre(cal, c, dpr).join(",")).join(" ");
  const points = $derived(line(net?.path ?? []));
  /** cast into a portal: the spell's way to where the new portal lands */
  const cast = $derived(line(net?.cast ?? []));
  const exit = $derived(net && net.path.length > 1 ? net.path[net.path.length - 1] : null);
  /** size of a cell on screen (CSS px) */
  const cell = $derived([cal.sx / dpr, cal.sy / dpr]);
  const label = $derived(net ? `${net.distance} cases · +${net.distance * PORTAL_BONUS} %` : "");
</script>

<svg width="100%" height="100%">
  {#if net}
    {#if net.dropped !== null}
      {@const [x, y] = screenCentre(cal, net.dropped, dpr)}
      <polygon points={screenDiamond(cal, net.dropped, dpr)} class="dropped" />
      <path d="M{x - cell[0] / 5} {y - cell[1] / 10}L{x + cell[0] / 5} {y + cell[1] / 10}M{x + cell[0] / 5} {y - cell[1] / 10}L{x - cell[0] / 5} {y + cell[1] / 10}" class="cross" />
    {/if}
    {#each net.portals as p (p.cell)}
      <polygon points={screenDiamond(cal, p.cell, dpr)} class="portal" class:closed={!p.open} class:added={p.cell === net.added}
        style:--c={portalColour(p.number, full)} />
    {/each}
    {#if start != null}
      <polygon points={screenDiamond(cal, start, dpr)} class="start" />
    {/if}
    {#if cast}
      <polyline points={cast} class="cast" />
    {/if}
    {#if points && exit !== null}
      <polygon points={screenDiamond(cal, exit, dpr)} class="exit" />
      <polyline {points} class="way under" />
      <polyline {points} class="way" />
    {/if}
    {#each net.portals as p (p.cell)}
      {@const [x, y] = screenCentre(cal, p.cell, dpr)}
      {@const r = Math.max(7, cell[1] / 7)}
      <circle cx={x} cy={y - cell[1] / 4} {r} class="badge" class:closed={!p.open} style:--c={portalColour(p.number, full)} />
      <text x={x} y={y - cell[1] / 4} font-size={r * 1.3} class="number">{p.number}</text>
    {/each}
    {#if exit !== null}
      {@const [x, y] = screenCentre(cal, exit, dpr)}
      <text x={x} y={y + cell[1] / 2.6} font-size={Math.max(12, cell[1] / 5)} class="length under">{label}</text>
      <text x={x} y={y + cell[1] / 2.6} font-size={Math.max(12, cell[1] / 5)} class="length">{label}</text>
    {/if}
  {/if}
</svg>

<style>
  .portal { fill: var(--c); fill-opacity: 0.25; stroke: var(--c); stroke-width: 2.5; }
  .portal.closed, .badge.closed { --c: #8c9096; }
  .portal.closed { stroke-dasharray: 4 3; }
  .portal.added { stroke-dasharray: 6 3; fill-opacity: 0.4; }
  .start { fill: none; stroke: #fff; stroke-width: 3.5; }
  .dropped { fill: rgba(140, 144, 150, 0.25); stroke: #8c9096; stroke-width: 2; stroke-dasharray: 4 3; }
  .cross { stroke: #ff4d4d; stroke-width: 3; stroke-linecap: round; }
  .exit { fill: #fff; stroke: #fff; stroke-width: 2; animation: pulse 0.9s ease-in-out infinite alternate; }
  @keyframes pulse { from { fill-opacity: 0.1; } to { fill-opacity: 0.55; } }
  .way { fill: none; stroke: #ffd23c; stroke-opacity: 0.75; stroke-width: 3; stroke-linejoin: round; }
  .way.under { stroke: rgba(0, 0, 0, 0.35); stroke-opacity: 1; stroke-width: 5; }
  .cast { fill: none; stroke: #fff; stroke-opacity: 0.45; stroke-width: 1.5; stroke-dasharray: 5 4; stroke-linejoin: round; }
  .length.under { stroke: rgba(0, 0, 0, 0.75); stroke-width: 6; }
  .badge { fill: var(--c); stroke: rgba(0, 0, 0, 0.7); stroke-width: 1.5; }
  .number { fill: #fff; font-weight: 700; text-anchor: middle; dominant-baseline: central; paint-order: stroke; stroke: rgba(0, 0, 0, 0.6); stroke-width: 2px; }
  .length { fill: #fff; font-weight: 700; text-anchor: middle; dominant-baseline: central; }
</style>
