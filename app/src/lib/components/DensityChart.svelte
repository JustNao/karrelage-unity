<script lang="ts" module>
  export interface Point {
    id: string | number;
    /** Price. */
    x: number;
    /** Item density (value of its effects). Not displayed: only the position relative to the trend matters. */
    y: number;
    label: string;
    /** The lot has an effect the item does not have by default. */
    added?: boolean;
    /** The lot lacks some default effect. */
    missing?: boolean;
  }
</script>

<script lang="ts">
  import { analyse, isGoodDeal, type Fit } from "../density";
  import { fmtCompact as compact } from "../fmt";

  /**
   * Price (x) against density (y) of the lots of one item, with a least-squares trend line and a ±2σ band.
   * Green dots sit far above the line: more value than their price suggests (good deals). Blue = has an added effect,
   * orange = lacks some default effect.
   */
  let {
    points,
    fit: given = null,
    faded = null,
    selected = null,
    equipment = true,
    onselect,
  }: {
    points: Point[];
    /** The fit of `points` when the caller already has it (computed here otherwise). */
    fit?: Fit | null;
    /** Ids drawn faded (lots lacking the effect being looked at); kept apart from the points so fading never refits. */
    faded?: Set<string | number> | null;
    selected?: string | number | null;
    /** Equipment only: EXO / missing-effect legend entries are meaningless for companions and mounts. */
    equipment?: boolean;
    onselect: (id: string | number) => void;
  } = $props();

  const H = 220;
  const M = { l: 14, r: 14, t: 12, b: 26 };
  let W = $state(600);

  const fit = $derived(given ?? analyse(points));

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

  const ticks = $derived.by(() => {
    const raw = (dom.x1 - dom.x0) / 5;
    const mag = 10 ** Math.floor(Math.log10(raw));
    const step = ([1, 2, 5, 10].find((m) => m * mag >= raw) ?? 10) * mag;
    const out: number[] = [];
    for (let t = Math.ceil(dom.x0 / step) * step; t <= dom.x1; t += step) out.push(t);
    return out;
  });

  /** green = good deal, else blue (added effect) / orange (missing effect), both = blue with an orange ring. */
  type Tone = "good" | "added" | "missing" | "both" | "mid";
  const tone = (i: number): Tone => {
    const p = points[i];
    return isGoodDeal(fit, i, points.length) ? "good" : p.added ? (p.missing ? "both" : "added") : p.missing ? "missing" : "mid";
  };
</script>

<div class="chart" bind:clientWidth={W}>
  <div class="legend">
    <span><i class="d good"></i>Bonne affaire</span>{#if equipment}<span><i class="d added"></i>EXO</span><span><i class="d missing"></i>Effet manquant</span>{/if}
  </div>
  <svg width={W} height={H} role="img" aria-label="Densité en fonction du prix">
    <path d={band} class="band" />
    <line x1={M.l} x2={W - M.r} y1={H - M.b} y2={H - M.b} class="axis" />
    {#each ticks as t}
      <line x1={sx(t)} x2={sx(t)} y1={H - M.b} y2={H - M.b + 4} class="axis" />
      <text x={sx(t)} y={H - 8} class="tick">{compact(t)}</text>
    {/each}
    <path d={line()} class="trend" />
    {#each points as p, i (p.id)}
      {@const t = tone(i)}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <g class="dot {t}" class:faded={faded?.has(p.id)} class:sel={selected === p.id} onclick={() => onselect(p.id)}>
        <circle cx={sx(p.x)} cy={sy(p.y)} r="11" class="hit" />
        <circle cx={sx(p.x)} cy={sy(p.y)} r={t === "good" ? 6.5 : 4.5} class="pt" />
        {#if selected === p.id}<circle cx={sx(p.x)} cy={sy(p.y)} r="10" class="ring" />{/if}
      </g>
    {/each}
  </svg>
</div>

<style>
  .chart { width: 100%; font-family: var(--font); }
  .legend { display: flex; gap: 14px; min-height: 20px; font-size: 12px; margin-bottom: 4px; color: var(--mut); }
  svg { display: block; background: #0a0b0e; border: 1px solid #2a2e38; border-radius: 8px; }
  .band { fill: color-mix(in srgb, var(--acc) 9%, transparent); }
  .trend { stroke: var(--acc); stroke-width: 1.5; stroke-dasharray: 5 4; fill: none; opacity: 0.85; }
  .axis { stroke: #3a3f4b; stroke-width: 1; }
  .tick { fill: var(--mut); font-size: 10px; text-anchor: middle; }
  .dot { cursor: pointer; transition: opacity 0.25s; }
  .dot.faded { opacity: 0.14; }
  .hit { fill: transparent; }
  .pt { transition: r 0.12s; stroke: #0a0b0e; stroke-width: 1; }
  .dot:hover .pt { r: 7; }
  .good .pt { fill: var(--ok); }
  .added .pt, .both .pt { fill: #4a9eff; }
  .missing .pt { fill: #f08a3c; }
  .both .pt { stroke: #f08a3c; stroke-width: 2; }
  .mid .pt { fill: #8a91a0; }
  .ring { fill: none; stroke: #fff; stroke-width: 1.5; }
  .d { display: inline-block; width: 8px; height: 8px; border-radius: 50%; margin: 0 5px 0 0; background: #8a91a0; }
  .d.good { background: var(--ok); }
  .d.added { background: #4a9eff; }
  .d.missing { background: #f08a3c; }
  .mut { color: var(--mut); }
</style>
