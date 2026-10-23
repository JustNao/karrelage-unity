<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { cached, call, listenAll } from "../../lib/api";
  import { persisted, readJSON, removeKey } from "../../lib/persist.svelte";
  import { breakEven, lotEffects, meanRoll, outcome, type BrisItem, type BrisPack, type Outcome } from "../../lib/brisage";
  import { craftCost, type Recipes } from "../../lib/craft";
  import { Columns } from "../../lib/columns.svelte";
  import { afterTax, lotKeys, unitPriceOrNull } from "../../lib/price";
  import { auction } from "../../lib/stores/auction.svelte";
  import { prices } from "../../lib/stores/prices.svelte";
  import { ago, fmtDateTime, fmtNum, kamas, runeShort, signedKamas } from "../../lib/fmt";
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
  /** Filters and selection, remembered between tabs and sessions. */
  const UI_KEY = "karrelage.brisage.ui";
  /** Coefficients typed before they were stored by the backend (moved there once). */
  const OLD_COEFS_KEY = "karrelage.brisage.coefs";
  /** Weapon types (the rest of the breakable types are worn equipment). */
  const WEAPONS = new Set([2, 3, 4, 5, 6, 7, 8, 19, 20, 21, 22, 114, 271]);

  interface UiState { q: string; types: number[]; lvMin: number | null; lvMax: number | null; priced: boolean; noDrop: boolean; absent: boolean; sel: number | null }
  const ui = persisted<UiState>(UI_KEY, { q: "", types: [], lvMin: null, lvMax: null, priced: true, noDrop: false, absent: false, sel: null });

  const cols = new Columns(`karrelage.cols.brisage`, { detail: 640 });
  let pack = $state.raw<BrisPack>({ items: [], runes: {} });
  let recipes = $state.raw<Recipes>({ recipes: {}, names: {} });
  let err = $state("");

  /** An item's coefficient: read from a break (`DecraftResultEvent`) or typed. */
  interface Coef { gid: number; coef: number; t: number; source: "brisage" | "manuel"; guessed?: boolean; first?: number; qty?: number | null; focus?: number | null }
  let coefs = $state<Record<string, Coef>>({});
  /** Break results whose item the backend could not tell (focus break, uid not in the inventory): to assign by hand. */
  /** A break result as the backend records it (first / last = coefficient of the first / last item of the stack, %). */
  interface Break { t: number; uid: number; gid: number | null; how: "inventaire" | "runes" | "manuel" | null; first: number; last: number; qty: number | null; focus: number | null; runes: [number, number][] }
  type Pending = Break;
  let pending = $state.raw<Pending[]>([]);
  const runeNames = $derived(new Map(Object.values(pack.runes).map((r) => [r.gid, runeShort(r.name)])));
  const runesText = (p: Pending) => p.runes.map(([g, q]) => `${q} ${runeNames.get(g) ?? "#" + g}`).join(", ");
  const assign = (p: Pending, gid: number | null) => call("brisage", "assign", { uid: p.uid, t: p.t, gid }).catch((e) => (err = String(e)));
  /** Source chosen per item in the detail panel ("craft" or a lot key); default = the best one. */
  let chosen = $state<Record<string, string>>({});

  const price = (gid: number) => prices.price(gid);
  const coefOf = (gid: number): number | null => coefs[gid]?.coef ?? null;

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

  /** Scanned listings and prices change by bursts while the player browses the AH: the list (every breakable item) is rebuilt
   *  at most every `REBUILD_MS` (`dataVersion`), and only the items whose listing changed are recomputed, unless a price
   *  changed (`priceVersion`: rune and ingredient prices count for every item). */
  const REBUILD_MS = 300;
  let dataVersion = $state(0);
  let priceVersion = 0;
  let rebuild: ReturnType<typeof setTimeout> | null = null;
  const changed = () => (rebuild ??= setTimeout(() => ((rebuild = null), dataVersion++), REBUILD_MS));
  const srcCache = new Map<number, { listing: unknown; pv: number; recipes: Recipes; pack: BrisPack; sources: Source[] }>();
  function cachedSources(i: BrisItem): Source[] {
    const listing = auction.items.get(i.gid);
    const c = srcCache.get(i.gid);
    if (c && c.listing === listing && c.pv === priceVersion && c.recipes === recipes && c.pack === pack) return c.sources;
    const sources = sourcesOf(i);
    srcCache.set(i.gid, { listing, pv: priceVersion, recipes, pack, sources });
    return sources;
  }

  /** What is on sale per item type, from the AH categories opened (`auction.on_sale`). */
  interface OnSale { type: number; t: number; gids: number[] }
  let onSale = $state.raw<Record<string, OnSale>>({});
  const saleSets = $derived(new Map(Object.values(onSale).map((o) => [o.type, new Set(o.gids)])));
  /** On sale in the AH: true / false, null while its category was never opened. */
  const saleOf = (i: BrisItem) => saleSets.get(i.type_id)?.has(i.gid) ?? null;

  /** A source with an exo: its runes count without focus (an item worth buying when it is cheap enough). */
  const hasExo = (s: Source | null) => !!s?.out.lines.some((l) => l.exo);
  const profitAt = (s: Source, coef: number) => (s.cost != null ? (s.out.best.value * coef) / 100 - s.cost : null);

  type SortKey = "name" | "level" | "sale" | "cost" | "even" | "profit";
  interface Row { item: BrisItem; gid: number; name: string; level: number; best: Source | null; cost: number | null; even: number | null; profit: number | null; coef: number | null; sale: boolean | null; good: number; goodExo: number }

  const rows = $derived.by<Row[]>(() => {
    void dataVersion; // listings / prices: through `changed`, never one dependency per item read
    const items = pack.items;
    void recipes;
    const all = untrack(() => items.map((item) => [item, cachedSources(item)] as const));
    return all.map(([item, sources]) => {
      const best = sources[0] ?? null;
      const coef = coefOf(item.gid);
      const profit = best && coef != null ? profitAt(best, coef) : null;
      // scanned lots that pay at the known coefficient (an exo makes a lot worth more without focus)
      const good = coef == null ? [] : sources.filter((s) => s.kind === "lot" && (profitAt(s, coef) ?? 0) > 0);
      return { item, gid: item.gid, name: item.name, level: item.level, best, cost: best?.cost ?? null, even: best?.even ?? null, profit, coef, sale: saleOf(item), good: good.length, goodExo: good.filter(hasExo).length };
    });
  });

  // ---- filters ----
  /** Breakable types, weapons and worn equipment apart. */
  const typeGroups = $derived.by(() => {
    const by = new Map<number, string>();
    for (const i of pack.items) by.set(i.type_id, i.type);
    const list = [...by].map(([id, name]) => ({ id, name })).sort((a, b) => a.name.localeCompare(b.name, "fr"));
    return [
      { label: "Équipements", types: list.filter((t) => !WEAPONS.has(t.id)) },
      { label: "Armes", types: list.filter((t) => WEAPONS.has(t.id)) },
    ];
  });
  const typeSet = $derived(new Set(ui.types));
  const toggleType = (id: number) => (ui.types = typeSet.has(id) ? ui.types.filter((t) => t !== id) : [...ui.types, id]);
  const toggleGroup = (ids: number[]) => {
    const all = ids.every((id) => typeSet.has(id));
    ui.types = all ? ui.types.filter((t) => !ids.includes(t)) : [...new Set([...ui.types, ...ids])];
  };
  const typeLabel = $derived.by(() => {
    if (!ui.types.length) return "Tous les types";
    const names = typeGroups.flatMap((g) => g.types).filter((t) => typeSet.has(t.id)).map((t) => t.name);
    return names.length <= 2 ? names.join(", ") : `${names.length} types`;
  });
  let typesOpen = $state(false);
  let typesEl = $state<HTMLElement | null>(null);
  const closeTypes = (e: PointerEvent) => typesOpen && typesEl && !typesEl.contains(e.target as Node) && (typesOpen = false);
  const num = (v: string) => (v.trim() === "" || !Number.isFinite(+v) ? null : +v);
  const filtered = $derived(!!(ui.q || ui.types.length || ui.lvMin != null || ui.lvMax != null || ui.noDrop || ui.absent));
  const resetFilters = () => Object.assign(ui, { q: "", types: [], lvMin: null, lvMax: null, noDrop: false, absent: false });

  const sort = useSort<SortKey>("even", false, (k) => k === "level" || k === "profit" || k === "sale");
  const view = $derived.by(() => {
    const m = matcher(ui.q);
    return sort.apply(
      rows.filter(
        (r) =>
          m(r.name) &&
          (!ui.types.length || typeSet.has(r.item.type_id)) &&
          (ui.lvMin == null || r.level >= ui.lvMin) &&
          (ui.lvMax == null || r.level <= ui.lvMax) &&
          (!ui.noDrop || !r.item.drop) &&
          (!ui.absent || r.sale === false) &&
          (!ui.priced || r.even != null),
      ),
    );
  });

  const COLUMNS: { key: SortKey; label: string; num?: boolean; title?: string }[] = [
    { key: "name", label: "Objet" },
    { key: "level", label: "Niv.", num: true },
    { key: "sale", label: "HDV", title: "En vente à l'hôtel de vente quand sa catégorie a été ouverte pour la dernière fois (? = catégorie jamais ouverte)" },
    { key: "cost", label: "Prix", num: true, title: "Craft (prix enregistrés des ingrédients) ou lot le plus intéressant à l'HDV" },
    { key: "even", label: "Coef. rentable", num: true, title: "Coefficient à partir duquel le brisage (meilleur résultat, avec ou sans focus) rembourse l'objet" },
    { key: "profit", label: "Bénéfice", num: true, title: "Au coefficient connu de cet objet (relevé au brisage ou saisi)" },
  ];

  /** Kamas rounded to the unit (rune values are fractional). */
  const k0 = (n: number) => kamas(Math.round(n));
  const focusName = (o: Outcome) => (o.best.focus == null ? "sans focus" : `focus ${runeShort(o.lines.find((l) => l.a === o.best.focus)?.rune.name)}`);
  const saleTitle = (r: Row) => {
    const o = onSale[r.item.type_id];
    return o ? `Catégorie « ${r.item.type} » ouverte le ${fmtDateTime(o.t)} (${ago(o.t)}) : ${o.gids.length} objets en vente` : `Ouvrez la catégorie « ${r.item.type} » à l'HDV`;
  };
  const coefText = (c: number | null) => (c == null ? "—" : `${fmtNum(c, c < 10 ? 1 : 0)} %`);

  /** Row clicked: select it and copy its name (to paste in the auction house search). */
  let copied = $state<number | null>(null);
  function pick(r: Row) {
    ui.sel = r.gid;
    navigator.clipboard?.writeText(r.name).then(() => {
      copied = r.gid;
      setTimeout(() => copied === r.gid && (copied = null), 900);
    }, () => {});
  }

  /** Break history of the selected item, latest first. */
  let breaks = $state.raw<Break[]>([]);
  const loadBreaks = (gid: number | null) => {
    if (gid == null) return void (breaks = []);
    call<Break[]>("brisage", "breaks", { gid }).then((l) => ui.sel === gid && (breaks = l.reverse())).catch(() => {});
  };
  $effect(() => loadBreaks(ui.sel));
  const runeName = (a: number | null) => (a == null ? "" : (runeShort(pack.runes[a]?.name) ?? "#" + a));
  const runeTotal = (b: Break) => b.runes.reduce((s, [, q]) => s + q, 0);

  // ---- detail ----
  const sel = $derived(rows.find((r) => r.gid === ui.sel) ?? null);
  const sources = $derived(sel ? sourcesOf(sel.item) : []);
  const src = $derived(sources.find((s) => s.key === chosen[ui.sel ?? ""]) ?? sources[0] ?? null);
  /** Without any source (no recipe price, nothing scanned): the mean roll, priced at nothing. */
  const out = $derived(src?.out ?? (sel ? outcome(meanRoll(sel.item), sel.level, pack.runes, price) : null));
  const selCoef = $derived(sel ? coefs[sel.gid] ?? null : null);
  const coef = $derived(selCoef?.coef ?? 100);
  const k = $derived(coef / 100);
  const setCoef = (gid: number, v: string) => {
    const n = parseFloat(v.replace(",", "."));
    const c = Number.isFinite(n) && n >= 0 ? n : null;
    if (c != null) coefs[gid] = { gid, coef: c, t: Date.now() / 1000, source: "manuel" };
    else delete coefs[gid];
    call("brisage", "set_coef", { gid, coef: c }).catch((e) => (err = String(e)));
  };
  const srcLabel = (s: Source) => (s.kind === "craft" ? `Craft (jet moyen) · ${s.cost != null ? kamas(s.cost) : "prix inconnu"}` : `Lot HDV · ${kamas(s.cost!)}`) + ` · rentable à ${coefText(s.even)}`;

  onMount(() => {
    cached<BrisPack>("brisage", "items").then((p) => (pack = p)).catch((e) => (err = String(e)));
    cached<Recipes>("auction", "recipes").then((r) => (recipes = r)).catch((e) => (err = String(e)));
    const offEvents = listenAll({
      "brisage:pending": (l: Pending[]) => (pending = l),
      "brisage:break": (b: Break) => b.gid === ui.sel && loadBreaks(ui.sel),
      "brisage:coef": (e: Coef | { gid: number; coef: null }) => {
        if (e.coef == null) delete coefs[e.gid];
        else coefs[e.gid] = e as Coef;
      },
      "auction:on_sale": (o: OnSale) => (onSale = { ...onSale, [o.type]: o }),
    });
    call<Pending[]>("brisage", "pending").then((l) => (pending = l)).catch(() => {});
    call<Record<string, Coef>>("brisage", "coefs")
      .then(async (c) => {
        Object.assign(coefs, c);
        // coefficients typed with the first version of the tab (browser storage) move to the backend
        const old = readJSON<Record<string, number>>(OLD_COEFS_KEY, {});
        for (const [g, v] of Object.entries(old)) if (!c[g]) await call("brisage", "set_coef", { gid: +g, coef: v });
        removeKey(OLD_COEFS_KEY);
      })
      .catch((e) => (err = String(e)));
    void prices.ensure();
    void auction.ensure();
    const offPrices = prices.subscribe(() => (priceVersion++, changed()));
    const offAuction = auction.subscribe(changed);
    call<Record<string, OnSale>>("auction", "on_sale").then((o) => (onSale = { ...o, ...onSale })).catch(() => {});
    return () => {
      offEvents();
      offPrices();
      offAuction();
      if (rebuild) clearTimeout(rebuild);
    };
  });
