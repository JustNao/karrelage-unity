<script lang="ts" module>
  /** Where the popup of a slot opens (fixed position, so the scroll area does not clip it). */
  export interface PopAt { slot: string; left: number; top?: number; bottom?: number; maxH: number }
  export const POP_W = 330;
</script>

<script lang="ts">
  import { clickOutside } from "../../lib/actions/clickOutside";
  import { inventory, type InvItem } from "../../lib/stores/inventory.svelte";
  import { SLOT_TITLES, type Build } from "../../lib/stuff";
  import ItemCard from "../../lib/components/ItemCard.svelte";

  /** Popup of a slot (click on its item): counted at its perfect roll, at one of the character's copies, or to buy. */
  let {
    pop = $bindable(),
    build,
    owned,
    mine,
    onset,
  }: {
    pop: PopAt | null;
    build: Build;
    owned: Set<string>;
    /** the character's copy the slot counts at, if any */
    mine: InvItem | undefined;
    /** owned at copy `uid`, at its perfect roll ("perfect"), or to buy (null) */
    onset: (slot: string, how: number | "perfect" | null) => void;
  } = $props();

  const set = (how: number | "perfect" | null) => {
    onset(pop!.slot, how);
    pop = null;
  };
</script>

<svelte:window onresize={() => (pop = null)} />
{#if pop && build.slots[pop.slot]}
  {@const id = pop.slot}
  {@const sl = build.slots[id]}
  <div class="pop" use:clickOutside={{ close: () => (pop = null), ignore: ".slot .ic" }} style:left="{pop.left}px" style:top={pop.top != null ? `${pop.top}px` : null} style:bottom={pop.bottom != null ? `${pop.bottom}px` : null} style:max-height="{pop.maxH}px" style:width="{POP_W}px">
    <div class="phd"><b>{sl.name}</b> <span class="mut">{SLOT_TITLES[id]}</span></div>
    <button class="opt" class:on={owned.has(id) && !mine} onclick={() => set("perfect")}>
      Jet parfait <span class="mut">— je l'ai déjà, compté au maximum</span>
    </button>
    {#each inventory.ofGid(sl.gid) as it (it.uid)}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="opt copy" class:on={mine?.uid === it.uid} role="button" tabindex="0" onclick={() => set(it.uid)}>
        <div>Mon objet <span class="mut">— {it.equipped ? "équipé" : "dans le sac"}</span></div>
        <ItemCard row={it} flags={false} priced={false} />
      </div>
    {:else}
      <div class="mut small">{inventory.updated ? "Le personnage n'a pas cet objet (équipé ou dans le sac)." : "Inventaire inconnu : connectez votre personnage pendant que Karrelage tourne."}</div>
    {/each}
    <button class="opt" class:on={!owned.has(id)} onclick={() => set(null)}>
      À acheter <span class="mut">— je ne l'ai pas</span>
    </button>
  </div>
{/if}

<style>
  .pop { position: fixed; z-index: 50; overflow-y: auto; box-sizing: border-box; padding: 8px; display: flex; flex-direction: column; gap: 6px; background: var(--panel); border: 1px solid var(--line); border-radius: 8px; box-shadow: 0 6px 24px rgba(0, 0, 0, 0.45); font-size: 12px; font-family: var(--font); }
  .phd { padding: 0 2px 2px; }
  .opt { text-align: left; padding: 5px 8px; font: inherit; border: 1px solid var(--line); border-radius: 6px; background: none; cursor: pointer; }
  .opt:hover { border-color: var(--mut); }
  .opt.on { border-color: #4aa3ff; }
  .copy { display: flex; flex-direction: column; gap: 4px; }
  .small { font-size: 12px; margin: 4px 0; }
</style>
