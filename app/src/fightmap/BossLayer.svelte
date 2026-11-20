<script lang="ts">
  import { guerreWeapon, spellTurn } from "../lib/bosses";
  import type { Calibration, FightMapState } from "../lib/fightMap";
  import { fight } from "../lib/stores/combat.svelte";
  import { settings } from "../lib/stores/settings.svelte";
  import GuerreLayer from "./GuerreLayer.svelte";
  import SpellCellLayer from "./SpellCellLayer.svelte";

  /**
   * Boss help in the in-game view: each boss's helper that applies to this fight (Comte Harebourg: where a spell lands,
   * fightmap/SpellCellLayer.svelte; Guerre: her next weapon, fightmap/GuerreLayer.svelte), each turned on in the Combat tab
   * « Général ». Drawn by the shortcut `bossHelp` (`pill`: says when none applies), else by the class help.
   */
  let { cal, live, cells, dpr, pointed, pill = false }: {
    cal: Calibration;
    live: FightMapState;
    cells: number[] | null;
    dpr: number;
    pointed: number | null;
    pill?: boolean;
  } = $props();

  const harebourg = $derived(settings.combat.harebourgCell && spellTurn(fight) !== null);
  const guerre = $derived(settings.combat.guerreWeapon && guerreWeapon(live, cells) !== null);
</script>

{#if harebourg}<SpellCellLayer {cal} {live} {dpr} {pointed} />{/if}
{#if guerre}<GuerreLayer {cal} {live} {cells} {dpr} />{/if}
{#if pill && !harebourg && !guerre}<div class="pill">Aucune aide de boss pour ce combat (Comte Harebourg, Guerre)</div>{/if}
