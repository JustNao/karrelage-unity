<script lang="ts">
  /** Progress bar: `value` from 0 to 1. `busy` adds a moving sheen (work still running), `fail` turns it red. */
  let {
    value,
    busy = false,
    fail = false,
    height = 8,
    width = "100%",
    speed = 0.25,
  }: { value: number; busy?: boolean; fail?: boolean; height?: number; width?: string; /** Seconds the fill takes to catch up. */ speed?: number } = $props();
  const pct = $derived(Math.round(Math.min(1, Math.max(0, value)) * 100));
</script>

<span class="bar" role="progressbar" aria-valuenow={pct} aria-valuemin="0" aria-valuemax="100" style:height="{height}px" style:width style:border-radius="{height / 2}px">
  <span class="fill" class:busy class:fail style:width="{pct}%" style:transition="width {speed}s"></span>
</span>

<style>
  .bar { display: inline-block; vertical-align: middle; background: var(--line); overflow: hidden; }
  .fill { display: block; height: 100%; background: var(--acc); }
  .fill.busy { background-image: linear-gradient(110deg, transparent 30%, #fff4 50%, transparent 70%); background-size: 200% 100%; animation: sweep 1.4s linear infinite; }
  .fill.fail { background: var(--bad); }
  @keyframes sweep { from { background-position: 200% 0; } to { background-position: -200% 0; } }
</style>
