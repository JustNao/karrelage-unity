<script lang="ts">
  import { call } from "../lib/api";
  import { spellTurn } from "../lib/bosses";
  import { screenDiamond, type Calibration, type FightMapState } from "../lib/fightMap";
  import { fight } from "../lib/stores/combat.svelte";
  import { settings } from "../lib/stores/settings.svelte";

  /**
   * Comte Harebourg in the in-game view: in blue, the cell a spell aimed at the cell under the mouse lands on (turned around the
   * caster by their confusion: crates/data/src/maps.rs `turned`). Drawn by the class help (ClassLayer.svelte), or alone with the
   * shortcut `spellCell` when one is set; it stays once the Comte is dead, until his wave is over (`spellTurn`). Every cell's landing comes at once (asked again when the caster moves or the confusion
   * changes): hovering is a lookup.
   */
  let { cal, live, dpr, pointed }: {
    cal: Calibration;
    live: FightMapState;
    dpr: number;
    pointed: number | null;
  } = $props();

  const turn = $derived(settings.combat.harebourgCell ? spellTurn(fight) : null);
  const around = $derived(turn && live.fighters.find((f) => f.id === turn.caster && f.alive && f.cell >= 0)?.cell);
  const quarters = $derived(turn?.quarters);

  let landings = $state.raw<(number | null)[] | null>(null);
  let asked = 0;
  $effect(() => {
    const [cell, q] = [around, quarters];
    const n = ++asked;
    landings = null;
    if (cell == null || q == null) return;
    call<(number | null)[] | null>("fightmap", "turned", { around: cell, quarters: q }, true)
      .then((r) => n === asked && (landings = r))
      .catch(() => {});
  });

  const target = $derived(pointed !== null && landings ? landings[pointed] : null);
</script>

{#if target != null}
  <svg width="100%" height="100%">
    <polygon points={screenDiamond(cal, target, dpr)} class="target" />
  </svg>
{/if}

<style>
  .target { fill: rgba(60, 140, 255, 0.5); stroke: #3c8cff; stroke-width: 2; }
</style>
