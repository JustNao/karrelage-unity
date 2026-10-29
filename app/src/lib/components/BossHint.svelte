<script lang="ts">
  import type { Hint } from "../bosses";

  /** One boss hint (lib/bosses.ts): the instruction, and when it says where to aim, a compass: the player in the middle, the
   *  target above, the cell to aim at lit. */
  let { hint }: { hint: Hint } = $props();
  // aim (quarter turns clockwise from the target) -> grid cell [column, row], the player at [1, 1]
  const CELLS: [number, number][] = [[1, 0], [2, 1], [1, 2], [0, 1]];
</script>

<div class="hint">
  {#if hint.aim != null}
    <svg viewBox="0 0 60 60" width="54" height="54" aria-hidden="true">
      {#each CELLS as [x, y], i (i)}
        <rect x={x * 20 + 2} y={y * 20 + 2} width="16" height="16" rx="3" class:target={i === 0} class:aim={i === hint.aim} />
      {/each}
      <circle cx="30" cy="30" r="6" class="me" />
    </svg>
  {/if}
  <div class="txt">
    <div class="boss">{hint.boss}</div>
    <div class="main">{hint.text}</div>
    {#if hint.detail}<div class="detail">{hint.detail}</div>{/if}
  </div>
</div>

<style>
  .hint { display: flex; gap: 10px; align-items: center; margin: 6px 8px; padding: 6px 8px; border-radius: 6px; background: rgba(90, 169, 255, 0.14); border: 1px solid rgba(90, 169, 255, 0.45); }
  svg { flex: none; }
  rect { fill: rgba(255, 255, 255, 0.06); stroke: rgba(255, 255, 255, 0.25); stroke-width: 1; }
  rect.target { stroke: var(--warn); stroke-width: 2; }
  rect.aim { fill: var(--acc); stroke: var(--acc); }
  .me { fill: var(--ok); }
  .txt { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .boss { font-size: 10px; text-transform: uppercase; opacity: 0.7; }
  .main { font-weight: 700; font-size: 14px; }
  .detail { font-size: 11px; opacity: 0.8; line-height: 1.3; }
</style>
