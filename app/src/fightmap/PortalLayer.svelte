<script lang="ts">
  import { screenCentre, screenDiamond, type Calibration, type FightMapState } from "../lib/fightMap";
  import { portalColour, type PlayerPortal } from "../lib/classHelp";
  import PortalHover from "./PortalHover.svelte";

  /** The Eliotrope's portals in the class help (inside ClassLayer.svelte): numbered by age, the two that go first in colour once
   *  there are four; the mouse over an open one: its way and damage bonus (PortalHover.svelte). */
  let { cal, dpr, portals, live, mine, pointed }: {
    cal: Calibration;
    dpr: number;
    portals: PlayerPortal[];
    live: FightMapState;
    mine: Set<number>;
    /** the cell under the mouse */
    pointed: number | null;
  } = $props();

  const full = $derived(portals.length >= 4);
  /** a cell's height on screen (CSS px) */
  const cellH = $derived(cal.sy / dpr);
</script>

<svg width="100%" height="100%">
  {#each portals as p (p.cell)}
    <polygon points={screenDiamond(cal, p.cell, dpr)} class="portal" class:closed={!p.open} style:--c={portalColour(p.number, full)} />
  {/each}
  {#each portals as p (p.cell)}
    {@const [x, y] = screenCentre(cal, p.cell, dpr)}
    {@const r = Math.max(7, cellH / 7)}
    <circle cx={x} cy={y - cellH / 4} {r} class="badge" class:closed={!p.open} style:--c={portalColour(p.number, full)} />
    <text x={x} y={y - cellH / 4} font-size={r * 1.3} class="number">{p.number}</text>
  {/each}
</svg>
{#if portals.length}<PortalHover {cal} {dpr} {live} {mine} {pointed} />{/if}

<style>
  .portal { fill: var(--c); fill-opacity: 0.25; stroke: var(--c); stroke-width: 2.5; }
  .portal.closed, .badge.closed { --c: #8c9096; }
  .portal.closed { stroke-dasharray: 4 3; }
  .badge { fill: var(--c); stroke: rgba(0, 0, 0, 0.7); stroke-width: 1.5; }
  .number { fill: #fff; font-weight: 700; text-anchor: middle; dominant-baseline: central; paint-order: stroke; stroke: rgba(0, 0, 0, 0.6); stroke-width: 2px; }
</style>
