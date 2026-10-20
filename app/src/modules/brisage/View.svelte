<script lang="ts">
  import { onMount } from "svelte";
  import { call } from "../../lib/api";
  import { breakEven, lotEffects, meanRoll, outcome, type BrisItem, type BrisPack, type Outcome } from "../../lib/brisage";
  import { craftCost, type Recipes } from "../../lib/craft";
  import { AH_TAX } from "../../lib/transcend";
  import { Columns } from "../../lib/columns.svelte";
  import { lotKeys, unitPriceOrNull } from "../../lib/price";
  import { auction } from "../../lib/stores/auction.svelte";
  import { prices } from "../../lib/stores/prices.svelte";
  import { fmtNum, kamas, signedKamas } from "../../lib/fmt";
  import { useSort } from "../../lib/sort.svelte";
  import Banner from "../../lib/components/Banner.svelte";
  import Grip from "../../lib/components/Grip.svelte";
  import ItemCard from "../../lib/components/ItemCard.svelte";
  import PanelHead from "../../lib/components/PanelHead.svelte";
  import SearchBar, { matcher } from "../../lib/components/SearchBar.svelte";
  import SmoothScroll from "../../lib/components/SmoothScroll.svelte";
  import SortTable from "../../lib/components/SortTable.svelte";

  /** Rows rendered at once in the list (narrow it with the search / filters). */
  const SHOWN = 300;
  const KEY = "karrelage.brisage";

  const load = <T,>(k: string, d: T): T => {
    try {
      return JSON.parse(localStorage.getItem(`${KEY}.${k}`) ?? "null") ?? d;
    } catch {
      return d;
    }
  };
  const save = (k: string, v: unknown) => {
    try {
      localStorage.setItem(`${KEY}.${k}`, JSON.stringify(v));
    } catch {}
  };

  const cols = new Columns(`karrelage.cols.brisage`, { detail: 640 });
  let pack = $state.raw<BrisPack>({ items: [], runes: {} });
  let recipes = $state.raw<Recipes>({ recipes: {}, names: {} });
  let err = $state("");
  let q = $state("");
  let type = $state("");
  let onlyPriced = $state(load("priced", true));
  let noDrop = $state(load("noDrop", false));
  /** Coefficient (%) typed per item (gid -> %). */
  let coefs = $state<Record<string, number>>(load("coefs", {}));
  let selGid = $state<number | null>(null);
  /** Source chosen per item in the detail panel ("craft" or a lot key); default = the best one. */
  let chosen = $state<Record<string, string>>({});
  $effect(() => save("priced", onlyPriced));
  $effect(() => save("noDrop", noDrop));
  $effect(() => save("coefs", coefs));

  const price = (gid: number) => prices.price(gid);
  const coefOf = (gid: number): number | null => coefs[gid] ?? null;

  /** One way of getting the item to break: crafting it (mean roll) or buying a scanned lot (its own effects). */
  interface Source { key: string; kind: "craft" | "lot"; cost: number | null; out: Outcome; even: number | null; lot?: any; missing?: string[] }

  /** Every source of an item, best (lowest break-even coefficient) first. */
  function sourcesOf(i: BrisItem): Source[] {
    const out: Source[] = [];
    if (i.craft) {
      const c = craftCost(i.gid, recipes, prices.map);
      const o = outcome(meanRoll(i), i.level, pack.runes, price);
      out.push({ key: "craft", kind: "craft", cost: c?.cost ?? null, out: o, even: c?.cost != null ? breakEven(c.cost, o.best.value) : null, missing: c?.missing ?? [] });
    }
    const listing = auction.items.get(i.gid);
    if (listing) {
      const keys = lotKeys(listing.rows);
      for (const lot of listing.rows) {
        const cost = unitPriceOrNull(lot);
        if (cost == null) continue;
        const o = outcome(lotEffects(lot), i.level, pack.runes, price);
        out.push({ key: keys.get(lot)!, kind: "lot", cost, out: o, even: breakEven(cost, o.best.value), lot });
      }
    }
    return out.sort((a, b) => (a.even ?? Infinity) - (b.even ?? Infinity));
  }

  type SortKey = "name" | "level" | "cost" | "value" | "even" | "profit";
  interface Row { item: BrisItem; gid: number; name: string; level: number; best: Source | null; cost: number | null; value: number | null; even: number | null; profit: number | null; coef: number | null; scanned: boolean }

  const rows = $derived.by<Row[]>(() =>
    pack.items.map((item) => {
      const best = sourcesOf(item)[0] ?? null;
      const coef = coefOf(item.gid);
      const value = best ? best.out.best.value : null;
      const profit = best?.cost != null && coef != null ? (value! * coef) / 100 - best.cost : null;
      return { item, gid: item.gid, name: item.name, level: item.level, best, cost: best?.cost ?? null, value, even: best?.even ?? null, profit, coef, scanned: auction.items.has(item.gid) };
    }),
  );
  const types = $derived([...new Set(pack.items.map((i) => i.type))].sort((a, b) => a.localeCompare(b, "fr")));

  const sort = useSort<SortKey>("even", false, (k) => k === "level" || k === "value" || k === "profit");
  const view = $derived.by(() => {
    const m = matcher(q);
    return sort.apply(rows.filter((r) => m(r.name) && (!type || r.item.type === type) && (!noDrop || !r.item.drop) && (!onlyPriced || r.even != null)));
  });

  const COLUMNS: { key: SortKey; label: string; num?: boolean; title?: string }[] = [
    { key: "name", label: "Objet" },
    { key: "level", label: "Niv.", num: true },
    { key: "cost", label: "Prix", num: true, title: "Craft (prix enregistrés des ingrédients) ou lot le plus intéressant à l'HDV" },
    { key: "value", label: "Runes à 100 %", num: true, title: "Meilleur résultat (sans focus ou focus sur un effet), runes vendues après la taxe" },
    { key: "even", label: "Coef. rentable", num: true, title: "Coefficient à partir duquel le brisage rembourse l'objet" },
    { key: "profit", label: "Bénéfice", num: true, title: "Au coefficient saisi pour cet objet" },
  ];

  /** Kamas rounded to the unit (rune values are fractional). */
  const k0 = (n: number) => kamas(Math.round(n));
  const focusName = (o: Outcome) => (o.best.focus == null ? "sans focus" : `focus ${o.lines.find((l) => l.a === o.best.focus)?.rune.name.replace(/^Rune /, "")}`);
  const coefText = (c: number | null) => (c == null ? "—" : `${fmtNum(c, c < 10 ? 1 : 0)} %`);

  // ---- detail ----
  const sel = $derived(rows.find((r) => r.gid === selGid) ?? null);
  const sources = $derived(sel ? sourcesOf(sel.item) : []);
  const src = $derived(sources.find((s) => s.key === chosen[selGid ?? ""]) ?? sources[0] ?? null);
  /** Without any source (no recipe price, nothing scanned): the mean roll, priced at nothing. */
  const out = $derived(src?.out ?? (sel ? outcome(meanRoll(sel.item), sel.level, pack.runes, price) : null));
  const coef = $derived(sel ? (coefOf(sel.gid) ?? 100) : 100);
  const k = $derived(coef / 100);
  const setCoef = (v: string) => {
    if (!sel) return;
    const n = parseFloat(v.replace(",", "."));
    if (Number.isFinite(n) && n >= 0) coefs[sel.gid] = n;
    else delete coefs[sel.gid];
  };
  const srcLabel = (s: Source) => (s.kind === "craft" ? `Craft (jet moyen) · ${s.cost != null ? kamas(s.cost) : "prix inconnu"}` : `Lot HDV · ${kamas(s.cost!)}`) + ` · rentable à ${coefText(s.even)}`;

  onMount(() => {
    call<BrisPack>("brisage", "items").then((p) => (pack = p)).catch((e) => (err = String(e)));
    call<Recipes>("auction", "recipes").then((r) => (recipes = r));
    void prices.ensure();
    void auction.ensure();
  });
