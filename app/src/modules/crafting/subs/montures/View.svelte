<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteMap } from "svelte/reactivity";
  import { call, kamas, on } from "../../../../lib/api";
  import { benefit, storedPrice } from "../../../../lib/craft";
  import SmoothScroll from "../../../../lib/components/SmoothScroll.svelte";
  import type { StoredPrice } from "../../../../lib/transcend";
  import { FAMILIES, isSenile } from "../../../../lib/mounts";
  import { unitPriceOrNull as lotPrice } from "../../../../lib/price";

  /** Every mount of every family (dragodindes, muldos, volkornes): [game item name, generation, resources per extraction] from the guides (see `source` in mounts.json). */
  const MOUNTS = FAMILIES.flatMap((f) => f.mounts.map(([name, gen, neurons]) => ({ name, gen, neurons, family: f.id, neurone: f.neurone })));
  const MOUNT_NAMES = new Set(MOUNTS.map((m) => m.name));

  type SortKey = "name" | "gen" | "neurons" | "buy" | "sale" | "profit" | "pct" | "good";
  interface Row { gid: number; name: string; family: string; gen: number; neurons: number; buy: number | null; scanned: boolean; bestSenile: boolean; good: number; goodSenile: number; sale: number | null; profit: number | null; pct: number | null }

  let gids = $state.raw<Record<string, number>>({});
  let stored = new SvelteMap<number, StoredPrice>();
  /** Per scanned mount: its lots (unit price, senile or not), from the auction house scans. */
  let scans = new SvelteMap<number, { price: number; senile: boolean }[]>();
  let q = $state("");
  let err = $state("");
  let onlyPriced = $state(false);
  let sort = $state<SortKey>("pct");
  let desc = $state(true);

  let fam = $state(""); // "" = every family
  /** Stored unit price of each family's resource (neurone / ambre / corne). */
  const unitOf = (neurone: string) => (gids[neurone] != null ? storedPrice(stored, gids[neurone]) : null);

  const rows = $derived.by<Row[]>(() =>
    MOUNTS.flatMap(({ name, gen, neurons, family, neurone }) => {
      const unit = unitOf(neurone);
      const gid = gids[name];
      if (gid == null) return [];
      const sc = scans.get(gid);
      // a senile lot gives 1 resource, otherwise `neurons` (= generation); scanned: the lot with the best profit, else the stored price (senility unknown, assumed not senile)
      const gain = (n: number, price: number) => (unit != null ? benefit(unit * n, price) : null);
      const lots = sc?.map((l) => ({ ...l, n: l.senile ? 1 : neurons, b: gain(l.senile ? 1 : neurons, l.price) }));
      const best = lots?.reduce<(typeof lots)[number] | null>((m, l) => (!m || (l.b?.profit ?? -Infinity) > (m.b?.profit ?? -Infinity) || (!m.b && l.price < m.price) ? l : m), null);
      const n = best?.n ?? neurons;
      const buy = best ? best.price : sc ? null : storedPrice(stored, gid);
      const sale = unit != null ? unit * n : null;
      const bn = best ? best.b : buy != null ? gain(n, buy) : null;
      const prof = lots?.filter((l) => (l.b?.profit ?? 0) > 0) ?? [];
      return [{ gid, name, family, gen, neurons: n, buy, scanned: !!sc, bestSenile: !!best?.senile, good: prof.length, goodSenile: prof.filter((l) => l.senile).length, sale, profit: bn?.profit ?? null, pct: bn?.pct ?? null }];
    }),
  );

  const view = $derived.by(() => {
    const n = q.trim().toLowerCase();
    const k = sort;
    const out = rows.filter((r) => (!fam || r.family === fam) && (!n || r.name.toLowerCase().includes(n)) && (!onlyPriced || r.pct != null));
    return out.sort((a, b) => {
      const x = a[k], y = b[k];
      if (x == null && y == null) return 0;
      if (x == null) return 1; // unknown values always last
      if (y == null) return -1;
      const d = typeof x === "string" ? x.localeCompare(y as string, "fr") : (x as number) - (y as number);
      return desc ? -d : d;
    });
  });
  const profitable = (id: string) => rows.filter((r) => r.family === id && r.good > 0).length;

  const scan = (it: any) => {
    if (!MOUNT_NAMES.has(it.name)) return;
    const lots: { price: number; senile: boolean }[] = [];
    for (const lot of it.rows) {
      const price = lotPrice(lot);
      if (price != null) lots.push({ price, senile: isSenile(lot) });
    }
    scans.set(it.gid, lots);
  };

  const setSort = (k: SortKey) => {
    if (sort === k) desc = !desc;
    else { sort = k; desc = k !== "name"; }
  };
  const arrow = (k: SortKey) => (sort === k ? (desc ? " ▾" : " ▴") : "");
  const sgn = (n: number) => (n > 0 ? "+" : "") + kamas(n);

  onMount(() => {
    call<Record<string, number>>("prices", "gids_by_name", { names: [...MOUNTS.map((m) => m.name), ...FAMILIES.map((f) => f.neurone)] }).then((m) => (gids = m)).catch((e) => (err = String(e)));
    call<StoredPrice[]>("prices", "list").then((l) => l.forEach((p) => stored.set(p.gid, p)));
    call<any[]>("auction", "items").then((l) => l.forEach(scan));
    const offs = [on("auction:item", scan), on("auction:cleared", () => scans.clear()), on<StoredPrice>("prices:price", (p) => stored.set(p.gid, p)), on<StoredPrice[]>("prices:bulk", (l) => l.forEach((p) => stored.set(p.gid, p))), on("prices:cleared", () => stored.clear())];
    return () => void offs.forEach((o) => o.then((f) => f()));
  });
