<script lang="ts">
  import { onMount } from "svelte";
  import { kamasFull } from "../../lib/fmt";
  import { unitPrice } from "../../lib/price";
  import { sellAdvice, UNDERCUT, type SellAdvice } from "../../lib/sell";
  import { inventory } from "../../lib/stores/inventory.svelte";
  import ItemCard from "../../lib/components/ItemCard.svelte";

  /**
   * HDV > Vente: the copies of the open item the player owns (inventory, worn or in the bag, and their own lots on sale),
   * each with a recommended price (lib/sell.ts) against the lots on sale that pass the filters. A copy can be pinned to be
   * compared with the lots of the Achat tab.
   */
  let {
    item,
    rows,
    keyOf,
    isMine,
    effLabel,
    pinned,
    onpin,
    onshow,
  }: {
    item: any;
    /** The lots passing the filters (the player's own included). */
    rows: any[];
    keyOf: (row: any) => string;
    isMine: (row: any) => boolean;
    effLabel: (a: number) => string;
    pinned: (row: any) => boolean;
    onpin: (row: any) => void;
    /** Show a lot in the Achat tab. */
    onshow: (key: string) => void;
  } = $props();

  onMount(() => void inventory.ensure().catch(() => {}));

  const others = $derived(rows.filter((r) => !isMine(r)));
  const pieces = $derived([
    ...item.rows.filter(isMine).map((row: any) => ({ row, onSale: true, label: "En vente" })),
    ...inventory.ofGid(item.gid).map((row: any) => ({ row, onSale: false, label: row.equipped ? "Équipé" : "Inventaire" })),
  ]);
  const advised = $derived(pieces.map((p) => ({ ...p, advice: sellAdvice(p.row, others) })));
  const listed = $derived(new Set(rows));

  const linesText = (a: SellAdvice) => a.lines.map((l) => `${l.hi} ${effLabel(l.action)}`).join(", ");
  function basisText(a: SellAdvice): string {
    const n = `${a.compared} lot${a.compared > 1 ? "s" : ""}`;
    if (a.fellBack) return `Aucun lot en vente avec ${linesText(a)} (ou mieux) : densité comparée aux ${n}`;
    if (a.basis === "density") return `Densité comparée aux ${n} en vente`;
    return `Comparé aux ${n} avec ${linesText(a)} (ou mieux)`;
  }
</script>

<div class="sell">
  {#if !advised.length}
    <p class="mut">
      Aucun exemplaire de {item.name} dans l'inventaire ni en vente.
      {#if !inventory.updated}L'inventaire est lu à la connexion du personnage : reconnectez-vous avec la capture active.{/if}
    </p>
  {:else}
    <p class="mut note">Prix conseillés face aux lots en vente qui passent les filtres. 📌 épingle l'exemplaire pour le comparer aux lots de l'onglet Achat.</p>
  {/if}
  {#each advised as p (p.row.uid ?? keyOf(p.row))}
    {@const a = p.advice}
    {@const ref = a.ref && listed.has(a.ref) ? a.ref : null}
    <div class="piece" class:sale={p.onSale}>
      <div class="adv">
        <span class="tag">{p.label}</span>
        {#if a.price == null}
          <span class="mut">Aucun lot en vente à comparer</span>
        {:else}
          <span>Prix conseillé <b class="price">{kamasFull(a.price)}</b></span>
          {#if p.onSale}
            {@const cur = unitPrice(p.row)}
            {#if cur > a.price}<span class="warn">baisser de {kamasFull(cur - a.price)}</span>
            {:else if cur < a.price}<span class="ok">peut monter de {kamasFull(a.price - cur)}</span>
            {:else}<span class="ok">bien placé</span>{/if}
          {/if}
          <div class="why mut">
            {basisText(a)} ·
            {#if a.best}plus dense que tous : au prix du plus cher{:else}{kamasFull(UNDERCUT)} sous le moins cher au moins aussi dense{/if}
            {#if ref}(<button class="link" onclick={() => onshow(keyOf(ref))}>{kamasFull(unitPrice(ref))}</button>){/if}
          </div>
        {/if}
      </div>
      <ItemCard row={p.row} priced={p.onSale} pinned={pinned(p.row)} onpin={() => onpin(p.row)} />
    </div>
  {/each}
</div>

<style>
  .note { margin: 0 0 10px; font-size: 12px; }
  .piece { margin-bottom: 12px; padding: 8px 10px 0; border: 1px solid var(--line); border-radius: 10px; }
  .piece.sale { border-color: color-mix(in srgb, var(--ok) 50%, var(--line)); }
  .adv { display: flex; flex-wrap: wrap; gap: 4px 10px; align-items: baseline; margin-bottom: 6px; }
  .tag { font-size: 10px; letter-spacing: 0.08em; text-transform: uppercase; color: var(--mut); }
  .sale .tag { color: var(--ok); }
  .price { color: var(--gold); }
  .why { flex-basis: 100%; font-size: 12px; }
  .ok { color: var(--ok); }
  .warn { color: var(--warn); }
  .link { border: 0; background: none; color: var(--acc); padding: 0; text-decoration: underline; }
  .piece :global(.card) { border: 0; padding-left: 0; padding-right: 0; background: none; }
</style>