</script>

<div class="view cgrid" class:anim={!cols.dragging} style:grid-template-columns="1fr 5px {cols.px("detail")}px">
  <div class="list">
    <div class="fh">Brisage</div>
    <SearchBar bind:value={q} bind:priced={onlyPriced} placeholder="Rechercher un objet…">
      {#snippet before()}
        <select bind:value={type}>
          <option value="">Tous les types</option>
          {#each types as t (t)}<option value={t}>{t}</option>{/each}
        </select>
      {/snippet}
      {#snippet after()}<label title="Objets lâchés par un monstre"><input type="checkbox" bind:checked={noDrop} /> Masquer les objets droppables</label>{/snippet}
    </SearchBar>
    {#if err}<Banner inset>Erreur : {err} (relancez l'application pour charger la dernière version)</Banner>{/if}
    <SmoothScroll>
      <SortTable columns={COLUMNS} rows={view.slice(0, SHOWN)} {sort} key={(r) => r.gid} rowProps={(r) => ({ class: [(r.profit ?? 0) > 0 ? "hot" : "", r.gid === selGid ? "sel" : ""].join(" "), onclick: () => (selGid = r.gid) })}>
        {#snippet row(r)}
          <td><b>{r.name}</b> <span class="mut">{r.item.type}</span>{#if r.item.drop} <span class="mut" title="Lâché par un monstre">· drop</span>{/if}</td>
          <td class="n">{r.level}</td>
          <td class="n" title={r.best?.kind === "craft" && r.best.missing?.length ? "Prix manquant : " + r.best.missing.join(", ") : ""}>
            {#if r.cost != null}{kamas(r.cost)} <span class="mut">{r.best?.kind === "lot" ? "HDV" : "craft"}</span>{:else}—{/if}
          </td>
          <td class="n">{#if r.value != null}{k0(r.value)} <span class="mut">{focusName(r.best!.out)}</span>{#if r.best!.out.unpriced} <span class="warn" title="Une rune n'a pas de prix">⚠</span>{/if}{:else}—{/if}</td>
          <td class="n"><b>{coefText(r.even)}</b></td>
          <td class="n" class:good={(r.profit ?? 0) > 0} class:bad={(r.profit ?? 0) < 0}>{#if r.profit != null}{signedKamas(Math.round(r.profit))} <span class="mut">à {coefText(r.coef)}</span>{:else}—{/if}</td>
        {/snippet}
        {#snippet empty()}Aucun objet à afficher : ouvrez les objets (ou leurs ingrédients) et les runes à l'hôtel de vente pour enregistrer leurs prix (ou désactivez « Prix connus seulement »).{/snippet}
      </SortTable>
      {#if view.length > SHOWN}<div class="mut more">{view.length - SHOWN} objets de plus : affinez la recherche.</div>{/if}
    </SmoothScroll>
  </div>

  <Grip {cols} name="detail" dir={-1} />

  <div class="detail" class:cz={cols.collapsed.detail}>
    <PanelHead {cols} name="detail" title="Détail" dir={-1} />
    {#if sel && out}
      <div class="fh">{sel.name} <span class="mut">· niv. {sel.level} · {sel.item.type}{sel.item.drop ? " · drop" : ""}{sel.item.craft ? " · craftable" : ""}</span></div>
      <div class="ctl">
        <label>Coefficient <input type="number" min="0" step="1" value={coefOf(sel.gid) ?? ""} placeholder="100" oninput={(e) => setCoef(e.currentTarget.value)} /> %</label>
        {#if sources.length}
          <select value={src?.key} onchange={(e) => (chosen[sel.gid] = e.currentTarget.value)}>
            {#each sources as s (s.key)}<option value={s.key}>{srcLabel(s)}</option>{/each}
          </select>
        {:else}<span class="mut">Pas de prix : jet moyen, ouvrez l'objet ou ses ingrédients à l'HDV.</span>{/if}
      </div>
      {#if src?.kind === "craft" && src.missing?.length}<div class="warn small">Prix manquant : {src.missing.join(", ")}</div>{/if}

      <table class="tbl runes">
        <thead>
          <tr><th>Rune</th><th class="n">Jet</th><th class="n">Prix</th><th class="n">Runes</th><th class="n">Kamas</th><th class="n">Focus</th><th class="n">Kamas focus</th></tr>
        </thead>
        <tbody>
          {#each out.lines as l, i (l.a)}
            <tr class:hot={out.best.focus === l.a}>
              <td title={l.rune.label ?? ""}>{l.rune.name.replace(/^Rune /, "")}</td>
              <td class="n">{fmtNum(l.value, 1)}</td>
              <td class="n">{#if l.price != null}{kamas(l.price)}{:else}<span class="warn" title="Ouvrez la rune à l'HDV">?</span>{/if}</td>
              <td class="n">{fmtNum(l.count * k)}</td>
              <td class="n">{k0(l.count * (l.price ?? 0) * (1 - AH_TAX) * k)}</td>
              <td class="n">{fmtNum(l.focus * k)}</td>
              <td class="n"><b>{k0(out.focus[i] * k)}</b></td>
            </tr>
          {/each}
          <tr class:hot={out.best.focus == null} class="tot">
            <td colspan="4">Total sans focus</td>
            <td class="n"><b>{k0(out.plain * k)}</b></td>
            <td colspan="2"></td>
          </tr>
        </tbody>
      </table>

      <div class="sum">
        <div>Meilleur résultat : <b>{k0(out.best.value * k)}</b> <span class="mut">({focusName(out)}, taxe de 2 % déduite)</span></div>
        {#if src?.cost != null}
          {@const profit = out.best.value * k - src.cost}
          <div>Prix de l'objet : <b>{kamas(src.cost)}</b> · bénéfice à {coefText(coef)} : <b class:good={profit > 0} class:bad={profit < 0}>{signedKamas(Math.round(profit))}</b></div>
          <div>Rentable à partir d'un coefficient de <b>{coefText(src.even)}</b></div>
        {/if}
        {#if out.unpriced}<div class="warn small">Certaines runes n'ont pas de prix (comptées 0) : ouvrez-les à l'hôtel de vente.</div>{/if}
      </div>

      {#if src?.kind === "lot"}<ItemCard row={src.lot} />{/if}
      <p class="mut small">Runes par effet = (3 × poids × jet × niveau / 200 + 1) × coefficient / poids de la rune. Le focus donne les runes de l'effet choisi plus la moitié du poids des autres effets. Valeurs moyennes : le jeu tire les fractions au hasard.</p>
    {:else}
      <div class="mut">Choisissez un objet dans la liste.</div>
    {/if}
  </div>
</div>

<style>
  .view { flex: 1; min-height: 0; display: grid; font-family: var(--font); }
  .list { min-width: 0; min-height: 0; display: flex; flex-direction: column; padding: 8px 10px 0; }
  .list > :global(.wrap) { flex: 1; }
  .list :global(select) { padding: 3px 6px; }
  .list :global(tbody tr) { cursor: pointer; }
  .list :global(tr.sel td) { box-shadow: inset 0 1px 0 var(--acc), inset 0 -1px 0 var(--acc); }
  .more { padding: 8px 6px; }
  .detail { overflow: auto; background: var(--panel); border-left: 1px solid var(--line); padding: 0 10px 10px; font-size: 12px; }
  .detail :global(.phead) { padding-left: 0; padding-right: 0; }
  .ctl { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; margin-bottom: 8px; }
  .ctl input { width: 70px; padding: 3px 6px; }
  .ctl select { padding: 3px 6px; max-width: 100%; }
  .runes { margin: 6px 0 10px; }
  .runes td { white-space: nowrap; }
  .runes .tot td { font-weight: 600; }
  .sum { padding: 8px 10px; background: var(--bg); border: 1px solid var(--line); margin-bottom: 10px; display: flex; flex-direction: column; gap: 4px; }
  .small { font-size: 11px; }
</style>
