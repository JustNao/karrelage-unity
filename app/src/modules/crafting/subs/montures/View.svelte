<script lang="ts">
  import { onMount } from "svelte";
  import { call } from "../../../../lib/api";
  import { benefit } from "../../../../lib/craft";
  import { FAMILIES, isSenile } from "../../../../lib/mounts";
  import { unitPriceOrNull as lotPrice } from "../../../../lib/price";
  import { auction } from "../../../../lib/stores/auction.svelte";
  import { prices } from "../../../../lib/stores/prices.svelte";
  import { kamas, pct, signedKamas } from "../../../../lib/fmt";
  import { useSort } from "../../../../lib/sort.svelte";
  import Banner from "../../../../lib/components/Banner.svelte";
  import SearchBar, { matcher } from "../../../../lib/components/SearchBar.svelte";
  import SmoothScroll from "../../../../lib/components/SmoothScroll.svelte";
  import SortTable from "../../../../lib/components/SortTable.svelte";

  /** Every mount of every family (dragodindes, muldos, volkornes): [game item name, generation, resources per extraction] from the guides (see `source` in mounts.json). */
  const MOUNTS = FAMILIES.flatMap((f) => f.mounts.map(([name, gen, neurons]) => ({ name, gen, neurons, family: f.id, neurone: f.neurone })));
  const MOUNT_NAMES = new Set(MOUNTS.map((m) => m.name));

  type SortKey = "name" | "gen" | "neurons" | "buy" | "sale" | "profit" | "pct" | "good";
  interface Row { gid: number; name: string; family: string; gen: number; neurons: number; buy: number | null; scanned: boolean; bestSenile: boolean; good: number; goodSenile: number; sale: number | null; profit: number | null; pct: number | null }

  let gids = $state.raw<Record<string, number>>({});
  let q = $state("");
  let err = $state("");
  let onlyPriced = $state(false);
  const sort = useSort<SortKey>("pct", true, (k) => k !== "name");

  let fam = $state(""); // "" = every family
  /** Stored unit price of each family's resource (neurone / ambre / corne). */
  const unitOf = (neurone: string) => (gids[neurone] != null ? prices.price(gids[neurone]) : null);

  /** Per scanned mount: its lots (unit price, senile or not), from the auction house scans (cached per listing object). */
  const scanCache = new WeakMap<object, { price: number; senile: boolean }[]>();
  const scanOf = (gid: number) => {
    const it = auction.items.get(gid);
    if (!it || !MOUNT_NAMES.has(it.name)) return undefined;
    let lots = scanCache.get(it);
    if (!lots) {
      lots = [];
      for (const lot of it.rows) {
        const price = lotPrice(lot);
        if (price != null) lots.push({ price, senile: isSenile(lot) });
      }
      scanCache.set(it, lots);
    }
    return lots;
  };

  const rows = $derived.by<Row[]>(() =>
    MOUNTS.flatMap(({ name, gen, neurons, family, neurone }) => {
      const unit = unitOf(neurone);
      const gid = gids[name];
      if (gid == null) return [];
      const sc = scanOf(gid);
      // a senile lot gives 1 resource, otherwise `neurons` (= generation); scanned: the lot with the best profit, else the stored price (senility unknown, assumed not senile)
      const gain = (n: number, price: number) => (unit != null ? benefit(unit * n, price) : null);
      const lots = sc?.map((l) => ({ ...l, n: l.senile ? 1 : neurons, b: gain(l.senile ? 1 : neurons, l.price) }));
      const best = lots?.reduce<(typeof lots)[number] | null>((m, l) => (!m || (l.b?.profit ?? -Infinity) > (m.b?.profit ?? -Infinity) || (!m.b && l.price < m.price) ? l : m), null);
      const n = best?.n ?? neurons;
      const buy = best ? best.price : sc ? null : prices.price(gid);
      const sale = unit != null ? unit * n : null;
      const bn = best ? best.b : buy != null ? gain(n, buy) : null;
      const prof = lots?.filter((l) => (l.b?.profit ?? 0) > 0) ?? [];
      return [{ gid, name, family, gen, neurons: n, buy, scanned: !!sc, bestSenile: !!best?.senile, good: prof.length, goodSenile: prof.filter((l) => l.senile).length, sale, profit: bn?.profit ?? null, pct: bn?.pct ?? null }];
    }),
  );

  const view = $derived.by(() => {
    const m = matcher(q);
    return sort.apply(rows.filter((r) => (!fam || r.family === fam) && m(r.name) && (!onlyPriced || r.pct != null)));
  });
  const profitable = (id: string) => rows.filter((r) => r.family === id && r.good > 0).length;

  const COLUMNS: { key: SortKey; label: string; num?: boolean }[] = [
    { key: "name", label: "Monture" },
    { key: "gen", label: "Génération", num: true },
    { key: "neurons", label: "Quantité", num: true },
    { key: "buy", label: "Prix d'achat", num: true },
    { key: "sale", label: "Valeur des ressources", num: true },
    { key: "profit", label: "Bénéfice", num: true },
    { key: "pct", label: "Rentabilité", num: true },
    { key: "good", label: "Lots rentables", num: true },
  ];

  onMount(() => {
    call<Record<string, number>>("prices", "gids_by_name", { names: [...MOUNTS.map((m) => m.name), ...FAMILIES.map((f) => f.neurone)] }).then((m) => (gids = m)).catch((e) => (err = String(e)));
    void prices.ensure();
    void auction.ensure();
  });
