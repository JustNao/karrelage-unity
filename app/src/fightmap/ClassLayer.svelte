<script lang="ts">
  import { untrack } from "svelte";
  import { call } from "../lib/api";
  import { settings } from "../lib/stores/settings.svelte";
  import { occupiedCells, screenDiamond, type Calibration, type FightMapState, type MapFighter, type Sight } from "../lib/fightMap";
  import { PORTAL_BONUS, playerPortals, TURRET_RANGE, TURRETS, turretOf } from "../lib/classHelp";
  import PortalLayer from "./PortalLayer.svelte";

  /**
   * Class help in the in-game view (FightMapOverlay.svelte), the mouse let through (only to look at). Steamer: for each of the
   * player's turrets, the cells where the Steamer gets its bonus (at most TURRET_RANGE cells away and in its line of sight:
   * the player's own fighters don't block it, they are the ones moving). The mouse over a turret's cell (`pointed`) brings its
   * zone forward. Eliotrope: the portals (fightmap/PortalLayer.svelte). Each class can be turned off in the Combat tab
   * (`settings.classHelp`).
   */
  let { cal, live, mine, dpr, hold, pointed }: {
    cal: Calibration;
    live: FightMapState;
    mine: Set<number>;
    dpr: number;
    hold: boolean;
    /** the cell under the mouse */
    pointed: number | null;
  } = $props();

  /** The player's turrets: summoned by one of their characters, else of their team when the summoner is unknown; every turret
   *  when the player is not known. */
  const turrets = $derived.by(() => {
    if (!settings.classHelp.steamer.enabled) return [];
    const all = live.fighters.filter((f) => f.alive && f.cell >= 0 && turretOf(f.monsters) !== undefined);
    if (!mine.size) return all;
    const teams = new Set(live.fighters.filter((f) => mine.has(f.id)).map((f) => f.team));
    return all.filter((f) => mine.has(f.summoner) || (!f.summoner && teams.has(f.team)));
  });

  /** what the zones depend on */
  const inputs = $derived(JSON.stringify([live.map, turrets.map((t) => [t.id, t.cell]), occupiedCells(live), [...mine]]));
  let areas = $state.raw<{ turret: MapFighter; seen: number[] }[]>([]);
  let asked = 0;
  $effect(() => {
    void inputs;
    const [list, ignore] = untrack(() => [turrets, [...mine]] as const);
    const n = ++asked;
    Promise.all(list.map((t) => call<Sight | null>("fightmap", "sight", { from: t.cell, min: 1, max: TURRET_RANGE, ignore }, true)))
      .then((s) => n === asked && (areas = list.map((turret, i) => ({ turret, seen: s[i]?.seen ?? [] }))))
      .catch(() => {});
  });

  const look = (t: MapFighter) => TURRETS[turretOf(t.monsters)!];
  /** the turret under the mouse: its zone forward, the others faded */
  const focused = $derived(turrets.find((t) => t.cell === pointed)?.id ?? null);
  const key = $derived(settings.shortcuts.classHelp);
  const keyLabel = $derived(key.vk ? key.label : "le raccourci");
  const portals = $derived(settings.classHelp.eliotrope.enabled ? playerPortals(live, mine) : []);
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
<PortalLayer {cal} {dpr} {portals} />
{#snippet zone(a: { turret: MapFighter; seen: number[] })}
  {#each a.seen as c (c)}
    <polygon points={screenDiamond(cal, c, dpr)} style:fill={look(a.turret).colour} style:stroke={look(a.turret).colour}
      class="area" class:focused={a.turret.id === focused} class:faded={focused !== null && a.turret.id !== focused} />
  {/each}
{/snippet}

<div class="pill">
  {#if areas.length}
    Tourelles ({TURRET_RANGE} PO, en ligne de vue) :
    {#each areas as a (a.turret.id)}<span class="chip" style:color={look(a.turret).colour}>■ {look(a.turret).name}</span>{/each}
    {#if focused === null}· survolez une tourelle pour sa zone{/if}
  {:else if portals.length}
    Portails : {portals.length} (+{PORTAL_BONUS} % de dommages par case parcourue)
    {#if portals.length >= 4}<span class="chip" style:color="#ff4d4d">■ 1 part au prochain</span><span class="chip" style:color="#ffa53c">■ 2 ensuite</span>{/if}
    {#if settings.classHelp.eliotrope.portalKey.vk}· {settings.classHelp.eliotrope.portalKey.label} pour simuler un portail{/if}
  {:else}Aucune aide pour ce combat (Steamer : aucune tourelle posée, Éliotrope : aucun portail){/if}
  · {hold ? `lâchez ${keyLabel}` : `${keyLabel} ou Échap`} pour fermer
</div>

<style>
  .area { fill-opacity: 0.22; stroke-opacity: 0.8; stroke-width: 1; }
  .turret { fill: none; stroke-width: 3; }
  .area.focused { fill-opacity: 0.45; stroke-opacity: 1; stroke-width: 1.5; }
  .area.faded { fill-opacity: 0.06; stroke-opacity: 0.2; }
</style>
