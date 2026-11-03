<script lang="ts">
  import { call } from "../lib/api";
  import { screenCentre, screenDiamond, type Calibration, type FightMapState } from "../lib/fightMap";
  import { PORTAL_BONUS, portalAuthor, portalColour, type PortalNet } from "../lib/classHelp";

  /**
   * Eliotrope portals in the in-game view (inside ClassLayer.svelte): the player's portals numbered by age, the two that go
   * first in colour once there are four. `simulate` (view `portals`): a portal put down on the cell under the mouse (the
   * oldest one struck out when there were four) and the way from it through the network to its exit, with its length and
   * damage bonus (PORTAL_BONUS % per cell); the mouse on one of the portals: the way from that one.
   */
  let { cal, live, mine, dpr, pointed, simulate, net = $bindable(null) }: {
    cal: Calibration;
    live: FightMapState;
    mine: Set<number>;
    dpr: number;
    pointed: number | null;
    simulate: boolean;
    net?: PortalNet | null;
  } = $props();

  const author = $derived(portalAuthor(live, mine));

  let asked = 0;
  $effect(() => {
    const [who, add] = [author, simulate ? pointed : null];
    void live.portals;
    void live.fighters;
    if (who === null) return void (net = null);
    const n = ++asked;
    call<PortalNet>("fightmap", "portals", { author: who, ...(add !== null && { add }) }, true)
      .then((r) => n === asked && (net = r))
      .catch(() => {});
  });

  const full = $derived((net?.portals.length ?? 0) >= 4);
  const points = $derived((net?.path ?? []).map((c) => screenCentre(cal, c, dpr).join(",")).join(" "));
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
  .dropped { fill: rgba(140, 144, 150, 0.25); stroke: #8c9096; stroke-width: 2; stroke-dasharray: 4 3; }
  .cross { stroke: #ff4d4d; stroke-width: 3; stroke-linecap: round; }
  .exit { fill: #fff; stroke: #fff; stroke-width: 2; animation: pulse 0.9s ease-in-out infinite alternate; }
  @keyframes pulse { from { fill-opacity: 0.1; } to { fill-opacity: 0.55; } }
  .way { fill: none; stroke: #f5f5f5; stroke-width: 3; stroke-linejoin: round; stroke-dasharray: 10 6; animation: flow 0.6s linear infinite; }
  .way.under, .length.under { stroke: rgba(0, 0, 0, 0.75); stroke-width: 6; stroke-dasharray: none; animation: none; }
  @keyframes flow { to { stroke-dashoffset: -16; } }
  .badge { fill: var(--c); stroke: rgba(0, 0, 0, 0.7); stroke-width: 1.5; }
  .number { fill: #fff; font-weight: 700; text-anchor: middle; dominant-baseline: central; paint-order: stroke; stroke: rgba(0, 0, 0, 0.6); stroke-width: 2px; }
  .length { fill: #fff; font-weight: 700; text-anchor: middle; dominant-baseline: central; }
</style>
