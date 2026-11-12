<script lang="ts">
  import { call } from "../lib/api";
  import { occupiedCells, type Calibration, type FightMapState } from "../lib/fightMap";
  import { playerPortals, portalAuthor, type PortalSim, type PortalSims } from "../lib/classHelp";
  import PortalWay from "./PortalWay.svelte";

  /** The mouse over one of the player's open portals: the way a spell sent into it takes through the network, its damage bonus
   *  and where it lands (fight_map call `portals` from that portal, `base_only`). In the class help (PortalLayer.svelte) and,
   *  with `settings.classHelp.eliotrope.hover`, with no view for the whole fight (FightMapOverlay.svelte). */
  let { cal, dpr, live, mine, pointed }: {
    cal: Calibration;
    dpr: number;
    live: FightMapState;
    mine: Set<number>;
    /** the cell under the mouse */
    pointed: number | null;
  } = $props();

  const author = $derived(portalAuthor(live, mine));
  const hovered = $derived(playerPortals(live, mine).find((p) => p.open && p.cell === pointed)?.cell ?? null);
  /** what the way depends on */
  const inputs = $derived(JSON.stringify([live.map, live.portals, occupiedCells(live), live.fighters.find((f) => f.id === author)?.cell, hovered]));
  let way = $state.raw<PortalSim | null>(null);
  let asked = 0;
  $effect(() => {
    void inputs;
    const [who, from] = [author, hovered];
    const n = ++asked;
    if (who === null || from === null) return void (way = null);
    call<PortalSims>("fightmap", "portals", { author: who, start: from, base_only: true }, true)
      .then((r) => n === asked && (way = r.base))
      .catch(() => {});
  });
</script>

{#if way && hovered !== null}
  <svg width="100%" height="100%"><PortalWay {cal} {dpr} sim={way} /></svg>
{/if}