</script>

<div class="rv">
  <div class="head">
    <div class="fh">Montures</div>
  </div>
  <SearchBar bind:value={q} bind:priced={onlyPriced} placeholder="Rechercher une monture…">
    {#snippet before()}
      <select bind:value={fam}>
        <option value="">Toutes les montures</option>
        {#each FAMILIES as f (f.id)}<option value={f.id}>{f.title}</option>{/each}
      </select>
    {/snippet}
  </SearchBar>
  <div class="sum">
    {#each FAMILIES.filter((f) => !fam || f.id === fam) as f (f.id)}
      {@const u = unitOf(f.neurone)}
      <div>
        {#if u == null}<span class="mut">Prix de « {f.neurone} » inconnu : ouvrez-le à l'hôtel de vente.</span>
        {:else}{f.neurone} : <b>{kamas(u)}</b> · {#if profitable(f.id)}<b class="good">{profitable(f.id)}</b> monture{profitable(f.id) > 1 ? "s" : ""} avec un lot rentable{:else}<span class="mut">aucun lot rentable pour l'instant</span>{/if}{/if}
      </div>
    {/each}
  </div>
  {#if err}<Banner inset>Erreur : {err} (relancez l'application pour charger la dernière version)</Banner>{/if}
  <SmoothScroll>
    <SortTable columns={COLUMNS} rows={view} {sort} key={(r) => r.gid} rowProps={(r) => ({ class: r.good > 0 ? "hot" : "" })}>
      {#snippet row(r)}
        <td><b>{r.name}</b></td>
        <td class="n">{r.gen}</td>
        <td class="n">{r.neurons}{#if r.bestSenile} <span class="mut" title="Meilleur lot : monture sénile">sénile</span>{/if}</td>
        <td class="n" title={r.scanned ? "" : "Prix enregistré : sénilité inconnue, ouvrez la monture à l'HDV"}>{r.buy != null ? kamas(r.buy) : "—"}{#if !r.scanned && r.buy != null} <span class="mut">?</span>{/if}</td>
        <td class="n">{r.sale != null ? kamas(r.sale) : "—"}</td>
        <td class="n" class:good={(r.profit ?? 0) > 0} class:bad={(r.profit ?? 0) < 0}>{r.profit != null ? signedKamas(r.profit) : "—"}</td>
        <td class="n" class:good={(r.pct ?? 0) > 0} class:bad={(r.pct ?? 0) < 0}><b>{r.pct != null ? pct(r.pct) : "—"}</b></td>
        <td class="n" class:good={r.good > 0}>{#if r.scanned}<b>{r.good}</b>{#if r.goodSenile} <span class="mut">(dont {r.goodSenile} sénile{r.goodSenile > 1 ? "s" : ""})</span>{/if}{:else}—{/if}</td>
      {/snippet}
      {#snippet empty()}Aucune monture à afficher : ouvrez-les et leur ressource (neurone, ambre, corne) à l'hôtel de vente pour enregistrer leurs prix (ou désactivez « Prix connus seulement »).{/snippet}
    </SortTable>
  </SmoothScroll>
</div>

<style>
  .rv { flex: 1; min-height: 0; display: flex; flex-direction: column; font-family: var(--font); padding: 8px 10px 0; }
  .rv > :global(.wrap) { flex: 1; }
  .head { display: flex; align-items: center; gap: 10px; margin-bottom: 10px; flex-wrap: wrap; }
  .head .fh { margin: 0; flex: 1; min-width: 260px; }
  .rv :global(select) { padding: 3px 6px; }
  .sum { margin-bottom: 10px; padding: 8px 10px; background: var(--panel); border: 1px solid var(--line); }
</style>
