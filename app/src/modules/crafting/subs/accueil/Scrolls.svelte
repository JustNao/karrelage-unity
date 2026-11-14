<script lang="ts">
  import { onMount } from "svelte";
  import { afterTax, effectivePrice } from "../../../../lib/price";
  import { prices } from "../../../../lib/stores/prices.svelte";
  import { fmtInt, fmtNum, kamas } from "../../../../lib/fmt";
  import { fitRows } from "../../../../lib/actions/fitRows";
  import { copyName, homeItems } from "./home";

  /**
   * Accueil card: the characteristic scrolls (type 76) worth the most kamas per aviton, bought with avitons then sold at the
   * auction house (price: cheapest read, else weekly average; after the 2 % tax), best first: as many as the card holds. Aviton prices by weight: Petit 20, plain 60,
   * Grand 140, Puissant 340 (told by the author).
   */
  const AVITONS: Record<string, number> = { petit: 20, "": 60, grand: 140, puissant: 340 };
  const NAME = /^(?:(Petit|Grand|Puissant) )?Parchemin (?:de |d')(.+)$/i;

  let scrolls = $state.raw<{ gid: number; name: string; cost: number }[]>([]);
  let err = $state("");
  onMount(() => {
    homeItems()
      .then((h) =>
        (scrolls = h.scrolls.flatMap((s) => {
          const m = NAME.exec(s.name);
          const cost = m ? AVITONS[(m[1] ?? "").toLowerCase()] : undefined;
          return cost ? [{ ...s, cost }] : [];
        })),
      )
      .catch((e) => (err = String(e)));
  });

  const rows = $derived(
    scrolls
      .flatMap((s) => {
        const p = effectivePrice(prices.map.get(s.gid));
        return p ? [{ ...s, price: p.value, avg: p.source === "moyenne", per: afterTax(p.value) / s.cost }] : [];
      })
      .sort((a, b) => b.per - a.per),
  );
  const unpriced = $derived(scrolls.length - rows.length);
</script>

<div class="fill">
  {#if err}<div class="bad">{err}</div>{/if}
  <div class="mut note">Kamas obtenus par aviton dépensé, une fois le parchemin revendu à l'hôtel de vente (taxe déduite).</div>
  <div class="rows" use:fitRows>
    {#each rows as r, i (r.gid)}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="row" onclick={() => copyName(r.name)} title="Cliquer pour copier le nom">
        <span class="rank mut">{i + 1}</span>
        <span class="n"><b>{r.name}</b> <span class="mut">{fmtInt(r.cost)} avitons</span></span>
        <span class="p">{kamas(Math.round(r.price))}{#if r.avg} <span class="mut avg">moy.</span>{/if}</span>
        <span class="per"><b>{fmtNum(r.per, 1)}</b> <span class="mut">K / aviton</span></span>
      </div>
    {:else}
      <div class="mut">Aucun prix connu : ouvrez les parchemins de caractéristique à l'hôtel de vente.</div>
    {/each}
  </div>
  {#if unpriced}<div class="mut small">{unpriced} parchemin{unpriced > 1 ? "s" : ""} sans prix, non comparé{unpriced > 1 ? "s" : ""}.</div>{/if}
</div>

<style>
  .fill { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .rows { flex: 1; min-height: 0; }
  .note { flex: none; font-size: 12px; margin-bottom: 6px; }
  .row { display: flex; align-items: baseline; gap: 10px; padding: 3px 0; border-top: 1px solid var(--line); cursor: pointer; }
  .row:hover .n b { text-decoration: underline; }
  .rank { flex: none; width: 14px; }
  .n { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .p, .per { flex: none; font-variant-numeric: tabular-nums; }
  .per { min-width: 110px; text-align: right; }
  .avg { font-size: 10px; }
  .small { flex: none; font-size: 12px; margin-top: 6px; }
</style>
