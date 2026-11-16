<script lang="ts">
  import { ELEMENTS } from "../lib/bosses";
  import { attackerStats, bestElement } from "../lib/damage";
  import { screenCentre, type Calibration, type FightMapState, type MapFighter } from "../lib/fightMap";
  import { settings } from "../lib/stores/settings.svelte";

  /**
   * Weakest element of each enemy in the in-game view (line of sight, F3; or always, `settings.combat.weakElementAlways`):
   * written at the fighter's feet, in its element's colour, with its % resistance under it. Resistances are the fight's live ones (crates/modules/src/fight_map: the fighter's stats,
   * then each stats refresh: buffs and debuffs count). Neutral left out. Ties: every tied element (all four: « Tous »).
   * Enemies: not on the team of the player's characters (`mine`), else every monster when the player is not known.
   * `settings.combat.weakElementMode` "damage": the element the player's build hits hardest instead (lib/damage.ts: a 30 damage
   * spell with their characteristics, power and damages against the enemy's % and fixed resistances; whose turn it is when one
   * of `mine` (`playing`)), alone (no value under it). Their characteristics unknown yet: the weakest element as before.
   */
  let { cal, live, mine, dpr, playing = null }: {
    cal: Calibration;
    live: FightMapState;
    mine: Set<number>;
    dpr: number;
    playing?: number | null;
  } = $props();

  const attacker = $derived(settings.combat.weakElementMode === "damage" ? attackerStats(live, mine, playing) : null);

  /** neutral, earth, fire, water, air (ELEMENTS' order) */
  const COLOURS = ["#c8c8c8", "#c9a15a", "#ff6a3d", "#4fb3ff", "#6fdc6f"];

  const enemies = $derived.by(() => {
    const teams = new Set(live.fighters.filter((f) => mine.has(f.id)).map((f) => f.team));
    const enemy = (f: MapFighter) => (teams.size ? f.team !== null && !teams.has(f.team) : f.kind === "monster");
    return live.fighters.filter((f) => f.alive && !f.carried && !f.invisible && f.cell >= 0 && f.resists && enemy(f));
  });

  /** The lowest resistance among earth, fire, water and air: neutral is never offered (few spells hit neutral), the next
   *  lowest element is shown instead. */
  function weakest(r: number[]) {
    const low = Math.min(...r.slice(1));
    return { value: `${low} %`, elements: r.flatMap((v, i) => (i > 0 && v === low ? [i] : [])) };
  }

  /** What is written on an enemy: the best element for the player's build, else its weakest. */
  function shown(f: MapFighter) {
    const best = attacker && bestElement(attacker, f);
    return best ? { value: null, elements: best.elements } : weakest(f.resists!);
  }

  /** text size from the cell's size on screen */
  const size = $derived(Math.max(12, (cal.sx / dpr) * 0.17));
</script>

<svg width="100%" height="100%">
  {#each enemies as f (f.id)}
    {@const [x, y] = screenCentre(cal, f.cell, dpr)}
    {@const w = shown(f)}
    <!-- at the fighter's feet: both lines centred on the cell it stands on (name cap height ~0.7 em above its baseline,
         value baseline ~1 em under it) -->
    <text {x} y={y - size * 0.2} font-size={size} class="weak">
      {#if w.elements.length === 4}<tspan fill="#fff">Tous</tspan>
      {:else}
        {#each w.elements as e, i (e)}{#if i}<tspan fill="#fff"> / </tspan>{/if}<tspan fill={COLOURS[e]}>{ELEMENTS[e]}</tspan>{/each}
      {/if}
      {#if w.value}<tspan {x} dy="1.15em" fill="#fff" class="value">{w.value}</tspan>{/if}
    </text>
  {/each}
</svg>

<style>
  .weak { font-family: var(--font); font-weight: 700; text-anchor: middle; paint-order: stroke; stroke: rgba(0, 0, 0, 0.85); stroke-width: 3px; stroke-linejoin: round; }
  .value { font-weight: 600; font-size: 0.85em; }
</style>
