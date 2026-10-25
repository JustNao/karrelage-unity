<script lang="ts">
  import { SvelteSet } from "svelte/reactivity";
  import { call } from "../../lib/api";
  import type { Columns } from "../../lib/columns.svelte";
  import { ago, kamas } from "../../lib/fmt";
  import { readJSON, writeJSON } from "../../lib/persist.svelte";
  import { auction } from "../../lib/stores/auction.svelte";
  import { minute } from "../../lib/stores/clock.svelte";
  import Collapsible from "../../lib/components/Collapsible.svelte";
  import ConfirmButton from "../../lib/components/ConfirmButton.svelte";
  import ItemIcon from "../../lib/components/ItemIcon.svelte";
  import SearchBar, { matcher } from "../../lib/components/SearchBar.svelte";
  import SideList from "../../lib/components/SideList.svelte";

  /** HDV left panel: every scanned item, grouped by type (groups collapsible, remembered), latest scan first; the green dot
   *  = lots well above the price trend (`dealCount`). */
  let { cols, sel = $bindable(), dealCount }: { cols: Columns<"list" | "filter">; sel: number | null; dealCount: (item: any) => number } = $props();

  const COLLAPSE_KEY = "karrelage.collapsedTypes";
  const collapsed = new SvelteSet<string>(readJSON<string[]>(COLLAPSE_KEY, []));
  function toggleType(t: string) {
    collapsed.has(t) ? collapsed.delete(t) : collapsed.add(t);
    writeJSON(COLLAPSE_KEY, [...collapsed]);
  }
  let search = $state(""); // filter by item or type name
  const typeGroups = $derived.by(() => {
    const m = new Map<string, any[]>();
    const match = matcher(search);
    for (const i of [...auction.items.values()].sort((a, b) => b.updated - a.updated)) {
      if (!match(i.name, i.type)) continue;
      const t = i.type ?? "?";
      if (!m.has(t)) m.set(t, []);
      m.get(t)!.push(i);
    }
    return [...m.entries()].sort((a, b) => a[0].localeCompare(b[0], "fr")).map(([type, items]) => ({ type, items }));
  });
</script>

<SideList {cols} name="list" title="Objets">
  {#snippet bar()}
    <SearchBar bind:value={search} placeholder="Rechercher un objet…" grow compact>
      {#snippet after()}<ConfirmButton mini label="Nettoyer" title="Oublier tous les objets enregistrés" onconfirm={() => call("auction", "clear")} />{/snippet}
    </SearchBar>
  {/snippet}
  {#each typeGroups as g (g.type)}
    <Collapsible head="thead" title={g.type} count={g.items.length} open={!collapsed.has(g.type)} ontoggle={() => toggleType(g.type)}>
      {#each g.items as i (i.gid)}
        {@const dc = dealCount(i)}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <div class="irow ii" class:sel={sel === i.gid} onclick={() => (sel = i.gid)}>
          <ItemIcon icon={i.icon} class="iic" />
          <div class="itx">
            <b>{i.name}</b> <span class="mut">niv. {i.level ?? "?"}</span>
            {#if dc}<span class="deal-ico" title="{dc} {dc === 1 ? 'bonne affaire' : 'bonnes affaires'} dans cette liste">●</span>{/if}<br />
            <span class="mut">{i.lots} lots · {i.min_price ? "min " + kamas(i.min_price) : "—"} · {ago(i.updated, minute.now)}{i.demo ? " · demo" : ""}</span>
          </div>
        </div>
      {/each}
    </Collapsible>
  {:else}
    <div class="mut empty">{search ? "Aucun objet ne correspond à la recherche." : "Ouvrez un objet à l'hôtel de vente : il apparaît ici (dernière liste par objet)."}</div>
  {/each}
</SideList>

<style>
  /* item icon left of a list row, as tall as the row (fetched once on screen) */
  .irow.ii { display: flex; gap: 8px; align-items: stretch; padding-left: 4px; }
  /* the icon never sizes the row: positioned over a box as tall as the row's two lines of text (and as wide);
     aspect-ratio cannot give the width here (a flex item's width is set before it is stretched) */
  :global(.ii.iic) { align-self: stretch; width: calc(2lh + 6px); margin: -3px 0; }
  .itx { min-width: 0; }
  .deal-ico { display: inline-block; width: 8px; height: 8px; margin-left: 4px; border-radius: 50%; background: var(--gold); vertical-align: middle; position: relative; top: -1px; font-size: 0; }
  /* item types of the list: sticky under the search bar */
  :global(.chd.thead) { border-bottom: 1px solid var(--line); background: var(--bg); padding: 5px 10px; font: 600 11px var(--font); letter-spacing: 0.04em; text-transform: uppercase; position: sticky; top: 66px; z-index: 1; }
</style>