</script>

<svelte:window onpointerdown={closeTypes} />

<div class="view cgrid" class:anim={!cols.dragging} style:grid-template-columns="1fr 5px {cols.px("detail")}px">
  <div class="list">
    <div class="fh">Brisage</div>
    <SearchBar bind:value={ui.q} bind:priced={ui.priced} placeholder="Rechercher un objet…">
      {#snippet before()}
        <div class="types" bind:this={typesEl}>
          <button class="tbtn" class:on={ui.types.length > 0} onclick={() => (typesOpen = !typesOpen)}>{typeLabel} ▾</button>
          {#if typesOpen}
            <div class="tpop">
              <div class="trow top">
                <button class="mini" onclick={() => (ui.types = [])}>Tous</button>
              </div>
              {#each typeGroups as g (g.label)}
                {@const ids = g.types.map((t) => t.id)}
                <label class="tgroup"><input type="checkbox" checked={ids.every((id) => typeSet.has(id))} indeterminate={ids.some((id) => typeSet.has(id)) && !ids.every((id) => typeSet.has(id))} onchange={() => toggleGroup(ids)} /> {g.label}</label>
                <div class="tlist">
                  {#each g.types as t (t.id)}
                    <label><input type="checkbox" checked={typeSet.has(t.id)} onchange={() => toggleType(t.id)} /> {t.name}</label>
                  {/each}
                </div>
              {/each}
            </div>
          {/if}
        </div>
        <span class="lv">Niv. <input type="number" min="1" max="200" placeholder="min" value={ui.lvMin ?? ""} oninput={(e) => (ui.lvMin = num(e.currentTarget.value))} /> – <input type="number" min="1" max="200" placeholder="max" value={ui.lvMax ?? ""} oninput={(e) => (ui.lvMax = num(e.currentTarget.value))} /></span>
      {/snippet}
      {#snippet after()}
        <label title="Objets lâchés par un monstre"><input type="checkbox" bind:checked={ui.noDrop} /> Masquer les objets droppables</label>
        <label title="Objets absents de l'hôtel de vente quand leur catégorie a été ouverte (ouvrez les catégories à l'HDV pour les relever)"><input type="checkbox" bind:checked={ui.absent} /> Absents de l'HDV seulement</label>
        {#if filtered}<button class="mini" onclick={resetFilters}>Effacer les filtres</button>{/if}
      {/snippet}
    </SearchBar>
    {#if err}<Banner inset>Erreur : {err} (relancez l'application pour charger la dernière version)</Banner>{/if}
    <SmoothScroll>
      <SortTable columns={COLUMNS} rows={view.slice(0, SHOWN)} {sort} key={(r) => r.gid} rowProps={(r) => ({ class: [(r.profit ?? 0) > 0 ? "hot" : "", r.gid === ui.sel ? "sel" : ""].join(" "), onclick: () => pick(r) })}>
        {#snippet row(r)}
          <td><b>{r.name}</b>{#if copied === r.gid} <span class="copied">copié</span>{/if} <span class="mut">{r.item.type}</span>{#if r.item.drop} <span class="mut" title="Lâché par un monstre">· drop</span>{/if}</td>
          <td class="n">{r.level}</td>
          <td title={saleTitle(r)}>{#if r.sale === true}<span class="mut">en vente</span>{:else if r.sale === false}<b class="good">absent</b>{:else}<span class="mut">?</span>{/if}</td>
          <td class="n" title={r.best?.kind === "craft" && r.best.missing?.length ? "Prix manquant : " + r.best.missing.join(", ") : ""}>
            {#if r.cost != null}{kamas(r.cost)} <span class="mut">{r.best?.kind === "lot" ? "HDV" : "craft"}</span>{#if r.best?.kind === "lot" && hasExo(r.best)} <span class="exo" title="Le lot a un exo : ses runes comptent sans focus">exo</span>{/if}{:else}—{/if}
          </td>
          <td class="n" title={r.best ? `Meilleur résultat : ${focusName(r.best.out)}` : ""}><b>{coefText(r.even)}</b>{#if r.best?.out.unpriced} <span class="warn" title="Une rune n'a pas de prix (comptée 0)">⚠</span>{/if}</td>
          <td class="n" class:good={(r.profit ?? 0) > 0} class:bad={(r.profit ?? 0) < 0}>{#if r.profit != null}{signedKamas(Math.round(r.profit))} <span class="mut">à {coefText(r.coef)}</span>{:else}—{/if}{#if r.good}<br /><span class="good small" title="Lots scannés rentables au coefficient connu">{r.good} lot{r.good > 1 ? "s" : ""} rentable{r.good > 1 ? "s" : ""}{r.goodExo ? ` (${r.goodExo} avec exo)` : ""}</span>{/if}</td>
        {/snippet}
        {#snippet empty()}Aucun objet à afficher : ouvrez les objets (ou leurs ingrédients) et les runes à l'hôtel de vente pour enregistrer leurs prix (ou désactivez « Prix connus seulement »).{/snippet}
      </SortTable>
      {#if view.length > SHOWN}<div class="mut more">{view.length - SHOWN} objets de plus : affinez la recherche.</div>{/if}
    </SmoothScroll>
  </div>

  <Grip {cols} name="detail" dir={-1} />

  <div class="detail" class:cz={cols.collapsed.detail}>
    <PanelHead {cols} name="detail" title="Détail" dir={-1} />
    {#each pending as p (p.uid + ":" + p.t)}
      <div class="pending">
        <div><b>Brisage non reconnu</b> <span class="mut">{fmtDateTime(p.t)}</span> · coefficient <b>{coefText(p.last)}</b></div>
        <div class="mut">{runesText(p)}</div>
        <div class="pbtns">
          <button disabled={!sel} onclick={() => sel && assign(p, sel.gid)}>{sel ? `Attribuer à ${sel.name}` : "Choisissez l'objet brisé dans la liste"}</button>
          <button class="mini" onclick={() => assign(p, null)}>Ignorer</button>
        </div>
      </div>
    {/each}
    {#if sel && out}
      <div class="fh">{sel.name} <span class="mut">· niv. {sel.level} · {sel.item.type}{sel.item.drop ? " · drop" : ""}{sel.item.craft ? " · craftable" : ""}</span></div>
      <div class="ctl">
        <label>Coefficient <input type="number" min="0" step="1" value={selCoef?.coef ?? ""} placeholder="100" oninput={(e) => setCoef(sel.gid, e.currentTarget.value)} /> %</label>
        {#if selCoef}
          <span class="mut" title={selCoef.guessed ? "Objet reconnu d'après les runes obtenues (absent de l'inventaire lu)" : ""}>
            {selCoef.source === "brisage" ? "brisé le" : "saisi le"} {fmtDateTime(selCoef.t)} ({ago(selCoef.t)}){selCoef.guessed ? " ?" : ""}
          </span>
        {/if}
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
              <td title={l.rune.label ?? ""}>{runeShort(l.rune.name)}{#if l.exo} <span class="exo" title="Exo : donne ses runes mais ne peut pas être focus">exo</span>{/if}</td>
              <td class="n">{fmtNum(l.value, 1)}</td>
              <td class="n">{#if l.price != null}{kamas(l.price)}{:else}<span class="warn" title="Ouvrez la rune à l'HDV">?</span>{/if}</td>
              <td class="n">{fmtNum(l.count * k)}</td>
              <td class="n">{k0(afterTax(l.count * (l.price ?? 0)) * k)}</td>
              {#if l.exo}<td class="n mut">—</td><td class="n mut">—</td>
              {:else}<td class="n">{fmtNum(l.focus * k)}</td>
              <td class="n"><b>{k0(out.focus[i] * k)}</b></td>{/if}
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

      {#if breaks.length}
        <div class="fh hist">Brisages enregistrés</div>
        <table class="tbl runes">
          <thead><tr><th>Date</th><th class="n" title="Coefficient du premier et du dernier objet du lot brisé">Coefficient</th><th class="n" title="Taille du lot dans l'inventaire (si l'objet y a été vu)">Qté</th><th>Focus</th><th class="n">Runes</th></tr></thead>
          <tbody>
            {#each breaks as b (b.uid + ":" + b.t)}
              <tr title={b.how === "runes" ? "Objet reconnu d'après les runes" : b.how === "manuel" ? "Objet attribué à la main" : ""}>
                <td>{fmtDateTime(b.t)}</td>
                <td class="n">{b.first !== b.last ? `${fmtNum(b.first)} → ` : ""}<b>{fmtNum(b.last)} %</b></td>
                <td class="n">{b.qty ?? "?"}</td>
                <td>{runeName(b.focus) || "—"}</td>
                <td class="n">{runeTotal(b)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    {:else}
      <div class="mut">Choisissez un objet dans la liste.</div>
    {/if}
  </div>
</div>

<style>
  .view { flex: 1; min-height: 0; display: grid; font-family: var(--font); }
  .list { min-width: 0; min-height: 0; display: flex; flex-direction: column; padding: 8px 10px 0; }
  .list > :global(.wrap) { flex: 1; }
  /* no top padding in the scroller: the sticky header would stick below it and rows would show through the gap */
  .list :global(.scroller) { padding-top: 0; }
  .list :global(.tbl th) { z-index: 1; }
  .list :global(tbody tr) { cursor: pointer; }
  .list :global(tr.sel td) { box-shadow: inset 0 1px 0 var(--acc), inset 0 -1px 0 var(--acc); }
  .more { padding: 8px 6px; }
  .types { position: relative; }
  .tbtn { padding: 3px 8px; min-width: 130px; text-align: left; }
  .tbtn.on { border-color: var(--acc); }
  .tpop { position: absolute; top: calc(100% + 4px); left: 0; z-index: 20; background: var(--panel); border: 1px solid var(--line); box-shadow: 0 4px 16px #0008; padding: 6px 10px 8px; min-width: 280px; display: flex; flex-direction: column; gap: 4px; }
  .tpop label { display: flex; align-items: center; gap: 6px; white-space: nowrap; cursor: pointer; }
  .tgroup { font-weight: 600; margin-top: 4px; }
  .tlist { display: grid; grid-template-columns: 1fr 1fr; gap: 2px 12px; padding-left: 18px; }
  .lv { display: inline-flex; align-items: center; gap: 4px; }
  .lv input { width: 56px; padding: 3px 6px; }
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
  .copied { color: var(--ok); font-size: 10px; }
  .hist { margin-top: 12px; }
  .pending { border: 1px solid var(--warn); background: color-mix(in srgb, var(--warn) 10%, transparent); padding: 6px 8px; margin-bottom: 10px; display: flex; flex-direction: column; gap: 4px; }
  .pbtns { display: flex; gap: 8px; align-items: center; }
  .exo { color: var(--acc); font-size: 10px; }
</style>
