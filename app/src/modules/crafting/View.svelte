<script lang="ts">
  import Grip from "../../lib/components/Grip.svelte";
  import SideList from "../../lib/components/SideList.svelte";
  import { Columns } from "../../lib/columns.svelte";
  import { subs } from "./subs";

  const cols = new Columns("karrelage.cols.crafting", { list: 220 });
  let selId = $state(subs[0]?.id);
  const sel = $derived(subs.find((s) => s.id === selId) ?? subs[0]);
</script>

<div class="view cgrid" class:anim={!cols.dragging} style:grid-template-columns="{cols.px("list")}px 5px 1fr">
  <SideList {cols} name="list" title="Crafting">
    {#each subs as s, i (s.id)}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="irow" class:first={i === 0} class:sel={sel?.id === s.id} onclick={() => (selId = s.id)}>
        <b>{s.title}</b><br /><span class="mut">{s.description}</span>
      </div>
    {/each}
  </SideList>
  <Grip {cols} name="list" dir={1} />
  <div class="body">
    {#if sel}<sel.component />{/if}
  </div>
</div>

<style>
  .view { flex: 1; min-height: 0; display: grid; font-family: var(--font); }
  .body { min-width: 0; min-height: 0; display: flex; flex-direction: column; }
</style>
