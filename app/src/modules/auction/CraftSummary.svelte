<script lang="ts">
  import type { CraftLine, CraftResult } from "../../lib/craft";
  import { kamas, pct, signedKamas } from "../../lib/fmt";
  import { AH_TAX } from "../../lib/price";

  /** Craft cost of the open item (recursive, stored prices), its cheapest sale and the profit, with the ingredient tree. */
  let { craft, sale, gain }: { craft: CraftResult; sale: number; gain: { profit: number; pct: number } | null } = $props();
</script>

<div class="craft" class:pos={!!gain && gain.profit >= 0}>
  <div class="crow">
    <span>Craft <b>{craft.cost != null ? kamas(Math.round(craft.cost)) : "—"}</b></span>
    {#if Number.isFinite(sale)}<span class="mut">Vente min {kamas(Math.round(sale))}</span>{/if}
    {#if gain}
      <span class={gain.profit >= 0 ? "gain" : "loss"} title="Après {AH_TAX * 100} % de taxe de vente : prix de vente − coût de craft">
        Bénéfice <b>{signedKamas(gain.profit)}</b> ({pct(gain.pct)})
      </span>
    {/if}
    {#if craft.missing.length}<span class="warn" title={craft.missing.join(", ")}>prix manquant : {craft.missing.slice(0, 3).join(", ")}{craft.missing.length > 3 ? "…" : ""}</span>{/if}
  </div>
  <details>
    <summary class="mut">Ingrédients</summary>
    {#snippet tree(lines: CraftLine[])}
      <ul>
        {#each lines as l}
          <li>
            {l.qty}× {l.name}
            <span class="mut">{l.unit != null ? kamas(Math.round(l.unit)) + " l'unité" + (l.how === "craft" ? " · crafté" : "") : "prix inconnu"}</span>
            {#if l.sub}{@render tree(l.sub)}{/if}
          </li>
        {/each}
      </ul>
    {/snippet}
    {@render tree(craft.lines)}
  </details>
</div>

<style>
  .craft { margin: 0 0 10px; padding: 6px 10px; border: 1px solid var(--bad); border-radius: 8px; font-family: var(--font); font-size: 13px; }
  .craft.pos { border-color: var(--ok); }
  .craft .crow { display: flex; gap: 14px; flex-wrap: wrap; align-items: baseline; }
  .craft .gain b, .craft .gain { color: var(--ok); }
  .craft .loss b, .craft .loss { color: var(--bad); }
  .craft details { margin-top: 2px; font-size: 12px; }
  .craft summary { cursor: pointer; }
  .craft ul { margin: 2px 0; padding-left: 18px; }
</style>
