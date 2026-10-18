<script lang="ts">
  import { Columns } from "../../lib/columns.svelte";
  import { subs } from "./subs";

  const cols = new Columns("karrelage.cols.crafting", { list: 220 });
  let selId = $state(subs[0]?.id);
  const sel = $derived(subs.find((s) => s.id === selId) ?? subs[0]);
</script>

<div class="view" style:grid-template-columns="{cols.w.list}px 5px 1fr">
  <div class="ilist">
    {#each subs as s (s.id)}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="irow" class:sel={sel?.id === s.id} onclick={() => (selId = s.id)}>
        <b>{s.title}</b><br /><span class="mut">{s.description}</span>
      </div>
    {/each}
  </div>
  <div class="grip" role="separator" aria-orientation="vertical" onpointerdown={(e) => cols.drag("list", 1, e)}></div>
  <div class="body">
    {#if sel}<sel.component />{/if}
  </div>
</div>

<style>
  .view { flex: 1; min-height: 0; display: grid; font-family: var(--font); }
  .ilist { overflow: auto; background: var(--panel); border-right: 1px solid var(--line); }
  .ilist div { padding: 6px 10px; border-bottom: 1px solid var(--line); cursor: pointer; }
  .ilist .irow:hover { background: var(--bg); }
  .ilist div.sel { outline: 1px solid var(--acc); }
  .body { min-width: 0; min-height: 0; display: flex; flex-direction: column; }
</style>
