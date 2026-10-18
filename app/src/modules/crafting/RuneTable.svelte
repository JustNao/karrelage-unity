<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteMap } from "svelte/reactivity";
  import { call, kamas, on } from "../../lib/api";
  import { benefit, craftCost, storedPrice, type Recipes } from "../../lib/craft";
  import type { StoredPrice } from "../../lib/transcend";

  let { kind, title, tiers: TIERS, parse }: { kind: "forge" | "transcend"; title: string; tiers: string[]; parse: (name: string) => { tier: string; stat: string } | null } = $props();
  type Tier = string;
  type SortKey = "stat" | "best" | Tier;
  interface Cell { effect: string | null; gid: number; name: string; cost: number | null; sale: number | null; profit: number | null; pct: number | null; recipe: string; missing: string[] }
  interface Row { stat: string; cells: Record<string, Cell | undefined>; best: number | null }

  let list = $state.raw<{ gid: number; name: string; effect: string | null }[]>([]);
  let recipes = $state.raw<Recipes>({ recipes: {}, names: {} });
  let stored = new SvelteMap<number, StoredPrice>();
  let q = $state("");
  let onlyPriced = $state(true);
  let sort = $state<SortKey>("best");
  let desc = $state(true);

  const rows = $derived.by<Row[]>(() => {
    const by = new Map<string, Row>();
    for (const r of list) {
      const sp = parse(r.name);
      if (!sp) continue;
      const c = craftCost(r.gid, recipes, stored);
      const cost = c?.cost ?? null;
      const sale = storedPrice(stored, r.gid);
      const b = cost != null && sale != null ? benefit(sale, cost) : null;
      const recipe = (recipes.recipes[r.gid] ?? []).map(([g, n]) => `${n} × ${recipes.names[g] ?? g}`).join(" + ");
      if (!by.has(sp.stat)) by.set(sp.stat, { stat: sp.stat, cells: {}, best: null });
      by.get(sp.stat)!.cells[sp.tier] = { effect: r.effect, gid: r.gid, name: r.name, cost, sale, profit: b?.profit ?? null, pct: b?.pct ?? null, recipe, missing: c?.missing ?? [] };
    }
    for (const row of by.values()) {
      const p = TIERS.map((t) => row.cells[t]?.pct).filter((x): x is number => x != null);
      row.best = p.length ? Math.max(...p) : null;
    }
    return [...by.values()];
  });

  const value = (r: Row, k: SortKey) => (k === "stat" ? r.stat : k === "best" ? r.best : (r.cells[k]?.pct ?? null));
  const view = $derived.by(() => {
    const n = q.trim().toLowerCase();
    const k = sort;
    const out = rows.filter((r) => (!n || r.stat.toLowerCase().includes(n)) && (!onlyPriced || r.best != null));
    return out.sort((a, b) => {
      const x = value(a, k), y = value(b, k);
      if (x == null && y == null) return 0;
      if (x == null) return 1; // unknown values always last
      if (y == null) return -1;
      const d = typeof x === "string" ? x.localeCompare(y as string, "fr") : (x as number) - (y as number);
      return desc ? -d : d;
    });
  });

  const setSort = (k: SortKey) => {
    if (sort === k) desc = !desc;
    else { sort = k; desc = k !== "stat"; }
  };
  const arrow = (k: SortKey) => (sort === k ? (desc ? " ▾" : " ▴") : "");
  const pct = (n: number) => (n > 0 ? "+" : "") + n.toFixed(1) + " %";

  onMount(() => {
    call<{ gid: number; name: string; effect: string | null }[]>("prices", "craft_runes", { kind }).then((l) => (list = l));
    call<Recipes>("auction", "recipes").then((r) => (recipes = r));
    call<StoredPrice[]>("prices", "list").then((l) => l.forEach((p) => stored.set(p.gid, p)));
    const offs = [on<StoredPrice>("prices:price", (p) => stored.set(p.gid, p)), on<StoredPrice[]>("prices:bulk", (l) => l.forEach((p) => stored.set(p.gid, p))), on("prices:cleared", () => stored.clear())];
    return () => void offs.forEach((o) => o.then((f) => f()));
  });
</script>

<div class="rv">
  <div class="head">
    <div class="fh cat">{title} <span class="mut">· coût de fabrication (achat ou craft des ingrédients, au moins cher) contre prix de vente, taxe HDV de 2 % déduite</span></div>
    <input type="search" placeholder="Rechercher une rune…" bind:value={q} />
    <label><input type="checkbox" bind:checked={onlyPriced} /> Prix connus seulement</label>
  </div>
    <table>
      <thead>
        <tr>
          <th><button onclick={() => setSort("stat")}>Rune{arrow("stat")}</button></th>
          {#each TIERS as t (t)}
            <th class="n"><button onclick={() => setSort(t)}>{t}{arrow(t)}</button></th>
          {/each}
          <th class="n"><button onclick={() => setSort("best")}>Meilleur{arrow("best")}</button></th>
        </tr>
      </thead>
      <tbody>
        {#each view as r (r.stat)}
          <tr>
            <td><b>{r.stat}</b><br />
              <small class="mut">{#each TIERS as t (t)}{#if r.cells[t]?.effect}<span class="eff">{t} {r.cells[t]?.effect}</span>{/if}{/each}</small></td>
            {#each TIERS as t (t)}
              {@const c = r.cells[t]}
              <td class="n">
                {#if c}
                  <span class="p" class:good={(c.pct ?? 0) > 0} class:bad={(c.pct ?? 0) < 0}>{c.pct != null ? pct(c.pct) : "—"}</span>
                  <small class="mut" title={c.recipe + (c.missing.length ? " — prix manquant : " + c.missing.join(", ") : "")}>
                    {c.cost != null ? kamas(c.cost) : "?"} → {c.sale != null ? kamas(c.sale) : "?"}{#if c.profit != null} · {c.profit > 0 ? "+" : ""}{kamas(c.profit)}{/if}{#if c.missing.length} <span class="warn">⚠</span>{/if}
                  </small>
                {:else}
                  <span class="mut">—</span>
                {/if}
              </td>
            {/each}
            <td class="n" class:good={(r.best ?? 0) > 0} class:bad={(r.best ?? 0) < 0}><b>{r.best != null ? pct(r.best) : "—"}</b></td>
          </tr>
        {:else}
          <tr><td colspan={TIERS.length + 2} class="mut">Aucune rune à afficher : ouvrez les runes et leurs ingrédients à l'hôtel de vente pour enregistrer leurs prix (ou désactivez « Prix connus seulement »).</td></tr>
        {/each}
      </tbody>
    </table>
</div>

<style>
  .rv { font-family: var(--font); padding: 8px 10px 14px; }
  .head { display: flex; align-items: center; gap: 10px; margin-bottom: 10px; flex-wrap: wrap; }
  .head .fh { margin: 0; flex: 1; min-width: 260px; }
  .head input[type="search"] { width: 180px; padding: 3px 6px; }
  table { width: 100%; border-collapse: collapse; font-size: 12px; }
  th { text-align: left; font-weight: 400; color: var(--mut); padding: 2px 6px; position: sticky; top: 0; background: var(--bg); }
  th button { border: 0; background: none; color: inherit; font: inherit; padding: 0; cursor: pointer; }
  td { padding: 4px 6px; border-top: 1px solid var(--line); }
  .n { text-align: right; white-space: nowrap; }
  .eff + .eff::before { content: " · "; }
  .p { display: block; font-weight: 700; }
  .good { color: var(--good, #4caf50); }
  .bad { color: var(--bad, #e05555); }
  .warn { color: var(--warn, orange); }
</style>
