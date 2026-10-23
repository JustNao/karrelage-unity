<script lang="ts" module>
  import type { Lot } from "../../lib/stuff";
  /** A lot of the output: the combination found, else the lots chosen in the build. */
  export interface OutRow { slot: string; name: string; picture: number; price: number; lot: Lot }
</script>

<script lang="ts">
  import { kamas } from "../../lib/fmt";
  import { SLOT_TITLES, type Totalled } from "../../lib/stuff";
  import { iconOfItem } from "../../lib/stores/itemIcons.svelte";
  import LotTable from "../../lib/components/LotTable.svelte";
  import ProgressBar from "../../lib/components/ProgressBar.svelte";
  import type { Solver } from "./solver.svelte";

  /**
   * Bottom of the Stuff sheet: what the build costs (craft, first roll, selection; Dofus left out on demand) and the
   * cheapest-combination search over the scanned lots, with the lots found as a LotTable.
   */
  let {
    craft,
    first,
    picked,
    estSlots,
    anyChosen,
    dofusSlots,
    allDofusOwned,
    onDofusOwned,
    solver,
    canSearch,
    unscanned,
    onrun,
    rows,
  }: {
    craft: Totalled;
    first: Totalled;
    picked: Totalled;
    /** slots priced at the weekly average (uncraftable items) */
    estSlots: string[];
    anyChosen: boolean;
    dofusSlots: string[];
    allDofusOwned: boolean;
    onDofusOwned: (on: boolean) => void;
    solver: Solver;
    /** some minimum is set */
    canSearch: boolean;
    /** bought items without a fresh scan (the search waits for them) */
    unscanned: string[];
    onrun: () => void;
    rows: OutRow[];
  } = $props();

  const names = (l: string[]) => l.map((s) => SLOT_TITLES[s]).join(", ");
  /** Lots of the output unfolded in the table (by slot). */
  let openLots = $state<Record<string, boolean>>({});
  const r = $derived(solver.result);
</script>

<section class="bottom">
  <div class="box prices">
    <div class="fh">Prix</div>
    <div class="price" title={craft.missingSlots.length ? `Sans prix : ${names(craft.missingSlots)}` : ""}>
      <span class="mut">Craft</span> <b>{kamas(craft.total)}</b>
      {#if craft.missingSlots.length}<span class="bad">{craft.missingSlots.length} sans prix</span>{/if}
      {#if estSlots.length}<span class="mut" title="Objets non craftables au prix moyen : {names(estSlots)}">dont {estSlots.length} estimé{estSlots.length > 1 ? "s" : ""}</span>{/if}
    </div>
    <div class="price" title={first.missingSlots.length ? `Jamais scanné : ${names(first.missingSlots)}` : ""}>
      <span class="mut">Premier jet</span> <b>{kamas(first.total)}</b>
      {#if first.missingSlots.length}<span class="bad">{first.missingSlots.length} sans lot</span>{/if}
    </div>
    {#if anyChosen}
      <div class="price" title={picked.missingSlots.length ? `Pas de lot choisi : ${names(picked.missingSlots)}` : ""}>
        <span class="mut">Sélection</span> <b>{kamas(picked.total)}</b>
        {#if picked.missingSlots.length}<span class="mut">{picked.missingSlots.length} non choisi</span>{/if}
      </div>
    {/if}
    <label class="small"><input type="checkbox" checked={allDofusOwned} disabled={!dofusSlots.length} onchange={(e) => onDofusOwned(e.currentTarget.checked)} /> Sans les Dofus (hors trophées) dans le prix</label>
  </div>
  <div class="box solver">
    <div class="fh">Combinaison la moins chère</div>
    <div class="mut small">Renseignez des minimums dans les cases « min » des stats, puis lancez la recherche parmi les lots scannés.</div>
    {#if solver.solving}
      <div class="prog"><ProgressBar value={solver.progress} height={6} speed={0.1} /></div>
      <div class="small mut">Calcul… {Math.round(solver.progress * 100)} %</div>
      <button onclick={() => solver.cancel()}>Annuler</button>
    {:else}
      <button onclick={onrun} disabled={!canSearch || unscanned.length > 0}>Rechercher</button>
      {#if unscanned.length}<div class="small bad">À scanner d'abord (ou à marquer comme possédé avec ✕) : {names(unscanned)}.</div>{/if}
      {#if solver.cancelled}<span class="small mut">Calcul annulé.</span>{/if}
    {/if}
    {#if r === "none"}<div class="bad small">Aucune combinaison ne satisfait ces minimums avec les lots scannés.</div>
    {:else if r}
      <div class="small">Coût : <b>{kamas(r.cost)}</b>{#if r.approx} <span class="mut">(recherche limitée, résultat approché)</span>{/if}</div>
      <div class="small mut">{Object.keys(r.picks).length} emplacement(s) choisis{#if r.fixedSlots.length} · gardés tels quels : {names(r.fixedSlots)}{/if}</div>
    {/if}
    {#if rows.length}
      <div class="fh tt">{r && r !== "none" ? "Combinaison trouvée" : "Lots choisis"}</div>
      {#snippet itemCell(o: OutRow)}
        {#if iconOfItem(o.picture)}<img class="lic" src={iconOfItem(o.picture)} alt="" />{/if}<span class="mut">{SLOT_TITLES[o.slot]}</span> <b>{o.name}</b>
      {/snippet}
      <LotTable
        lots={rows.map((o) => ({ row: o.lot, key: o.slot, meta: o }))}
        deals={false}
        transcend={false}
        leads={[{ key: "item", label: "Objet", cell: itemCell, sort: (o) => o.name }]}
        open={(k) => !!openLots[k]}
        ontoggle={(k) => (openLots[k] = !openLots[k])}
      />
      <div class="tot">Total <b>{kamas(rows.reduce((s, o) => s + o.price, 0))}</b></div>
    {/if}
  </div>
</section>

<style>
  .bottom { display: grid; grid-template-columns: minmax(260px, 320px) minmax(0, 1fr); gap: 14px; align-items: start; padding-bottom: 16px; }
  @container (max-width: 920px) { .bottom { grid-template-columns: minmax(0, 1fr); } }
  .box { background: var(--panel); border: 1px solid var(--line); border-radius: 6px; padding: 6px 8px; }
  .price { white-space: nowrap; }
  .price .bad { font-size: 11px; margin-left: 4px; }
  .prices .price { margin: 2px 0; }
  .prog { margin: 6px 0 2px; line-height: 0; }
  .small { font-size: 12px; margin: 4px 0; }
  .lic { width: 22px; height: 22px; vertical-align: middle; margin-right: 6px; }
  .tot { margin-top: 8px; text-align: right; }
  .tt { margin-top: 12px; }
</style>
