<script lang="ts">
  import { onMount } from "svelte";
  import type { Window } from "@tauri-apps/api/window";
  import { call } from "../lib/api";
  import { settings } from "../lib/stores/settings.svelte";
  import { ALL_RANGE, cellAt, MOV, screenDiamond, type Calibration, type FightMapState, type Sight } from "../lib/fightMap";

  /**
   * Line of sight in the in-game view (FightMapOverlay.svelte): the window takes the mouse, the cells in line of sight of the
   * hovered cell are drawn (the player's character counted as standing there). A click pins the cell (`sticky`: the view stays
   * once the shortcut is let go): the window lets the mouse through, the player plays, the view follows the fighters' moves.
   */
  let { cal, cells, live, mine, dpr, win, hold, sticky = $bindable(false) }: {
    cal: Calibration;
    cells: number[];
    live: FightMapState;
    mine: Set<number>;
    dpr: number;
    win: Window;
    hold: boolean;
    sticky?: boolean;
  } = $props();

  let hovered = $state<number | null>(null);
  let pinned = $state<number | null>(null);
  let sight = $state.raw<Sight | null>(null);

  onMount(() => void win.setIgnoreCursorEvents(false));

  function click() {
    if (pinned !== null || hovered === null) return;
    pinned = hovered;
    sticky = true;
    void win.setIgnoreCursorEvents(true);
  }

  function move(e: MouseEvent) {
    if (pinned !== null) return;
    const c = cellAt(cal, e.clientX * dpr, e.clientY * dpr);
    hovered = c !== null && cells[c] & MOV ? c : null;
  }

  // line of sight from the hovered / pinned cell, asked again when fighters move
  let asked = 0;
  $effect(() => {
    const from = pinned ?? hovered;
    void live.fighters;
    if (from === null) return void (sight = null);
    const n = ++asked;
    call<Sight | null>("fightmap", "sight", { from, ...ALL_RANGE, ignore: [...mine] }, true).then((s) => n === asked && (sight = s));
  });

  const from = $derived(pinned ?? hovered);
  const keyLabel = $derived(settings.shortcuts.los.vk ? settings.shortcuts.los.label : "le raccourci");
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="catch" onclick={click} onmousemove={move}></div>
<svg width="100%" height="100%">
  {#each sight?.seen ?? [] as c (c)}
    <polygon points={screenDiamond(cal, c, dpr)} class="seen" />
  {/each}
  {#if from !== null}
    <polygon points={screenDiamond(cal, from, dpr)} class="from" />
  {/if}
</svg>
<div class="pill">
  {#if pinned === null}Ligne de vue : survolez une case, clic pour la figer · {hold ? `lâchez ${keyLabel} pour fermer` : "Échap pour fermer"}
  {:else}Ligne de vue figée · {keyLabel}, Échap ou la fin du tour l'effacent{/if}
</div>

<style>
  .catch { position: absolute; inset: 0; }
  .seen { fill: rgba(90, 169, 255, 0.35); stroke: rgba(90, 169, 255, 0.9); stroke-width: 1; }
  .from { fill: rgba(242, 194, 48, 0.45); stroke: #f2c230; stroke-width: 2; }
</style>
