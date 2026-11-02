<script lang="ts">
  import { onMount } from "svelte";
  import { call, listenAll } from "../../lib/api";
  import { app } from "../../lib/app.svelte";
  import { fight } from "../../lib/stores/combat.svelte";
  import { settings } from "../../lib/stores/settings.svelte";
  import { onStored, readJSON, removeKey } from "../../lib/persist.svelte";
  import { ALL_RANGE as RANGE, CALIBRATION_KEY, type Calibration, type FightMapState, type Sight } from "../../lib/fightMap";
  import Banner from "../../lib/components/Banner.svelte";
  import SmoothScroll from "../../lib/components/SmoothScroll.svelte";
  import Minimap from "./Minimap.svelte";

  let live = $state.raw<FightMapState>({ map: null, fight: false, fighters: [] });
  /** cells of `live.map` (undefined = loading, null = not in the pack) */
  let cells = $state.raw<number[] | null | undefined>(undefined);
  let loaded: number | null = null;
  let hovered = $state<number | null>(null);
  let pinned = $state<number | null>(null);
  let sight = $state.raw<Sight | null>(null);

  /** The in-game view's calibration (saved by its window: followed through the storage events) */
  let calibrated = $state(readJSON<Calibration | null>(CALIBRATION_KEY, null) !== null);
  function forget() {
    removeKey(CALIBRATION_KEY);
    calibrated = false;
  }

  onMount(() => {
    const off = listenAll({ "fightmap:state": (s: FightMapState) => (live = s) });
    off.ready.then(() => call<FightMapState>("fightmap", "state")).then((s) => (live = s));
    const stored = onStored<Calibration>(CALIBRATION_KEY, (c) => (calibrated = c !== null));
    return () => (off(), stored());
  });

  // the map's cells, asked again when the map changes (shown during a fight only)
  $effect(() => {
    const map = live.fight ? live.map : null;
    if (map === loaded) return;
    loaded = map;
    cells = undefined;
    pinned = null;
    if (map === null) return;
    call<number[] | null>("fightmap", "cells", { map }).then((c) => map === loaded && (cells = c));
  });

  const mine = $derived(new Set(fight.mine ?? []));
  /** the player's team: allies are drawn green, the others red */
  const allies = $derived(live.fighters.find((f) => mine.has(f.id))?.team ?? null);
  const names = $derived.by(() => {
    const m = new Map<number, string>();
    for (const f of fight.fighters) m.set(f.actor, f.name);
    for (const c of fight.characters ?? []) if (c.name) m.set(c.id, c.name);
    return m;
  });
  const from = $derived(pinned ?? hovered);

  // line of sight from the hovered / pinned cell: the player's own fighter stands there instead of where it is now
  let asked = 0;
  $effect(() => {
    const f = from;
    void live.fighters; // fighters moved: asked again
    if (f === null || !cells) return void (sight = null);
    const n = ++asked;
    call<Sight | null>("fightmap", "sight", { from: f, ...RANGE, ignore: [...mine] }, true).then((s) => n === asked && (sight = s));
  });

  const pick = (c: number) => (pinned = pinned === c ? null : c);
</script>

<div class="view">
  <SmoothScroll>
    <div class="combat">
      <div class="head">
        <h2>Ligne de vue</h2>
        <span class="mut status">
          {live.fight ? "Combat en cours" : "Hors combat"}{#if app.dev && live.fight && live.map !== null}&nbsp;· carte {live.map}{/if}
        </span>
        <button
          class="mini"
          disabled={!calibrated}
          onclick={forget}
          title={calibrated ? "Oublier la calibration de la vue en jeu : il sera redemandé au prochain usage du raccourci" : "Aucune calibration enregistrée"}
        >Effacer la calibration</button>
      </div>
      <p class="mut help">
        Survolez une case : les cases en ligne de vue s'éclairent, celles derrière un obstacle restent pâles. Un clic fige la
        case (un second la libère). Votre personnage est compté comme déplacé sur la case survolée.
        {#if settings.shortcuts.los.vk}Directement sur le jeu : <b>{settings.shortcuts.los.label}</b> (modifiable dans Paramètres › Raccourcis).{/if}
      </p>
      {#if !live.fight}
        <p class="mut">La carte du combat s'affiche dès qu'un combat commence, application ouverte.</p>
      {:else if live.map !== null && cells === null}
        <Banner tone="warn" inset>Carte du combat absente du pack de données : mettez-le à jour.</Banner>
      {:else if cells}
        <div class="map">
          <Minimap {cells} fighters={live.fighters} {allies} {mine} {names} {sight} {from} pinned={pinned !== null} onhover={(c) => (hovered = c)} onpick={pick} />
        </div>
        <div class="legend mut">
          <span><i class="dot me"></i>Vous</span>
          <span><i class="dot ally"></i>Alliés</span>
          <span><i class="dot enemy"></i>Ennemis</span>
          <span><i class="sq wall"></i>Obstacle</span>
          <span><i class="sq seen"></i>En vue</span>
          <span><i class="sq hidden"></i>Hors vue</span>
        </div>
      {/if}
    </div>
  </SmoothScroll>
</div>

<style>
  .view { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .view > :global(.wrap) { flex: 1; }
  .combat { max-width: 980px; font-family: var(--font); font-size: 13px; }
  .head { display: flex; flex-wrap: wrap; align-items: center; gap: 8px 16px; padding-bottom: 4px; border-bottom: 1px solid var(--line); }
  h2 { font: 600 12px var(--font); letter-spacing: 0.08em; text-transform: uppercase; color: var(--mut); margin: 0; }
  .status { margin-left: auto; }
  .help { font-size: 12px; margin: 8px 0 12px; }
  .map { padding: 4px 0; }
  .legend { display: flex; flex-wrap: wrap; gap: 6px 16px; font-size: 12px; margin-top: 8px; }
  .legend span { display: inline-flex; align-items: center; gap: 6px; }
  .dot { width: 12px; height: 8px; border-radius: 50%; display: inline-block; }
  .dot.me { background: var(--gold); }
  .dot.ally { background: var(--ok); }
  .dot.enemy { background: var(--bad); }
  .sq { width: 12px; height: 12px; display: inline-block; transform: rotate(45deg) scale(0.75); border: 1px solid var(--line); }
  .sq.wall { background: color-mix(in srgb, var(--mut) 55%, var(--bg)); }
  .sq.seen { background: color-mix(in srgb, var(--acc) 45%, var(--panel)); }
  .sq.hidden { background: color-mix(in srgb, var(--acc) 20%, var(--panel)); }
</style>
