<script lang="ts">
  import { screenCentre, screenDiamond, type Calibration } from "../lib/fightMap";
  import { PORTAL_BONUS, type PortalSim } from "../lib/classHelp";

  /** A way through the Eliotrope's portals (PortalSimLayer.svelte, PortalLayer.svelte): the line from its first portal to its
   *  exit, the damage bonus there (PORTAL_BONUS % per cell), in yellow the cell a spell sent into it ends up on. */
  let { cal, dpr, sim }: { cal: Calibration; dpr: number; sim: PortalSim } = $props();

  const exit = $derived(sim.path.length > 1 ? sim.path[sim.path.length - 1] : null);
  const points = $derived(sim.path.map((c) => screenCentre(cal, c, dpr).join(",")).join(" "));
  /** size of a cell on screen (CSS px) */
  const cell = $derived([cal.sx / dpr, cal.sy / dpr]);
</script>

{#if sim.target !== null}
  <polygon points={screenDiamond(cal, sim.target, dpr)} class="target" />
{/if}
{#if exit !== null}
  {@const [x, y] = screenCentre(cal, exit, dpr)}
  {@const size = Math.max(10, cell[1] / 7)}
  <polyline {points} class="way under" />
  <polyline {points} class="way" />
  <text x={x} y={y + cell[1] / 2.6} font-size={size} class="bonus under">+{sim.distance * PORTAL_BONUS} %</text>
  <text x={x} y={y + cell[1] / 2.6} font-size={size} class="bonus">+{sim.distance * PORTAL_BONUS} %</text>
{/if}

<style>
  .target { fill: rgba(255, 210, 60, 0.45); stroke: #ffd23c; stroke-width: 2; }
  .way { fill: none; stroke: #ffd23c; stroke-opacity: 0.75; stroke-width: 3; stroke-linejoin: round; }
  .way.under { stroke: rgba(0, 0, 0, 0.35); stroke-opacity: 1; stroke-width: 5; }
  .bonus { fill: #fff; font-weight: 700; text-anchor: middle; dominant-baseline: central; }
  .bonus.under { stroke: rgba(0, 0, 0, 0.75); stroke-width: 6; }
</style>
