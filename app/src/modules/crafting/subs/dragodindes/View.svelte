<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteMap } from "svelte/reactivity";
  import { call, kamas, on } from "../../../../lib/api";
  import { benefit, storedPrice } from "../../../../lib/craft";
  import SmoothScroll from "../../../../lib/components/SmoothScroll.svelte";
  import type { StoredPrice } from "../../../../lib/transcend";
  import table from "./dragodindes.json";

  /** [game item name, generation, neurons per extraction] from the guide (see `source` in the json). */
  const MOUNTS = table.mounts as [string, number, number][];
  const NEURONE = table.neurone;

  type SortKey = "name" | "gen" | "neurons" | "buy" | "sale" | "profit" | "pct";
  interface Row { gid: number; name: string; gen: number; neurons: number; buy: number | null; sale: number | null; profit: number | null; pct: number | null }

  let gids = $state.raw<Record<string, number>>({});
  let stored = new SvelteMap<number, StoredPrice>();
  let q = $state("");
  let err = $state("");
  let onlyPriced = $state(false);
  let sort = $state<SortKey>("pct");
  let desc = $state(true);

  /** Value of the neurons obtained by extracting one mount, sold at the neurone's stored unit price. */
  const unit = $derived(gids[NEURONE] != null ? storedPrice(stored, gids[NEURONE]) : null);

  const rows = $derived.by<Row[]>(() =>
    MOUNTS.flatMap(([name, gen, neurons]) => {
      const gid = gids[name];
      if (gid == null) return [];
      const buy = storedPrice(stored, gid);
      const sale = unit != null ? unit * neurons : null;
      const b = buy != null && sale != null ? benefit(sale, buy) : null;
      return [{ gid, name, gen, neurons, buy, sale, profit: b?.profit ?? null, pct: b?.pct ?? null }];
    }),
  );

  const view = $derived.by(() => {
    const n = q.trim().toLowerCase();
    const k = sort;
    const out = rows.filter((r) => (!n || r.name.toLowerCase().includes(n)) && (!onlyPriced || r.pct != null));
    return out.sort((a, b) => {
      const x = a[k], y = b[k];
      if (x == null && y == null) return 0;
      if (x == null) return 1; // unknown values always last
      if (y == null) return -1;
      const d = typeof x === "string" ? x.localeCompare(y as string, "fr") : (x as number) - (y as number);
      return desc ? -d : d;
    });
  });
  const profitable = $derived(rows.filter((r) => (r.profit ?? 0) > 0).length);

  const setSort = (k: SortKey) => {
    if (sort === k) desc = !desc;
    else { sort = k; desc = k !== "name"; }
  };
  const arrow = (k: SortKey) => (sort === k ? (desc ? " ▾" : " ▴") : "");
  const sgn = (n: number) => (n > 0 ? "+" : "") + kamas(n);

  onMount(() => {
    call<Record<string, number>>("prices", "gids_by_name", { names: [...MOUNTS.map((m) => m[0]), NEURONE] }).then((m) => (gids = m)).catch((e) => (err = String(e)));
    call<StoredPrice[]>("prices", "list").then((l) => l.forEach((p) => stored.set(p.gid, p)));
    const offs = [on<StoredPrice>("prices:price", (p) => stored.set(p.gid, p)), on<StoredPrice[]>("prices:bulk", (l) => l.forEach((p) => stored.set(p.gid, p))), on("prices:cleared", () => stored.clear())];
    return () => void offs.forEach((o) => o.then((f) => f()));
  });
</script>

<div class="rv">
  <div class="head">
    <div class="fh">Dragodindes <span class="mut">· extraction en {NEURONE.toLowerCase()} ; valeur = neurones × prix unitaire, taxe HDV de 2 % déduite</span></div>
    <input type="search" placeholder="Rechercher une dragodinde…" bind:value={q} />
    <label><input type="checkbox" bind:checked={onlyPriced} /> Prix connus seulement</label>
  </div>
  <div class="sum">
    {#if unit == null}<span class="mut">Prix du {NEURONE.toLowerCase()} inconnu : ouvrez-le à l'hôtel de vente.</span>
    {:else}{NEURONE} : <b>{kamas(unit)}</b> · {#if profitable}<b class="good">{profitable}</b> extraction{profitable > 1 ? "s" : ""} rentable{profitable > 1 ? "s" : ""}{:else}<span class="mut">aucune extraction rentable pour l'instant</span>{/if}{/if}
  </div>
  {#if err}<div class="bad">Erreur : {err} (relancez l'application pour charger la dernière version)</div>{/if}
  <SmoothScroll>
    <table>
      <thead>
        <tr>
          <th><button onclick={() => setSort("name")}>Dragodinde{arrow("name")}</button></th>
          <th class="n"><button onclick={() => setSort("gen")}>Génération{arrow("gen")}</button></th>
          <th class="n"><button onclick={() => setSort("neurons")}>Neurones{arrow("neurons")}</button></th>
          <th class="n"><button onclick={() => setSort("buy")}>Prix d'achat{arrow("buy")}</button></th>
          <th class="n"><button onclick={() => setSort("sale")}>Valeur des neurones{arrow("sale")}</button></th>
          <th class="n"><button onclick={() => setSort("profit")}>Bénéfice{arrow("profit")}</button></th>
          <th class="n"><button onclick={() => setSort("pct")}>Rentabilité{arrow("pct")}</button></th>
        </tr>
      </thead>
      <tbody>
        {#each view as r (r.gid)}
          <tr>
            <td><b>{r.name}</b></td>
            <td class="n">{r.gen}</td>
            <td class="n">{r.neurons}</td>
            <td class="n">{r.buy != null ? kamas(r.buy) : "—"}</td>
            <td class="n">{r.sale != null ? kamas(r.sale) : "—"}</td>
            <td class="n" class:good={(r.profit ?? 0) > 0} class:bad={(r.profit ?? 0) < 0}>{r.profit != null ? sgn(r.profit) : "—"}</td>
            <td class="n" class:good={(r.pct ?? 0) > 0} class:bad={(r.pct ?? 0) < 0}><b>{r.pct != null ? (r.pct > 0 ? "+" : "") + r.pct.toFixed(1) + " %" : "—"}</b></td>
          </tr>
        {:else}
          <tr><td colspan="7" class="mut">Aucune dragodinde à afficher : ouvrez-les et le {NEURONE.toLowerCase()} à l'hôtel de vente pour enregistrer leurs prix (ou désactivez « Prix connus seulement »).</td></tr>
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
  .head input[type="search"] { width: 200px; padding: 3px 6px; }
  .sum { margin-bottom: 10px; padding: 8px 10px; background: var(--panel); border: 1px solid var(--line); }
  table { width: 100%; border-collapse: collapse; font-size: 12px; }
  th { text-align: left; font-weight: 400; color: var(--mut); padding: 2px 6px; position: sticky; top: 0; background: var(--bg); }
  th button { border: 0; background: none; color: inherit; font: inherit; padding: 0; cursor: pointer; }
  td { padding: 4px 6px; border-top: 1px solid var(--line); }
  .n { text-align: right; white-space: nowrap; }
  .good { color: var(--good, #4caf50); }
  .bad { color: var(--bad, #e05555); }
</style>
