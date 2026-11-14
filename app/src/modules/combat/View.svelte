<script lang="ts">
  import { onMount } from "svelte";
  import { call, listenAll } from "../../lib/api";
  import { app } from "../../lib/app.svelte";
  import { fight } from "../../lib/stores/combat.svelte";
  import { settings } from "../../lib/stores/settings.svelte";
  import { onStored, readJSON, removeKey, writeJSON } from "../../lib/persist.svelte";
  import { ALL_RANGE as RANGE, CALIBRATE_ASKED_KEY, CALIBRATION_KEY, type Calibration, type FightMapState, type PortalPlan, type Sight } from "../../lib/fightMap";
  import { PORTAL_BONUS } from "../../lib/classHelp";
  import Segmented from "../../lib/components/Segmented.svelte";
  import Banner from "../../lib/components/Banner.svelte";
  import SmoothScroll from "../../lib/components/SmoothScroll.svelte";
  import Minimap from "./Minimap.svelte";
  import GeneralOptions from "./GeneralOptions.svelte";
  import Automation from "./Automation.svelte";
  import ClassHelp from "./ClassHelp.svelte";

  let live = $state.raw<FightMapState>({ map: null, fight: false, fighters: [], portals: [] });
  /** cells of `live.map` (undefined = loading, null = not in the pack) */
  let cells = $state.raw<number[] | null | undefined>(undefined);
  let loaded: number | null = null;
  let hovered = $state<number | null>(null);
  let pinned = $state<number | null>(null);
  let sight = $state.raw<Sight | null>(null);

  /** what the map does: line of sight, or the portals planner (two portals put down by hand, the two that complete them) */
  type Mode = "los" | "portals";
  const MODE_KEY = "karrelage.combat.mapMode";
  const MODES: { id: Mode; label: string; title: string }[] = [
    { id: "los", label: "Ligne de vue", title: "Les cases en ligne de vue de la case survolée" },
    { id: "portals", label: "Portails", title: "Deux portails posés à la main : les deux à ajouter pour le plus de dommages" },
  ];
  let mode = $state<Mode>(readJSON<Mode>(MODE_KEY, "los") === "portals" ? "portals" : "los");
  $effect(() => writeJSON(MODE_KEY, mode));
  /** portals put down on the map, in order (at most two) */
  let placed = $state<number[]>([]);
  /** plans keyed by entrance cell (call `plan`, asked again when the portals or the fighters change) */
  let plans = $state.raw<Record<string, PortalPlan | null>>({});

  /** The in-game view's calibration by hand and whether one is asked (changed by its window too: followed through the storage
   *  events). The grid is otherwise computed from the game window's size. */
  let calibrated = $state(readJSON<Calibration | null>(CALIBRATION_KEY, null) !== null);
  let asked = $state(readJSON(CALIBRATE_ASKED_KEY, false));
  function calibration() {
    if (calibrated) {
      removeKey(CALIBRATION_KEY);
      calibrated = false;
    } else {
      asked = !asked;
      if (asked) writeJSON(CALIBRATE_ASKED_KEY, true);
      else removeKey(CALIBRATE_ASKED_KEY);
    }
  }

  onMount(() => {
    const off = listenAll({ "fightmap:state": (s: FightMapState) => (live = s) });
    off.ready.then(() => call<FightMapState>("fightmap", "state")).then((s) => (live = s));
    const stored = onStored<Calibration>(CALIBRATION_KEY, (c) => (calibrated = c !== null));
    const storedAsk = onStored<boolean>(CALIBRATE_ASKED_KEY, (a) => (asked = a === true));
    return () => (off(), stored(), storedAsk());
  });

  // the map's cells, asked again when the map changes (shown during a fight only)
  $effect(() => {
    const map = live.fight ? live.map : null;
    if (map === loaded) return;
    loaded = map;
    cells = undefined;
    pinned = null;
    placed = [];
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
  const from = $derived(mode === "los" ? (pinned ?? hovered) : null);

  // line of sight from the hovered / pinned cell: the player's own fighter stands there instead of where it is now
  let sightAsked = 0;
  $effect(() => {
    const f = from;
    void live.fighters; // fighters moved: asked again
    if (f === null || !cells) return void (sight = null);
    const n = ++sightAsked;
    call<Sight | null>("fightmap", "sight", { from: f, ...RANGE, ignore: [...mine] }, true).then((s) => n === sightAsked && (sight = s));
  });

  let planAsked = 0;
  $effect(() => {
    const p = placed;
    void live.fighters; // the cells they stand on are not free
    if (mode !== "portals" || p.length !== 2 || !cells) return void (plans = {});
    const n = ++planAsked;
    call<Record<string, PortalPlan | null> | null>("fightmap", "plan", { portals: p }, true).then((r) => n === planAsked && (plans = r ?? {}));
  });
  /** the plan from the hovered portal (to the other one) */
  const plan = $derived(mode === "portals" && hovered !== null && placed.includes(hovered) ? (plans[hovered] ?? null) : null);
  const noPlan = $derived(mode === "portals" && hovered !== null && placed.length === 2 && placed.includes(hovered) && hovered in plans && !plans[hovered]);

  const occupied = $derived(new Set(live.fighters.filter((f) => f.alive && !f.carried).map((f) => f.cell)));
  function pick(c: number) {
    if (mode === "los") return void (pinned = pinned === c ? null : c);
    if (placed.length < 2 && !placed.includes(c) && !occupied.has(c)) placed = [...placed, c];
  }
  const remove = (c: number) => (placed = placed.filter((p) => p !== c));
</script>

<div class="view">
  <SmoothScroll>
    <div class="combat">
      <div class="head">
        <h2>Carte</h2>
        <Segmented bind:value={mode} options={MODES} label="Mode de la carte" />
        <span class="mut status">
          {live.fight ? "Combat en cours" : "Hors combat"}{#if app.dev && live.fight && live.map !== null}&nbsp;· carte {live.map}{/if}
        </span>
        <button
          class="mini"
          onclick={calibration}
          title={calibrated
            ? "Oublier la calibration à la main : la grille de la vue en jeu est de nouveau calculée d'après la taille de la fenêtre du jeu"
            : "La grille de la vue en jeu est calculée d'après la taille de la fenêtre du jeu. Si elle est décalée : deux clics sur des combattants au prochain usage du raccourci"}
        >{calibrated ? "Effacer la calibration" : asked ? "Annuler la calibration" : "Calibrer à la main"}</button>
      </div>
      {#if mode === "portals"}
        <p class="mut help">
          Cliquez deux cases libres pour y poser deux portails (clic droit sur un portail : le retirer). Survolez-en un : les deux
          portails à ajouter (3 et 4) pour que le chemin entré par lui finisse sur l'autre avec le plus de cases parcourues, soit le
          plus de dommages ({PORTAL_BONUS} % par case).
          {#if noPlan}<b>Aucune paire de portails ne mène à l'autre portail.</b>{/if}
        </p>
      {:else}
      <p class="mut help">
        Survolez une case : les cases en ligne de vue s'éclairent, celles derrière un obstacle restent pâles. Un clic fige la
        case (un second la libère). Votre personnage est compté comme déplacé sur la case survolée.
        {#if settings.shortcuts.los.vk}Directement sur le jeu : <b>{settings.shortcuts.los.label}</b> (modifiable dans Paramètres › Raccourcis).{/if}
      </p>
      {/if}
      {#if !live.fight}
        <p class="mut">La carte du combat s'affiche dès qu'un combat commence, application ouverte.</p>
      {:else if live.map === null}
        <p class="mut">Karrelage a été lancé pendant le combat : la carte sera connue dès que votre personnage se déplacera.</p>
      {:else if live.map !== null && cells === null}
        <Banner tone="warn" inset>Carte du combat absente du pack de données : mettez-le à jour.</Banner>
      {:else if cells}
        <div class="map">
          <Minimap {cells} fighters={live.fighters} {allies} {mine} {names} sight={mode === "los" ? sight : null} {from} pinned={mode === "los" && pinned !== null} onhover={(c) => (hovered = c)} onpick={pick}
            portals={mode === "portals" ? placed : []} plan={plan} onremove={mode === "portals" ? remove : undefined} />
        </div>
        <div class="legend mut">
          <span><i class="dot me"></i>Vous</span>
          <span><i class="dot ally"></i>Alliés</span>
          <span><i class="dot enemy"></i>Ennemis</span>
          <span><i class="sq wall"></i>Obstacle</span>
          {#if mode === "los"}
            <span><i class="sq seen"></i>En vue</span>
            <span><i class="sq hidden"></i>Hors vue</span>
          {:else}
            <span><i class="dot portal"></i>Portails posés</span>
            <span><i class="dot planned"></i>À ajouter</span>
          {/if}
        </div>
      {/if}
      <Automation />
      <GeneralOptions />
      <ClassHelp />
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
  .dot.portal { background: var(--acc); width: 10px; height: 10px; }
  .dot.planned { background: color-mix(in srgb, var(--acc) 35%, var(--panel)); border: 1px dashed var(--acc); width: 10px; height: 10px; }
  .sq { width: 12px; height: 12px; display: inline-block; transform: rotate(45deg) scale(0.75); border: 1px solid var(--line); }
  .sq.wall { background: color-mix(in srgb, var(--mut) 55%, var(--bg)); }
  .sq.seen { background: color-mix(in srgb, var(--acc) 45%, var(--panel)); }
  .sq.hidden { background: color-mix(in srgb, var(--acc) 20%, var(--panel)); }
</style>
