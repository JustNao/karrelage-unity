<script lang="ts">
  import { onMount } from "svelte";
  import { cursorPosition, type Window } from "@tauri-apps/api/window";
  import { call } from "../lib/api";
  import { settings } from "../lib/stores/settings.svelte";
  import { cellAt, screenDiamond, type Calibration, type FightMapState, type MapFighter, type Sight } from "../lib/fightMap";
  import { PORTAL_BONUS, TURRET_RANGE, TURRETS, turretOf, type PortalNet } from "../lib/classHelp";
  import PortalLayer from "./PortalLayer.svelte";

  /**
   * Class help in the in-game view (FightMapOverlay.svelte), the mouse let through (only to look at). Steamer: for each of the
   * player's turrets, the cells where the Steamer gets its bonus (at most TURRET_RANGE cells away and in its line of sight:
   * the player's own fighters don't block it, they are the ones moving). The mouse over a turret's cell brings its zone forward
   * (the cursor is polled: the window lets the mouse through). Eliotrope: the portals (fightmap/PortalLayer.svelte), with a
   * portal simulated under the mouse when `simulate` (view `portals`, the turrets left out). Each class can be turned off in
   * the Combat tab (`settings.classHelp`).
   */
  let { cal, live, mine, dpr, win, hold, simulate }: {
    cal: Calibration;
    live: FightMapState;
    mine: Set<number>;
    dpr: number;
    win: Window;
    hold: boolean;
    simulate: boolean;
  } = $props();

  let net = $state.raw<PortalNet | null>(null);

  /** the cell under the mouse */
  let pointed = $state<number | null>(null);

  onMount(() => {
    void win.setIgnoreCursorEvents(true);
    let origin: { x: number; y: number } | null = null;
    void win.innerPosition().then((p) => (origin = p));
    const poll = setInterval(async () => {
      if (!origin) return;
      const p = await cursorPosition().catch(() => null);
      pointed = p && cellAt(cal, p.x - origin.x, p.y - origin.y);
    }, 60);
    return () => {
      clearInterval(poll);
      void win.setIgnoreCursorEvents(false);
    };
  });

  /** The player's turrets: summoned by one of their characters, else of their team when the summoner is unknown; every turret
   *  when the player is not known. */
  const turrets = $derived.by(() => {
    if (simulate || !settings.classHelp.steamer.enabled) return [];
    const all = live.fighters.filter((f) => f.alive && f.cell >= 0 && turretOf(f.monsters) !== undefined);
    if (!mine.size) return all;
    const teams = new Set(live.fighters.filter((f) => mine.has(f.id)).map((f) => f.team));
    return all.filter((f) => mine.has(f.summoner) || (!f.summoner && teams.has(f.team)));
  });

  let areas = $state.raw<{ turret: MapFighter; seen: number[] }[]>([]);
  let asked = 0;
  $effect(() => {
    const list = turrets;
    const n = ++asked;
    const ignore = [...mine];
    Promise.all(list.map((t) => call<Sight | null>("fightmap", "sight", { from: t.cell, min: 1, max: TURRET_RANGE, ignore }, true)))
      .then((s) => n === asked && (areas = list.map((turret, i) => ({ turret, seen: s[i]?.seen ?? [] }))))
      .catch(() => {});
  });

  const look = (t: MapFighter) => TURRETS[turretOf(t.monsters)!];
  /** the turret under the mouse: its zone forward, the others faded */
  const focused = $derived(turrets.find((t) => t.cell === pointed)?.id ?? null);
  const key = $derived(simulate ? settings.classHelp.eliotrope.portalKey : settings.shortcuts.classHelp);
  const keyLabel = $derived(key.vk ? key.label : "le raccourci");
  const portals = $derived(net?.portals.length ?? 0);
</script>

<svg width="100%" height="100%">
  {#each areas as a (a.turret.id)}
    {#if a.turret.id !== focused}{@render zone(a)}{/if}
  {/each}
  {#each areas as a (a.turret.id)}
    {#if a.turret.id === focused}{@render zone(a)}{/if}
    <polygon points={screenDiamond(cal, a.turret.cell, dpr)} style:stroke={look(a.turret).colour} class="turret" />
  {/each}
</svg>
{#if settings.classHelp.eliotrope.enabled}<PortalLayer {cal} {live} {mine} {dpr} {pointed} {simulate} bind:net />{/if}
{#snippet zone(a: { turret: MapFighter; seen: number[] })}
  {#each a.seen as c (c)}
    <polygon points={screenDiamond(cal, c, dpr)} style:fill={look(a.turret).colour} style:stroke={look(a.turret).colour}
      class="area" class:focused={a.turret.id === focused} class:faded={focused !== null && a.turret.id !== focused} />
  {/each}
{/snippet}

<div class="pill">
  {#if simulate}
    Portail simulé sous la souris
    {#if net && net.path.length > 1}· sortie à {net.distance} cases : <b>+{net.distance * PORTAL_BONUS} % de dommages</b>{/if}
    {#if net?.dropped != null}· <span class="chip" style:color="#ff4d4d">✕ le n° 1 disparaît</span>
    {:else if !net?.added && !net?.path.length}· survolez une case libre ou un portail{/if}
  {:else if areas.length}
    Tourelles ({TURRET_RANGE} PO, en ligne de vue) :
    {#each areas as a (a.turret.id)}<span class="chip" style:color={look(a.turret).colour}>■ {look(a.turret).name}</span>{/each}
  {:else if portals}
    Portails : {portals} (+{PORTAL_BONUS} % de dommages par case parcourue)
    {#if portals >= 4}<span class="chip" style:color="#ff4d4d">■ 1 part au prochain</span><span class="chip" style:color="#ffa53c">■ 2 ensuite</span>{/if}
    {#if settings.classHelp.eliotrope.portalKey.vk}· {settings.classHelp.eliotrope.portalKey.label} pour simuler un portail{/if}
  {:else}Aucune aide pour ce combat (Steamer : aucune tourelle posée, Éliotrope : aucun portail){/if}
  {#if areas.length && focused === null}· survolez une tourelle pour sa zone{/if}
  · {hold ? `lâchez ${keyLabel}` : simulate ? `${keyLabel}, un clic ou Échap` : `${keyLabel} ou Échap`} pour fermer
</div>

<style>
  .area { fill-opacity: 0.22; stroke-opacity: 0.8; stroke-width: 1; }
  .turret { fill: none; stroke-width: 3; }
  .area.focused { fill-opacity: 0.45; stroke-opacity: 1; stroke-width: 1.5; }
  .area.faded { fill-opacity: 0.06; stroke-opacity: 0.2; }
  .chip { margin-left: 8px; font-weight: 600; }
</style>
