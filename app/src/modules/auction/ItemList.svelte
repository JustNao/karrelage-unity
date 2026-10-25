<script lang="ts">
  import { SvelteSet } from "svelte/reactivity";
  import { call } from "../../lib/api";
  import type { Columns } from "../../lib/columns.svelte";
  import { ago, kamas } from "../../lib/fmt";
  import { readJSON, writeJSON } from "../../lib/persist.svelte";
  import { auction } from "../../lib/stores/auction.svelte";
  import { OPEN_BUILD_KEY, toBuy, type Build } from "../../lib/stuff";
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
  /** "Stuff à acheter": only the items of the build open in the Stuff tab that are still to buy (not owned). */
  const STUFF_KEY = "karrelage.auction.stuffOnly";
  let stuffOnly = $state(readJSON<boolean>(STUFF_KEY, false));
  $effect(() => writeJSON(STUFF_KEY, stuffOnly));
  let build = $state.raw<Build | null>(null);
  $effect(() => {
    if (!stuffOnly) return;
    const id = readJSON<string | null>(OPEN_BUILD_KEY, null); // read again on each toggle and each visit of the tab
    call<Build[]>("stuff", "list").then((l) => (build = l.find((b) => b.id === id) ?? l[0] ?? null)).catch(() => (build = null));
  });
  const wanted = $derived(stuffOnly && build ? toBuy(build) : null);
  let barH = $state(0); // the item types stick right under the bar
  const typeGroups = $derived.by(() => {
    const m = new Map<string, any[]>();
    const match = matcher(search);
    for (const i of [...auction.items.values()].sort((a, b) => b.updated - a.updated)) {
      if (!match(i.name, i.type) || (stuffOnly && !wanted?.has(i.gid))) continue;
      const t = i.type ?? "?";
      if (!m.has(t)) m.set(t, []);
      m.get(t)!.push(i);
    }
    return [...m.entries()].sort((a, b) => a[0].localeCompare(b[0], "fr")).map(([type, items]) => ({ type, items }));
  });
</script>

<SideList {cols} name="list" title="Objets">
  {#snippet bar()}
    <div class="bar" bind:offsetHeight={barH}>
      <SearchBar bind:value={search} placeholder="Rechercher un objet…" grow compact />
      <div class="tools">
        <button class="mini" class:on={stuffOnly} onclick={() => (stuffOnly = !stuffOnly)} title="N'afficher que les objets du stuff ouvert dans l'onglet Stuff qui restent à acheter{build ? ` (${build.name})` : ''}">Stuff à acheter</button>
        <span class="sp"></span>
        <ConfirmButton mini label="Nettoyer" title="Oublier tous les objets enregistrés" onconfirm={() => call("auction", "clear")} />
      </div>
    </div>
  {/snippet}
  <div class="groups" style:--thead-top="{barH + 42}px">
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
            <span class="mut">{i.lots} lots · {i.min_price ? "min " + kamas(i.min_price) : "—"} · {ago(i.updated)}{i.demo ? " · demo" : ""}</span>
          </div>
        </div>
      {/each}
    </Collapsible>
  {:else}
    <div class="mut empty">{stuffOnly && !build ? "Aucun stuff ouvert dans l'onglet Stuff." : stuffOnly ? "Aucun objet à acheter de ce stuff n'a été scanné" + (search ? " (ou aucun ne correspond à la recherche)." : ".") : search ? "Aucun objet ne correspond à la recherche." : "Ouvrez un objet à l'hôtel de vente : il apparaît ici (dernière liste par objet)."}</div>
  {/each}
  </div>
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
  :global(.chd.thead) { border-bottom: 1px solid var(--line); background: var(--bg); padding: 5px 10px; font: 600 11px var(--font); letter-spacing: 0.04em; text-transform: uppercase; position: sticky; top: var(--thead-top, 66px); z-index: 1; }
  .bar { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 6px; }
  .tools { display: flex; gap: 6px; align-items: center; }
  .tools .sp { flex: 1; }
  .tools .on { border-color: var(--acc); color: var(--acc); }
  .groups { display: contents; }
</style>
