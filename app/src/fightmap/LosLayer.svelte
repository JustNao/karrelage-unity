<script lang="ts">
  import { untrack } from "svelte";
  import { call } from "../lib/api";
  import { settings } from "../lib/stores/settings.svelte";
  import { ALL_RANGE, MOV, screenDiamond, type Calibration, type FightMapState, type Sight } from "../lib/fightMap";

  /**
   * Line of sight in the in-game view (FightMapOverlay.svelte): the window takes the mouse, the cells in line of sight of the
   * hovered cell are drawn (the player's character counted as standing there). A click pins the cell (`sticky`: the view stays
   * once the shortcut is let go, and the shell lets the mouse through): the player plays, the view follows the fighters' moves.
   */
  let { cal, cells, live, mine, dpr, hold, pointed, clicks, sticky = $bindable(false) }: {
    cal: Calibration;
    cells: number[];
    live: FightMapState;
    mine: Set<number>;
    dpr: number;
    hold: boolean;
    /** the cell under the mouse (polled by the shell: no mouse event comes while a mouse button shortcut is held, the game
     *  captures the mouse) */
    pointed: number | null;
    /** left clicks seen by the shell's click watcher: each one pins, like a click on the view */
    clicks: number;
    sticky?: boolean;
  } = $props();

  const hovered = $derived(pointed !== null && cells[pointed] & MOV ? pointed : null);
  let pinned = $state<number | null>(null);
  let sight = $state.raw<Sight | null>(null);

  function click() {
    if (pinned !== null || hovered === null) return;
    pinned = hovered;
    sticky = true;
  }

  // a polled left click pins too (the view's own click event doesn't come while the game captures the mouse)
  const clicksAtOpen = untrack(() => clicks);
  $effect(() => {
    if (clicks !== clicksAtOpen) untrack(click);
  });

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
<div class="catch" onclick={click}></div>
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
