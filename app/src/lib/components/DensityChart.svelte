<script lang="ts" module>
  export interface Point {
    id: string | number;
    /** Price. */
    x: number;
    /** Item density (value of its effects). Not displayed: only the position relative to the trend matters. */
    y: number;
    label: string;
  }
</script>

<script lang="ts">
  import { kamas } from "../api";

  /**
   * Price (x) against density (y) of the lots of one item, with a least-squares trend line and a ±2σ band.
   * Dots well above the line carry more value than their price suggests (bargains); well below = overpriced.
   */
  let { points, selected = null, onselect }: { points: Point[]; selected?: string | number | null; onselect: (id: string | number) => void } = $props();

  const H = 220;
  const M = { l: 14, r: 14, t: 12, b: 26 };
  let W = $state(600);
  let hover = $state<number | null>(null);

  const fit = $derived.by(() => {
    const n = points.length;
    const mx = points.reduce((s, p) => s + p.x, 0) / n;
    const my = points.reduce((s, p) => s + p.y, 0) / n;
    const sxx = points.reduce((s, p) => s + (p.x - mx) ** 2, 0);
    const slope = sxx > 0 ? points.reduce((s, p) => s + (p.x - mx) * (p.y - my), 0) / sxx : 0;
    const icpt = my - slope * mx;
    const res = points.map((p) => p.y - (slope * p.x + icpt));
    const sigma = Math.sqrt(res.reduce((s, r) => s + r * r, 0) / Math.max(1, n - 2));
    return { slope, icpt, sigma, z: res.map((r) => (sigma > 0 ? r / sigma : 0)) };
  });

  const dom = $derived.by(() => {
    const xs = points.map((p) => p.x);
    const [x0, x1] = [Math.min(...xs), Math.max(...xs)];
    const padX = (x1 - x0 || x0 || 1) * 0.06;
    const ys = [...points.map((p) => p.y), ...[x0, x1].flatMap((x) => [fit.slope * x + fit.icpt - 2 * fit.sigma, fit.slope * x + fit.icpt + 2 * fit.sigma])];
    const [y0, y1] = [Math.min(...ys), Math.max(...ys)];
    const padY = (y1 - y0 || 1) * 0.08;
    return { x0: x0 - padX, x1: x1 + padX, y0: y0 - padY, y1: y1 + padY };
  });

  const sx = (x: number) => M.l + ((x - dom.x0) / (dom.x1 - dom.x0)) * (W - M.l - M.r);
  const sy = (y: number) => H - M.b - ((y - dom.y0) / (dom.y1 - dom.y0)) * (H - M.t - M.b);
  const line = (off = 0) => `M${sx(dom.x0)},${sy(fit.slope * dom.x0 + fit.icpt + off)} L${sx(dom.x1)},${sy(fit.slope * dom.x1 + fit.icpt + off)}`;
  const band = $derived(
    `M${sx(dom.x0)},${sy(fit.slope * dom.x0 + fit.icpt + 2 * fit.sigma)} L${sx(dom.x1)},${sy(fit.slope * dom.x1 + fit.icpt + 2 * fit.sigma)} ` +
      `L${sx(dom.x1)},${sy(fit.slope * dom.x1 + fit.icpt - 2 * fit.sigma)} L${sx(dom.x0)},${sy(fit.slope * dom.x0 + fit.icpt - 2 * fit.sigma)} Z`,
  );

  const compact = (n: number) => (n >= 1e6 ? +(n / 1e6).toFixed(2) + "M" : n >= 1e3 ? +(n / 1e3).toFixed(1) + "k" : String(Math.round(n)));
  const ticks = $derived.by(() => {
    const raw = (dom.x1 - dom.x0) / 5;
    const mag = 10 ** Math.floor(Math.log10(raw));
    const step = ([1, 2, 5, 10].find((m) => m * mag >= raw) ?? 10) * mag;
    const out: number[] = [];
    for (let t = Math.ceil(dom.x0 / step) * step; t <= dom.x1; t += step) out.push(t);
    return out;
  });

  const tone = (z: number) => (z >= 2 ? "good" : z <= -2 ? "bad" : "mid");
  const info = $derived(hover === null ? null : { p: points[hover], z: fit.z[hover] });
</script>

<div class="chart" bind:clientWidth={W}>
  <div class="status">
    {#if info}
      <b>{info.p.label}</b> · {kamas(info.p.x)} kamas ·
      <span class={tone(info.z)}>{info.z >= 0 ? "+" : ""}{info.z.toFixed(1)}σ {info.z >= 2 ? "· worth more than its price" : info.z <= -2 ? "· overpriced" : ""}</span>
    {:else}
      <span class="mut">Price → · above the line = more value than its price suggests · click a dot to jump to the lot</span>
    {/if}
  </div>
  <svg width={W} height={H} role="img" aria-label="Density against price">
    <path d={band} class="band" />
    <line x1={M.l} x2={W - M.r} y1={H - M.b} y2={H - M.b} class="axis" />
    {#each ticks as t}
      <line x1={sx(t)} x2={sx(t)} y1={H - M.b} y2={H - M.b + 4} class="axis" />
      <text x={sx(t)} y={H - 8} class="tick">{compact(t)}</text>
    {/each}
    <path d={line()} class="trend" />
    {#each points as p, i (p.id)}
      {@const z = fit.z[i]}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <g class="dot {tone(z)}" class:sel={selected === p.id} onclick={() => onselect(p.id)} onmouseenter={() => (hover = i)} onmouseleave={() => (hover = null)}>
        <circle cx={sx(p.x)} cy={sy(p.y)} r="11" class="hit" />
        <circle cx={sx(p.x)} cy={sy(p.y)} r={Math.abs(z) >= 2 ? 6 : 4.5} class="pt" />
        {#if selected === p.id}<circle cx={sx(p.x)} cy={sy(p.y)} r="10" class="ring" />{/if}
      </g>
    {/each}
  </svg>
</div>

<style>
  .chart { width: 100%; font-family: system-ui, sans-serif; }
  .status { min-height: 20px; font-size: 12px; margin-bottom: 4px; }
  svg { display: block; background: #0a0b0e; border: 1px solid #2a2e38; border-radius: 8px; }
  .band { fill: color-mix(in srgb, var(--acc) 9%, transparent); }
  .trend { stroke: var(--acc); stroke-width: 1.5; stroke-dasharray: 5 4; fill: none; opacity: 0.85; }
  .axis { stroke: #3a3f4b; stroke-width: 1; }
  .tick { fill: var(--mut); font-size: 10px; text-anchor: middle; }
  .dot { cursor: pointer; }
  .hit { fill: transparent; }
  .pt { transition: r 0.12s; stroke: #0a0b0e; stroke-width: 1; }
  .dot:hover .pt { r: 7; }
  .good .pt { fill: var(--ok); }
  .bad .pt { fill: var(--bad); }
  .mid .pt { fill: #8a91a0; }
  .ring { fill: none; stroke: #fff; stroke-width: 1.5; }
  .good { color: var(--ok); }
  .bad { color: var(--bad); }
  .mut { color: var(--mut); }
</style>
