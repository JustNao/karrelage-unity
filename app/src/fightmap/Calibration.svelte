<script lang="ts">
  import { writeJSON } from "../lib/persist.svelte";
  import { fight } from "../lib/stores/combat.svelte";
  import { CALIBRATION_KEY, calibrate, centre, type Calibration, type FightMapState, type MapFighter } from "../lib/fightMap";

  /**
   * Calibration by hand of the in-game view (FightMapOverlay.svelte), asked in the Combat tab when the grid computed from the
   * window's size is off: the player clicks the centres of the two fighters named (lib/fightMap.ts `calibrate`); saved, then `oncalibrated`. The window takes the mouse meanwhile.
   */
  let { live, mine, dpr, width, height, oncalibrated }: {
    live: FightMapState;
    mine: Set<number>;
    dpr: number;
    /** the Dofus window's client area (physical px) */
    width: number;
    height: number;
    oncalibrated: (c: Calibration) => void;
  } = $props();

  /** clicks so far (physical px of the client area) */
  let clicks = $state.raw<[number, number][]>([]);
  let failed = $state(false);
  /** the mouse (CSS px): a cross about a cell wide is drawn there */
  let pointer = $state<[number, number] | null>(null);

  const names = $derived.by(() => {
    const m = new Map<number, string>();
    for (const f of fight.fighters) m.set(f.actor, f.name);
    for (const c of fight.characters ?? []) if (c.name) m.set(c.id, c.name);
    return m;
  });
  const alive = $derived(live.fighters.filter((f) => f.alive && !f.carried && f.cell >= 0));

  /** The two fighters to click: the two farthest apart on both axes (the calibration is more precise the farther apart they
   *  are), the player's character first when it is one of them. */
  const targets = $derived.by((): [MapFighter, MapFighter] | null => {
    const at = alive.map((f) => [f, ...centre(f.cell, 1)] as const);
    let best: [MapFighter, MapFighter] | null = null, most = -1;
    for (let i = 0; i < at.length; i++)
      for (let j = i + 1; j < at.length; j++) {
        const [a, ax, ay] = at[i], [b, bx, by] = at[j];
        const spread = Math.min(Math.abs(bx - ax), 2 * Math.abs(by - ay));
        if (spread > most) [best, most] = [mine.has(b.id) && !mine.has(a.id) ? [b, a] : [a, b], spread];
      }
    return best;
  });
  const nameOf = (f: MapFighter) => names.get(f.id) ?? (mine.has(f.id) ? "votre personnage" : f.kind === "player" ? "ce personnage" : "ce monstre");

  function click(e: MouseEvent) {
    if (!targets) return;
    const next = [...clicks, [e.clientX * dpr, e.clientY * dpr] as [number, number]];
    if (next.length < 2) return void (clicks = next);
    const c = calibrate(targets[0].cell, next[0], targets[1].cell, next[1], width, height);
    clicks = [];
    failed = !c;
    if (!c) return;
    writeJSON(CALIBRATION_KEY, c);
    oncalibrated(c);
  }

  /** a cell's width (CSS px) guessed before the calibration: the map is 14.5 cells wide and about 10 high, the game's bars
   *  take some of the height */
  const crossW = Math.min(window.innerWidth / 15, window.innerHeight / 12);
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="calibration" onclick={click} onmousemove={(e) => (pointer = [e.clientX, e.clientY])}>
  <div class="popup">
    <b>Calibration de la vue en jeu</b>
    {#if !targets}
      <p>Il faut au moins deux combattants placés. Échap pour fermer.</p>
    {:else}
      <p>
        {#if clicks.length === 0}1/2 : cliquez au centre de la case de <b>{nameOf(targets[0])}</b>.
        {:else}2/2 : cliquez au centre de la case de <b>{nameOf(targets[1])}</b>.{/if}
      </p>
      {#if failed}<p class="bad">Ces deux clics ne collent pas à la grille : recommencez.</p>{/if}
      <p class="mut">Gardée pour cette taille de fenêtre du jeu. Échap pour annuler.</p>
    {/if}
  </div>
  {#if pointer}
    {@const [x, y] = pointer}
    <svg width="100%" height="100%" class="cross">
      <path d="M{x - crossW / 2} {y}H{x + crossW / 2}M{x} {y - crossW / 4}V{y + crossW / 4}" class="under" />
      <path d="M{x - crossW / 2} {y}H{x + crossW / 2}M{x} {y - crossW / 4}V{y + crossW / 4}" />
    </svg>
  {/if}
</div>

<style>
  .calibration { position: absolute; inset: 0; background: rgba(0, 0, 0, 0.18); cursor: none; }
  .cross path { stroke: #fff; stroke-width: 1; }
  .cross path.under { stroke: rgba(0, 0, 0, 0.8); stroke-width: 3; }
  .popup { position: absolute; left: 50%; top: 18%; transform: translateX(-50%); padding: 12px 16px; max-width: 420px; text-align: center; background: var(--panel); color: var(--fg); border: 1px solid var(--line); border-radius: 8px; box-shadow: 0 6px 24px rgba(0, 0, 0, 0.4); pointer-events: none; }
  .popup p { margin: 8px 0 0; }
  .bad { color: var(--bad); }
</style>
