<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteMap } from "svelte/reactivity";
  import { call, kamas, on } from "../../../../lib/api";
  import { benefit, craftCost, storedPrice, type Recipes } from "../../../../lib/craft";
  import SmoothScroll from "../../../../lib/components/SmoothScroll.svelte";
  import type { StoredPrice } from "../../../../lib/transcend";

  type SortKey = "name" | "tier" | "cost" | "sale" | "profit" | "pct";
  interface Row { gid: number; name: string; tier: string; stat: string; cost: number | null; sale: number | null; profit: number | null; pct: number | null; recipe: string; missing: string[] }

  let list = $state.raw<{ gid: number; name: string }[]>([]);
  let recipes = $state.raw<Recipes>({ recipes: {}, names: {} });
  let stored = new SvelteMap<number, StoredPrice>();
  let q = $state("");
  let tier = $state("");
  let onlyPriced = $state(true);
  let sort = $state<SortKey>("pct");
  let desc = $state(true);

  /** "Rune Rata Do Feu" -> tier "Rata", stat "Do Feu". */
  const split = (n: string) => {
    const m = n.match(/^Rune (Ta|Pata|Rata) (.+)$/);
    return m ? { tier: m[1], stat: m[2] } : { tier: "?", stat: n.replace(/^Rune /, "") };
  };

  const rows = $derived.by<Row[]>(() =>
    list.map((r) => {
      const { tier, stat } = split(r.name);
      const c = craftCost(r.gid, recipes, stored);
      const cost = c?.cost ?? null;
      const sale = storedPrice(stored, r.gid);
      const b = cost != null && sale != null ? benefit(sale, cost) : null;
      const recipe = (recipes.recipes[r.gid] ?? []).map(([g, n]) => `${n} × ${recipes.names[g] ?? g}`).join(" + ");
      return { gid: r.gid, name: r.name, tier, stat, cost, sale, profit: b?.profit ?? null, pct: b?.pct ?? null, recipe, missing: c?.missing ?? [] };
    }),
  );

  const view = $derived.by(() => {
    const n = q.trim().toLowerCase();
    const k = sort;
    const out = rows.filter((r) => (!tier || r.tier === tier) && (!n || r.name.toLowerCase().includes(n)) && (!onlyPriced || r.profit != null));
    const cmp = (a: Row, b: Row) => {
      const x = a[k], y = b[k];
      if (x == null && y == null) return 0;
      if (x == null) return 1; // unknown values always last
      if (y == null) return -1;
      const d = typeof x === "string" ? x.localeCompare(y as string, "fr") : (x as number) - (y as number);
      return desc ? -d : d;
    };
    return out.sort(cmp);
  });

  const setSort = (k: SortKey) => {
    if (sort === k) desc = !desc;
    else { sort = k; desc = k !== "name" && k !== "tier"; }
  };
  const arrow = (k: SortKey) => (sort === k ? (desc ? " ▾" : " ▴") : "");
  const sgn = (n: number) => (n > 0 ? "+" : "") + kamas(n);

  onMount(() => {
    call<{ gid: number; name: string }[]>("prices", "craft_runes").then((l) => (list = l));
    call<Recipes>("auction", "recipes").then((r) => (recipes = r));
    call<StoredPrice[]>("prices", "list").then((l) => l.forEach((p) => stored.set(p.gid, p)));
    const offs = [on<StoredPrice>("prices:price", (p) => stored.set(p.gid, p)), on<StoredPrice[]>("prices:bulk", (l) => l.forEach((p) => stored.set(p.gid, p))), on("prices:cleared", () => stored.clear())];
    return () => void offs.forEach((o) => o.then((f) => f()));
  });
</script>

<div class="rv">
  <div class="head">
    <div class="fh">Runes <span class="mut">· coût de fabrication (achat ou craft des ingrédients, au moins cher) contre prix de vente, taxe HDV de 2 % déduite</span></div>
    <input type="search" placeholder="Rechercher une rune…" bind:value={q} />
    <select bind:value={tier}>
      <option value="">Tous les rangs</option>
      <option>Ta</option><option>Pata</option><option>Rata</option>
    </select>
    <label><input type="checkbox" bind:checked={onlyPriced} /> Prix connus seulement</label>
  </div>
  <SmoothScroll>
    <table>
      <thead>
        <tr>
          <th><button onclick={() => setSort("name")}>Rune{arrow("name")}</button></th>
          <th><button onclick={() => setSort("tier")}>Rang{arrow("tier")}</button></th>
          <th>Recette</th>
          <th class="n"><button onclick={() => setSort("cost")}>Coût{arrow("cost")}</button></th>
          <th class="n"><button onclick={() => setSort("sale")}>Prix de vente{arrow("sale")}</button></th>
          <th class="n"><button onclick={() => setSort("profit")}>Bénéfice{arrow("profit")}</button></th>
          <th class="n"><button onclick={() => setSort("pct")}>Rentabilité{arrow("pct")}</button></th>
        </tr>
      </thead>
      <tbody>
        {#each view as r (r.gid)}
          <tr>
            <td><b>{r.name}</b></td>
            <td>{r.tier}</td>
            <td class="mut" title={r.missing.length ? "Prix manquant : " + r.missing.join(", ") : ""}>{r.recipe}{#if r.missing.length} <span class="warn">⚠ prix manquant</span>{/if}</td>
            <td class="n">{r.cost != null ? kamas(r.cost) : "—"}</td>
            <td class="n">{r.sale != null ? kamas(r.sale) : "—"}</td>
            <td class="n" class:good={(r.profit ?? 0) > 0} class:bad={(r.profit ?? 0) < 0}>{r.profit != null ? sgn(r.profit) : "—"}</td>
            <td class="n" class:good={(r.pct ?? 0) > 0} class:bad={(r.pct ?? 0) < 0}><b>{r.pct != null ? (r.pct > 0 ? "+" : "") + r.pct.toFixed(1) + " %" : "—"}</b></td>
          </tr>
        {:else}
          <tr><td colspan="7" class="mut">Aucune rune à afficher : ouvrez les runes et leurs ingrédients à l'hôtel de vente pour enregistrer leurs prix (ou désactivez « Prix connus seulement »).</td></tr>
        {/each}
      </tbody>
    </table>
  </SmoothScroll>
</div>

<style>
  .rv { flex: 1; min-height: 0; display: flex; flex-direction: column; font-family: var(--font); padding: 8px 10px 0; }
  .rv > :global(.wrap) { flex: 1; }
  .head { display: flex; align-items: center; gap: 10px; margin-bottom: 10px; flex-wrap: wrap; }
  .head .fh { margin: 0; flex: 1; min-width: 260px; }
  .head input[type="search"] { width: 180px; padding: 3px 6px; }
  table { width: 100%; border-collapse: collapse; font-size: 12px; }
  th { text-align: left; font-weight: 400; color: var(--mut); padding: 2px 6px; position: sticky; top: 0; background: var(--bg); }
  th button { border: 0; background: none; color: inherit; font: inherit; padding: 0; cursor: pointer; }
  td { padding: 4px 6px; border-top: 1px solid var(--line); }
  .n { text-align: right; white-space: nowrap; }
  .good { color: var(--good, #4caf50); }
  .bad { color: var(--bad, #e05555); }
  .warn { color: var(--warn, orange); }
</style>
