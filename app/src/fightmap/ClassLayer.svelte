<script lang="ts">
  import { onMount } from "svelte";
  import type { Window } from "@tauri-apps/api/window";
  import { call } from "../lib/api";
  import { settings } from "../lib/stores/settings.svelte";
  import { screenCentre, screenDiamond, type Calibration, type FightMapState, type MapFighter, type Sight } from "../lib/fightMap";
  import { TURRET_RANGE, TURRETS, turretOf } from "../lib/classHelp";

  /**
   * Class help in the in-game view (FightMapOverlay.svelte), the mouse let through (only to look at). Steamer: for each of the
   * player's turrets, the cells where the Steamer gets its bonus (at most TURRET_RANGE cells away and in its line of sight:
   * the player's own fighters don't block it, they are the ones moving).
   */
  let { cal, live, mine, dpr, win, hold }: {
    cal: Calibration;
    live: FightMapState;
    mine: Set<number>;
    dpr: number;
    win: Window;
    hold: boolean;
  } = $props();

  onMount(() => {
    void win.setIgnoreCursorEvents(true);
    return () => void win.setIgnoreCursorEvents(false);
  });

  /** The player's turrets: summoned by one of their characters, else of their team when the summoner is unknown; every turret
   *  when the player is not known. */
  const turrets = $derived.by(() => {
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
  const keyLabel = $derived(settings.shortcuts.classHelp.vk ? settings.shortcuts.classHelp.label : "le raccourci");
</script>

<svg width="100%" height="100%">
  {#each areas as a (a.turret.id)}
    {#each a.seen as c (c)}
      <polygon points={screenDiamond(cal, c, dpr)} style:fill={look(a.turret).colour} style:stroke={look(a.turret).colour} class="area" />
    {/each}
  {/each}
  {#each areas as a (a.turret.id)}
    {@const [x, y] = screenCentre(cal, a.turret.cell, dpr)}
    <polygon points={screenDiamond(cal, a.turret.cell, dpr)} style:stroke={look(a.turret).colour} class="turret" />
    <text {x} y={y - (cal.sy * 0.25) / dpr - 4} class="name">{look(a.turret).name}</text>
  {/each}
</svg>
<div class="pill">
  {#if areas.length}
    Tourelles ({TURRET_RANGE} PO, en ligne de vue) :
    {#each areas as a (a.turret.id)}<span class="chip" style:color={look(a.turret).colour}>■ {look(a.turret).name}</span>{/each}
  {:else}Aucune aide pour ce combat (Steamer : aucune tourelle posée){/if}
  · {hold ? `lâchez ${keyLabel}` : `${keyLabel} ou Échap`} pour fermer
</div>

<style>
  .area { fill-opacity: 0.22; stroke-opacity: 0.8; stroke-width: 1; }
  .turret { fill: none; stroke-width: 3; }
  .name { fill: #fff; font-size: 12px; font-weight: 600; text-anchor: middle; paint-order: stroke; stroke: rgba(0, 0, 0, 0.85); stroke-width: 3px; }
  .chip { margin-left: 8px; font-weight: 600; }
</style>
