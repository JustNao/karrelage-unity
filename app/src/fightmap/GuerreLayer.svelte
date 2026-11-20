<script lang="ts">
  import { guerreWeapon } from "../lib/bosses";
  import { screenCentre, screenDiamond, type Calibration, type FightMapState } from "../lib/fightMap";

  /**
   * Guerre in the in-game view (boss help, fightmap/BossLayer.svelte): where her next giant weapon would be summoned now (2 PO
   * in line up-right of her, else clockwise) and the zone it strikes from there (lib/bosses.ts `guerreWeapon`). Follows her and the fighters.
   */
  let { cal, live, cells, dpr }: {
    cal: Calibration;
    live: FightMapState;
    /** the map's cell flags */
    cells: number[] | null;
    dpr: number;
  } = $props();

  const next = $derived(guerreWeapon(live, cells));
  const label = $derived(next?.at != null ? screenCentre(cal, next.at, dpr) : null);
</script>

{#if next}
  <svg width="100%" height="100%">
    {#each next.area as c (c)}
      {#if c !== next.at}<polygon points={screenDiamond(cal, c, dpr)} class="area" />{/if}
    {/each}
    {#if next.at !== null}<polygon points={screenDiamond(cal, next.at, dpr)} class="weapon" />{/if}
  </svg>
  {#if label}
    <div class="tag" style:left="{label[0]}px" style:top="{label[1]}px">{next.weapon.name} · {next.weapon.zone} ({next.weapon.element})</div>
  {:else}
    <div class="pill">{next.weapon.name} : aucune case libre à 2 PO de Guerre</div>
  {/if}
{/if}

<style>
  .area { fill: rgba(255, 30, 30, 0.35); stroke: #ff3030; stroke-opacity: 0.8; stroke-width: 1; }
  .weapon { fill: rgba(255, 200, 0, 0.45); stroke: #ffc800; stroke-width: 3; }
  .tag {
    position: absolute; transform: translate(-50%, -260%); padding: 2px 8px; font-size: 12px; font-weight: 600; white-space: nowrap;
    background: var(--panel); color: var(--fg); border: 1px solid #ffc800; border-radius: 6px; pointer-events: none; opacity: 0.92;
  }
</style>