</script>

<div class="rv">
  <div class="head">
    <div class="fh">Montures</div>
  </div>
  <div class="head">
    <select bind:value={fam}>
      <option value="">Toutes les montures</option>
      {#each FAMILIES as f (f.id)}<option value={f.id}>{f.title}</option>{/each}
    </select>
    <input type="search" placeholder="Rechercher une monture…" bind:value={q} />
    <label><input type="checkbox" bind:checked={onlyPriced} /> Prix connus seulement</label>
  </div>
  <div class="sum">
    {#each FAMILIES.filter((f) => !fam || f.id === fam) as f (f.id)}
      {@const u = unitOf(f.neurone)}
      <div>
        {#if u == null}<span class="mut">Prix de « {f.neurone} » inconnu : ouvrez-le à l'hôtel de vente.</span>
        {:else}{f.neurone} : <b>{kamas(u)}</b> · {#if profitable(f.id)}<b class="good">{profitable(f.id)}</b> monture{profitable(f.id) > 1 ? "s" : ""} avec un lot rentable{:else}<span class="mut">aucun lot rentable pour l'instant</span>{/if}{/if}
      </div>
    {/each}
  </div>
  {#if err}<div class="bad">Erreur : {err} (relancez l'application pour charger la dernière version)</div>{/if}
  <SmoothScroll>
    <table>
      <thead>
        <tr>
          <th><button onclick={() => setSort("name")}>Monture{arrow("name")}</button></th>
          <th class="n"><button onclick={() => setSort("gen")}>Génération{arrow("gen")}</button></th>
          <th class="n"><button onclick={() => setSort("neurons")}>Quantité{arrow("neurons")}</button></th>
          <th class="n"><button onclick={() => setSort("buy")}>Prix d'achat{arrow("buy")}</button></th>
          <th class="n"><button onclick={() => setSort("sale")}>Valeur des ressources{arrow("sale")}</button></th>
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
            <td class="n">{r.neurons}{#if r.bestSenile} <span class="mut" title="Meilleur lot : monture sénile">sénile</span>{/if}</td>
            <td class="n" title={r.scanned ? "" : "Prix enregistré : sénilité inconnue, ouvrez la monture à l'HDV"}>{r.buy != null ? kamas(r.buy) : "—"}{#if !r.scanned && r.buy != null} <span class="mut">?</span>{/if}</td>
            <td class="n">{r.sale != null ? kamas(r.sale) : "—"}</td>
            <td class="n" class:good={(r.profit ?? 0) > 0} class:bad={(r.profit ?? 0) < 0}>{r.profit != null ? sgn(r.profit) : "—"}</td>
            <td class="n" class:good={(r.pct ?? 0) > 0} class:bad={(r.pct ?? 0) < 0}><b>{r.pct != null ? (r.pct > 0 ? "+" : "") + r.pct.toFixed(1) + " %" : "—"}</b></td>
            <td class="n" class:good={r.good > 0} >{#if r.scanned}<b>{r.good}</b>{#if r.goodSenile} <span class="mut">(dont {r.goodSenile} sénile{r.goodSenile > 1 ? "s" : ""})</span>{/if}{:else}—{/if}</td>
          </tr>
        {:else}
          <tr><td colspan="8" class="mut">Aucune monture à afficher : ouvrez-les et leur ressource (neurone, ambre, corne) à l'hôtel de vente pour enregistrer leurs prix (ou désactivez « Prix connus seulement »).</td></tr>
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
  .head select { padding: 3px 6px; }
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
