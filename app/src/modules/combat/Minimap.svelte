<script lang="ts">
  import { centre, diamond, mapSize, LOS, MAP_CELLS, MOV, type MapFighter, type Sight } from "../../lib/fightMap";

  interface Props {
    cells: number[];
    fighters: MapFighter[];
    /** the team drawn as allies (the player's), null = unknown */
    allies: number | null;
    /** the player's fighter ids */
    mine: Set<number>;
    names: Map<number, string>;
    sight: Sight | null;
    /** cell the line of sight is drawn from (hovered or pinned) */
    from: number | null;
    pinned: boolean;
    onhover: (cell: number | null) => void;
    onpick: (cell: number) => void;
  }
  let { cells, fighters, allies, mine, names, sight, from, pinned, onhover, onpick }: Props = $props();

  const W = 40;
  const all = Array.from({ length: MAP_CELLS }, (_, i) => i);
  const kindOf = (f: number) => (f & LOS) === 0 ? "wall" : (f & MOV) === 0 ? "hole" : "floor";
  /** framed on the cells that are not holes (most maps leave a band of them around the fight area) */
  const view = $derived.by(() => {
    const used = all.filter((c) => kindOf(cells[c]) !== "hole");
    if (!used.length) return `0 0 ${mapSize(W).join(" ")}`;
    const xs = used.map((c) => centre(c, W)[0]), ys = used.map((c) => centre(c, W)[1]);
    const [x0, y0] = [Math.min(...xs) - W / 2 - 4, Math.min(...ys) - W / 4 - 4];
    return `${x0} ${y0} ${Math.max(...xs) + W / 2 + 4 - x0} ${Math.max(...ys) + W / 4 + 4 - y0}`;
  });
  const seen = $derived(new Set(sight?.seen ?? []));
  const hidden = $derived(new Set(sight?.hidden ?? []));
  const shown = $derived(fighters.filter((f) => f.alive && !f.carried && f.cell >= 0 && f.cell < MAP_CELLS));
  const side = (f: MapFighter) => (mine.has(f.id) ? "me" : allies === null ? (f.kind === "player" ? "ally" : "enemy") : f.team === allies ? "ally" : "enemy");
  const label = (f: MapFighter) => names.get(f.id) ?? (f.kind === "player" ? "Personnage" : f.kind === "monster" ? "Monstre" : "Créature");
</script>

<svg viewBox={view} role="img" aria-label="Carte du combat" onmouseleave={() => onhover(null)}>
  {#each all as c (c)}
    {@const k = kindOf(cells[c])}
    <polygon
      points={diamond(c, W)}
      class="cell {k}"
      class:seen={seen.has(c)}
      class:hidden={hidden.has(c)}
      class:from={from === c}
      onmouseenter={() => k === "floor" && onhover(c)}
      onclick={() => k === "floor" && onpick(c)}
      role="presentation"
    />
  {/each}
  <!-- the pinned cell's border, drawn after every cell (SVG paints in order: the neighbours' borders would cover it) -->
  {#if pinned && from !== null}
    <polygon points={diamond(from, W)} class="pinned" />
  {/if}
  {#each shown as f (f.id)}
    {@const [x, y] = centre(f.cell, W)}
    <g class="fighter {side(f)}" class:summon={f.summoned} transform="translate({x} {y})" onmouseenter={() => onhover(f.cell)} onclick={() => onpick(f.cell)} role="presentation">
      <title>{label(f)}{f.summoned ? " (invocation)" : ""}</title>
      <ellipse rx={f.summoned ? 9 : 12} ry={f.summoned ? 5 : 7} />
    </g>
  {/each}
</svg>

<style>
  svg { width: 100%; height: auto; max-height: calc(100vh - 210px); display: block; user-select: none; }
  .cell { stroke: var(--line); stroke-width: 1; }
  .floor { fill: var(--panel); cursor: crosshair; }
  .hole { fill: transparent; stroke: transparent; }
  .wall { fill: color-mix(in srgb, var(--mut) 55%, var(--bg)); stroke: color-mix(in srgb, var(--mut) 55%, var(--bg)); }
  .floor:hover { fill: color-mix(in srgb, var(--acc) 18%, var(--panel)); }
  .seen { fill: color-mix(in srgb, var(--acc) 45%, var(--panel)); }
  .seen:hover { fill: color-mix(in srgb, var(--acc) 60%, var(--panel)); }
  .hidden { fill: color-mix(in srgb, var(--acc) 20%, var(--panel)); }
  .from { fill: var(--gold); }
  .pinned { fill: none; stroke: var(--fg); stroke-width: 2; pointer-events: none; }
  .fighter { cursor: crosshair; }
  .fighter ellipse { stroke: var(--bg); stroke-width: 2; }
  .fighter.enemy ellipse { fill: var(--bad); }
  .fighter.ally ellipse { fill: var(--ok); }
  .fighter.me ellipse { fill: var(--gold); stroke: var(--fg); }
  .fighter.summon ellipse { opacity: 0.85; stroke-dasharray: 3 2; }
</style>
