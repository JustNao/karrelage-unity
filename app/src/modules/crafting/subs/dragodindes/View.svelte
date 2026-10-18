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

  /** Effect carried by senile lots only ("effect 3836: [0]"): a senile mount gives no neurons. */
  const SENILE_EFFECT = 3836;
  const isSenile = (lot: any) => lot.effects.some((e: any) => e.action === SENILE_EFFECT && e.status !== "removed");
  const lotPrice = (lot: any) => {
    const v = lot.prices.filter((p: any) => p.price > 0).map((p: any) => p.price / p.qty);
    return v.length ? Math.min(...v) : null;
  };
  const MOUNT_NAMES = new Set(MOUNTS.map((m) => m[0]));

  type SortKey = "name" | "gen" | "neurons" | "buy" | "sale" | "profit" | "pct" | "good";
  interface Row { gid: number; name: string; gen: number; neurons: number; buy: number | null; scanned: boolean; senile: number; good: number; sale: number | null; profit: number | null; pct: number | null }

  let gids = $state.raw<Record<string, number>>({});
  let stored = new SvelteMap<number, StoredPrice>();
  /** Per scanned mount: unit prices of its non-senile lots and the number of senile ones (from the auction house scans). */
  let scans = new SvelteMap<number, { fresh: number[]; senile: number }>();
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
      const sc = scans.get(gid);
      // scanned: the cheapest lot that is not senile; otherwise the stored price (senility unknown)
      const buy = sc ? (sc.fresh.length ? Math.min(...sc.fresh) : null) : storedPrice(stored, gid);
      const sale = unit != null ? unit * neurons : null;
      const b = buy != null && sale != null ? benefit(sale, buy) : null;
      const good = sc && sale != null ? sc.fresh.filter((p) => benefit(sale, p).profit > 0).length : 0;
      return [{ gid, name, gen, neurons, buy, scanned: !!sc, senile: sc?.senile ?? 0, good, sale, profit: b?.profit ?? null, pct: b?.pct ?? null }];
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
  const profitable = $derived(rows.filter((r) => r.good > 0).length);

  const scan = (it: any) => {
    if (!MOUNT_NAMES.has(it.name)) return;
    const fresh: number[] = [];
    let senile = 0;
    for (const lot of it.rows) {
      if (isSenile(lot)) senile++;
      else {
        const p = lotPrice(lot);
        if (p != null) fresh.push(p);
      }
    }
    scans.set(it.gid, { fresh, senile });
  };

  const setSort = (k: SortKey) => {
    if (sort === k) desc = !desc;
    else { sort = k; desc = k !== "name"; }
  };
  const arrow = (k: SortKey) => (sort === k ? (desc ? " ▾" : " ▴") : "");
  const sgn = (n: number) => (n > 0 ? "+" : "") + kamas(n);

  onMount(() => {
    call<Record<string, number>>("prices", "gids_by_name", { names: [...MOUNTS.map((m) => m[0]), NEURONE] }).then((m) => (gids = m)).catch((e) => (err = String(e)));
    call<StoredPrice[]>("prices", "list").then((l) => l.forEach((p) => stored.set(p.gid, p)));
    call<any[]>("auction", "items").then((l) => l.forEach(scan));
    const offs = [on("auction:item", scan), on("auction:cleared", () => scans.clear()), on<StoredPrice>("prices:price", (p) => stored.set(p.gid, p)), on<StoredPrice[]>("prices:bulk", (l) => l.forEach((p) => stored.set(p.gid, p))), on("prices:cleared", () => stored.clear())];
    return () => void offs.forEach((o) => o.then((f) => f()));
  });
</script>

<div class="rv">
  <div class="head">
    <div class="fh">Dragodindes <span class="mut">· extraction en {NEURONE.toLowerCase()} ; valeur = neurones × prix unitaire, taxe HDV de 2 % déduite ; prix d'achat = lot le moins cher <b>non sénile</b> vu à l'HDV (sinon prix enregistré, sénilité inconnue)</span></div>
    <input type="search" placeholder="Rechercher une dragodinde…" bind:value={q} />
    <label><input type="checkbox" bind:checked={onlyPriced} /> Prix connus seulement</label>
  </div>
  <div class="sum">
    {#if unit == null}<span class="mut">Prix du {NEURONE.toLowerCase()} inconnu : ouvrez-le à l'hôtel de vente.</span>
    {:else}{NEURONE} : <b>{kamas(unit)}</b> · {#if profitable}<b class="good">{profitable}</b> dragodinde{profitable > 1 ? "s" : ""} avec un lot non sénile rentable{:else}<span class="mut">aucun lot non sénile rentable pour l'instant</span>{/if}{/if}
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
          <th class="n"><button onclick={() => setSort("good")}>Lots rentables{arrow("good")}</button></th>
        </tr>
      </thead>
      <tbody>
        {#each view as r (r.gid)}
          <tr class:hot={r.good > 0}>
            <td><b>{r.name}</b></td>
            <td class="n">{r.gen}</td>
            <td class="n">{r.neurons}</td>
            <td class="n" title={r.scanned ? "" : "Prix enregistré : sénilité inconnue, ouvrez la dragodinde à l'HDV"}>{r.buy != null ? kamas(r.buy) : r.scanned ? "sénile" : "—"}{#if !r.scanned && r.buy != null} <span class="mut">?</span>{/if}</td>
            <td class="n">{r.sale != null ? kamas(r.sale) : "—"}</td>
            <td class="n" class:good={(r.profit ?? 0) > 0} class:bad={(r.profit ?? 0) < 0}>{r.profit != null ? sgn(r.profit) : "—"}</td>
            <td class="n" class:good={(r.pct ?? 0) > 0} class:bad={(r.pct ?? 0) < 0}><b>{r.pct != null ? (r.pct > 0 ? "+" : "") + r.pct.toFixed(1) + " %" : "—"}</b></td>
            <td class="n" class:good={r.good > 0} title={r.senile ? `${r.senile} lot(s) sénile(s) ignoré(s)` : ""}>{#if r.scanned}<b>{r.good}</b>{#if r.senile} <span class="mut">(+{r.senile} sénile{r.senile > 1 ? "s" : ""})</span>{/if}{:else}—{/if}</td>
          </tr>
        {:else}
          <tr><td colspan="8" class="mut">Aucune dragodinde à afficher : ouvrez-les et le {NEURONE.toLowerCase()} à l'hôtel de vente pour enregistrer leurs prix (ou désactivez « Prix connus seulement »).</td></tr>
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
  tr.hot td { background: color-mix(in srgb, var(--good, #4caf50) 14%, transparent); }
  .good { color: var(--good, #4caf50); }
  .bad { color: var(--bad, #e05555); }
</style>
