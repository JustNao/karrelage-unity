<script lang="ts">
  import { onMount, tick } from "svelte";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";
  import { ago, call, kamas, on } from "../../lib/api";
  import { flip } from "svelte/animate";
  import DensityChart from "../../lib/components/DensityChart.svelte";
  import EffectPicker from "../../lib/components/EffectPicker.svelte";
  import ItemCard from "../../lib/components/ItemCard.svelte";
  import SmoothScroll from "../../lib/components/SmoothScroll.svelte";
  import { LEVEL_EFFECT, isLegendary, levelOf, xpOfLevel } from "../../lib/companion";
  import { analyse, isGoodDeal } from "../../lib/density";
  import { norm, rangeLabel, sections, type EffectMeta } from "../../lib/effects";
  import { AH_TAX, targetKey, transcendance, type Rune, type StoredPrice } from "../../lib/transcend";

  interface Filter { a: number; min: string | number; max: string | number }

  let items = new SvelteMap<number, any>();
  let selGid = $state<number | null>(null);
  let picker = $state<EffectMeta[]>([]); // effects found on level 150-200 equipment (what the picker offers)
  let meta = $state<Record<number, EffectMeta>>({}); // every labelled effect (labels/groups of whatever shows up in a lot)
  let filters = $state<Record<number, Filter[]>>({}); // gid -> filters
  let openSet = new SvelteSet<string | number>(); // lot keys
  let picked = $state<string | number | null>(null); // lot chosen from the graph
  let fade = $state<number | null>(null); // effect id: lots without it are faded out (list, graph, good deals)

  $effect(() => {
    selGid; // another item: forget the fade and the rune focus
    fade = null;
    focus = null;
    cand = null;
  });
  // Pinned lot (kept across items): every other lot shown gets a diff against it.
  let pin = $state.raw<{ item: any; row: any } | null>(null);
  const isPin = (r: any) => !!pin && !!item && pin.item.gid === item.gid && pin.row.uid === r.uid;
  const togglePin = (r: any) => (pin = isPin(r) ? null : { item, row: r });
  /** What a lot is compared with: the pinned lot (never shown against itself). */
  const versusFor = (r: any) => (pin && !comp && !isPin(r) ? { pin: pin.row, name: pin.item.name, unitPrice } : null);
  const lacksFade = (r: any) => fade !== null && !r.effects.some((e: any) => e.action === fade && e.status !== "removed");
  const toggleFade = (a: number) => (fade = fade === a ? null : a);
  let tab = $state<"listing" | "trans">("listing");
  let dealsOpen = $state(false); // "Good deals" starts collapsed
  let cand = $state<{ id: string | number; a: number; value: number } | null>(null); // selected candidate: its point + the lots with the same rune applied stand out
  let focus = $state<string | null>(null); // transcended effect (targetKey) the candidates are limited to
  let runes = $state.raw<Rune[]>([]);
  let stored = new SvelteMap<number, StoredPrice>(); // stored prices (the rune prices feed the transcendance tab)
  let allOpen = $state(false);
  let sortDir = $state(1);

  let legendOnly = $state(false); // companions: only legendary ones
  const list = $derived([...items.values()].sort((a, b) => b.updated - a.updated));
  /** Left panel: items grouped by type, each group collapsible (collapsed groups are remembered). */
  const COLLAPSE_KEY = "loutil.collapsedTypes";
  const collapsed = new SvelteSet<string>();
  try {
    JSON.parse(localStorage.getItem(COLLAPSE_KEY) ?? "[]").forEach((t: string) => collapsed.add(t));
  } catch {}
  function toggleType(t: string) {
    collapsed.has(t) ? collapsed.delete(t) : collapsed.add(t);
    try {
      localStorage.setItem(COLLAPSE_KEY, JSON.stringify([...collapsed]));
    } catch {}
  }
  let search = $state(""); // left list: filter by item or type name
  const typeGroups = $derived.by(() => {
    const m = new Map<string, any[]>();
    const n = norm(search.trim());
    for (const i of list) {
      if (n && !norm(i.name).includes(n) && !norm(i.type ?? "").includes(n)) continue;
      const t = i.type ?? "?";
      if (!m.has(t)) m.set(t, []);
      m.get(t)!.push(i);
    }
    return [...m.entries()].sort((a, b) => a[0].localeCompare(b[0], "fr")).map(([type, items]) => ({ type, items }));
  });
  const item = $derived(selGid != null ? items.get(selGid) : undefined);

  /** Familier / montilier listing: tracked by level (price follows the xp curve), no stats / pin / transcendance. */
  const comp = $derived(!!item?.companion);
  const effLabel = (a: number) => meta[a]?.label ?? `effect ${a}`;
  const unitPrice = (r: any) => {
    const v = r.prices.filter((p: any) => p.price > 0).map((p: any) => p.price / p.qty);
    return v.length ? Math.min(...v) : Infinity;
  };
  const lotVal = (r: any, a: number) => Math.max(0, ...r.effects.filter((e: any) => e.action === a && e.status !== "removed").map((e: any) => e.hi ?? 0));
  const flist = (gid: number) => filters[gid] ?? [];

  const lotKey = (r: any, i: number) => r.uid ?? `n${i}`;

  /** Effects the item has by default (filterable ones only), then the ones the user added himself. */
  const effectRows = $derived.by(() => {
    if (!item) return [];
    const seen = new Map<number, { a: number; def: number[] | null }>();
    for (const d of item.defaults ?? []) if (meta[d.a]?.filterable) seen.set(d.a, { a: d.a, def: [d.min, d.max] });
    for (const x of flist(item.gid)) if (!seen.has(x.a)) seen.set(x.a, { a: x.a, def: null });
    return [...seen.values()];
  });

  /** Weapon-damage effects are only offered for weapons (items whose default effects include weapon hits). */
  const isWeapon = $derived(!!item && (item.defaults ?? []).some((d: any) => meta[d.a]?.group === "weapon"));
  const pickable = $derived(picker.filter((e) => isWeapon || e.group !== "weapon"));

  /** Filter rows grouped like the in-game characteristics panel. */
  const groups = $derived(
    sections(effectRows.map((x) => ({ ...x, group: meta[x.a]?.group ?? "other", order: meta[x.a]?.order ?? 9999, label: effLabel(x.a) }))),
  );

  const rows = $derived.by(() => {
    if (!item) return [];
    const f = flist(item.gid).filter((x) => x.min !== "" || x.max !== "");
    return item.rows
      .filter((r: any) => !(comp && legendOnly) || isLegendary(r))
      .filter((r: any) => f.every((x) => {
        const v = lotVal(r, x.a);
        return (x.min === "" || v >= +x.min) && (x.max === "" || v <= +x.max);
      }))
      .sort((p: any, q: any) => {
        const a = unitPrice(p), b = unitPrice(q);
        return a === b ? 0 : a === Infinity ? 1 : b === Infinity ? -1 : (a - b) * sortDir;
      });
  });

  function filterOf(gid: number, a: number): Filter {
    filters[gid] ??= [];
    const f = filters[gid]; // re-read: `??=` yields the raw array, not the reactive proxy
    let x = f.find((y) => y.a === a);
    if (!x) {
      f.push({ a, min: "", max: "" });
      x = f[f.length - 1]; // the reactive proxy, so later edits trigger updates
    }
    return x;
  }
  const active = (a: number) => {
    const y = item && flist(item.gid).find((z) => z.a === a);
    return !!y && (y.min !== "" || y.max !== "");
  };

  function preset(p: string) {
    if (!item) return;
    if (p === "clear") return void (filters[item.gid] = []);
    for (const x of effectRows) {
      if (!x.def) continue; // presets only touch the item's default effects (perfect = default max)
      const y = filterOf(item.gid, x.a);
      if (p === "min") y.min = x.def[0];
      else if (p === meta[x.a]?.group) y.min = x.def[1];
    }
  }

  /** Fill the min field of one effect: the low end of its default range (or the lowest value seen in the lots). */
  function setMin(a: number, def: number[] | null) {
    if (!item) return;
    const seen = item.rows.map((r: any) => lotVal(r, a)).filter((v: number) => v > 0);
    const v = def ? def[0] : seen.length ? Math.min(...seen) : null;
    if (v !== null) filterOf(item.gid, a).min = v;
  }

  /** Fill the min field with the top of the default range (or the highest value seen): only perfect rolls remain. */
  function setPerf(a: number, def: number[] | null) {
    if (!item) return;
    const seen = item.rows.map((r: any) => lotVal(r, a)).filter((v: number) => v > 0);
    const v = def ? def[1] : seen.length ? Math.max(...seen) : null;
    if (v !== null) filterOf(item.gid, a).min = v;
  }

  function clearEffect(a: number) {
    if (!item) return;
    const y = filterOf(item.gid, a);
    y.min = "";
    y.max = "";
  }

  function toggleOpen(key: string | number) {
    openSet.has(key) ? openSet.delete(key) : openSet.add(key);
  }

  /** Graph dots: price (per unit) against the lot's density. Same lots as the list, so every dot has a card. */
  /** Rune focus (Transcendance tab): the graph fades the lots that lack the focused effect (like the highlight). */
  const focusEffect = $derived(tab === "trans" && !comp && focus ? Number(focus.split(":")[0]) : null);
  const points = $derived(
    rows
      .map((r: any, i: number) => {
        const sameRune = cand && (lotKey(r, i) === cand.id || (r.locked && r.effects.some((e: any) => e.status === "added" && e.action === cand!.a && e.hi === cand!.value)));
        const has = cand ? !!sameRune : focusEffect === null || r.effects.some((e: any) => e.action === focusEffect && e.status !== "removed");
        return {
        id: lotKey(r, i),
        x: unitPrice(r),
        y: comp ? (levelOf(r) != null ? xpOfLevel(levelOf(r)!) : NaN) : r.density, // companions: total xp of the level (price follows it)
        label: r.name,
        added: comp ? isLegendary(r) : r.effects.some((e: any) => e.status === "added"), // companions: blue dot = legendary
        missing: !comp && r.effects.some((e: any) => e.status === "removed"),
        faded: lacksFade(r) || !has,
      };
      })
      .filter((p: any) => Number.isFinite(p.x) && Number.isFinite(p.y)),
  );

  /** Lots standing out above the price trend per scanned item (all its lots, unfiltered): the green icon in the left list. */
  const dealCounts = $derived.by(() => {
    const m = new Map<number, number>();
    for (const it of items.values()) {
      const pts = it.rows
        .map((r: any) => ({ x: unitPrice(r), y: it.companion ? (levelOf(r) != null ? xpOfLevel(levelOf(r)!) : NaN) : r.density }))
        .filter((p: any) => Number.isFinite(p.x) && Number.isFinite(p.y));
      if (pts.length < 5) continue;
      const fit = analyse(pts);
      const n = pts.filter((_: any, i: number) => isGoodDeal(fit, i, pts.length)).length;
      if (n) m.set(it.gid, n);
    }
    return m;
  });

  /** Lots far above the price trend (same rule as the green dots), best first. */
  const deals = $derived.by(() => {
    if (points.length === 0) return [];
    const fit = analyse(points);
    const byKey = new Map(rows.map((r: any, i: number) => [lotKey(r, i), r]));
    return points
      .map((p: any, i: number) => ({ key: p.id, row: byKey.get(p.id), z: fit.z[i], good: isGoodDeal(fit, i, points.length) }))
      .filter((d: any) => d.good && d.row)
      .sort((a: any, b: any) => b.z - a.z)
      .slice(0, 6);
  });

  // ---- saved transcendance candidates: a snapshot kept across scans (the lot may be sold by the time you come back)
  interface Saved { key: string; gid: number; itemName: string; savedAt: number; row: any; label: string; value: number; rune: string; buy: number; runeCost: number; sale: number; profit: number }
  const SAVED_KEY = "loutil.savedCandidates";
  let saved = $state.raw<Saved[]>([]);
  try {
    saved = JSON.parse(localStorage.getItem(SAVED_KEY) ?? "[]");
  } catch {}
  const persistSaved = (l: Saved[]) => {
    saved = l;
    try {
      localStorage.setItem(SAVED_KEY, JSON.stringify(l));
    } catch {}
  };
  const candKey = (o: any) => `${item.gid}:${o.row.uid ?? o.buy + "-" + o.row.density}`;
  const isSaved = (o: any) => saved.some((s) => s.key === candKey(o));
  function toggleSaved(o: any) {
    const key = candKey(o);
    if (isSaved(o)) return persistSaved(saved.filter((s) => s.key !== key));
    const t = o.best.target;
    persistSaved([
      { key, gid: item.gid, itemName: item.name, savedAt: Date.now() / 1000, row: o.row, label: effLabel(t.a), value: t.value, rune: t.rune?.name ?? "?", buy: o.buy, runeCost: t.runeCost ?? 0, sale: t.sale, profit: o.best.profit },
      ...saved,
    ]);
  }
  /** Is the saved lot still in the latest scan of its item? null = the item has not been scanned (here) since. */
  const stillListed = (sv: Saved) => {
    const it = items.get(sv.gid);
    return it && sv.row.uid != null ? it.rows.some((r: any) => r.uid === sv.row.uid) && it.updated > sv.savedAt : null;
  };
  const trans = $derived(transcendance(comp ? [] : rows, unitPrice, runes, stored, focus));
  const runeName = (n: string) => n;

  /** Clicking a dot: unroll the lot and scroll the list down to it (nothing is filtered out). */
  async function focusLot(id: string | number) {
    picked = id;
    openSet.add(id);
    tab = "listing"; // from the Transcendance tab: switch so the lot is rendered
    await tick();
    document.getElementById(`lot-${id}`)?.scrollIntoView({ behavior: "smooth", block: "center" });
  }
  async function forget() {
    if (confirm("Oublier tous les objets enregistrés ?")) await call("auction", "clear");
  }
  function put(i: any) {
    items.set(i.gid, i);
    if (selGid === null) selGid = i.gid;
  }

  /** An item just captured from the game: show it (a refresh of the item already open keeps its scroll position). */
  async function showNew(i: any) {
    put(i);
    if (selGid === i.gid) return;
    selGid = i.gid;
    picked = null;
    await tick();
    document.querySelector(".view .scroller")?.scrollTo({ top: 0 });
  }

  onMount(() => {
    call<EffectMeta[]>("auction", "effects").then((e) => (picker = e));
    call<EffectMeta[]>("auction", "effect_meta").then((l) => (meta = Object.fromEntries(l.map((e) => [e.id, e]))));
    call<Rune[]>("prices", "runes").then((l) => (runes = l));
    call<StoredPrice[]>("prices", "list").then((l) => l.forEach((p) => stored.set(p.gid, p)));
    call<any[]>("auction", "items").then((l) => {
      l.forEach(put);
      if (selGid === null && l.length) selGid = l[0].gid;
    });
    const offs = [
      on("auction:item", showNew),
      on<StoredPrice>("prices:price", (p) => stored.set(p.gid, p)),
      on("prices:cleared", () => stored.clear()),
      on("auction:cleared", () => {
        items.clear();
        selGid = null;
      }),
    ];
    return () => void offs.forEach((o) => o.then((f) => f()));
  });
</script>

<div class="view">
  <div class="ilist">
    <div class="sbar">
      <input type="search" placeholder="Rechercher un objet…" bind:value={search} />
      <button class="mini" disabled={!search} onclick={() => (search = "")}>Effacer</button>
    </div>
    {#each typeGroups as g (g.type)}
      {@const shut = collapsed.has(g.type)}
      <button class="thead" aria-expanded={!shut} onclick={() => toggleType(g.type)}>
        <span class="chev" class:shut>▾</span> {g.type} <span class="mut">{g.items.length}</span>
      </button>
      {#if !shut}
        {#each g.items as i (i.gid)}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="irow" class:sel={selGid === i.gid} onclick={() => (selGid = i.gid)}>
            <b>{i.name}</b> <span class="mut">niv. {i.level ?? "?"}</span>
            {#if dealCounts.get(i.gid)}<span class="deal-ico" title="{dealCounts.get(i.gid)} {dealCounts.get(i.gid) === 1 ? 'bonne affaire' : 'bonnes affaires'} dans cette liste">●</span>{/if}<br />
            <span class="mut">{i.lots} lots · {i.min_price ? "min " + kamas(i.min_price) : "—"} · {ago(i.updated)}{i.demo ? " · demo" : ""}</span>
          </div>
        {/each}
      {/if}
    {:else}
      <div class="mut empty">{search ? "Aucun objet ne correspond à la recherche." : "Ouvrez un objet à l'hôtel de vente : il apparaît ici (dernière liste par objet)."}</div>
    {/each}
  </div>

  <SmoothScroll toTop>
    {#if item}
      <div class="head">
        <div class="fh">{item.name} <span class="mut">{item.type ?? ""} · gid {item.gid} · mis à jour {ago(item.updated)} · {rows.length}/{item.lots} lots</span></div>
        <button class="sort" onclick={() => (sortDir = -sortDir)} title="Cliquer pour inverser l'ordre des prix">
          Prix <span class="arrow" class:desc={sortDir === -1}>↑</span>
        </button>
        <button onclick={() => ((allOpen = !allOpen), openSet.clear())}>{allOpen ? "Tout replier" : "Tout déplier"}</button>
        {#if pin}<button class="on" onclick={() => (pin = null)} title="Arrêter de comparer les lots avec le lot épinglé">Désépingler {pin.item.name}</button>{/if}
        <button onclick={forget}>Oublier tous les objets</button>
      </div>
      {#if points.length >= 3}
        <div class="graph"><DensityChart {points} selected={tab === "trans" && cand ? cand.id : picked} onselect={focusLot} /></div>
      {/if}
      {#if !comp}<div class="tabs">
        <button class:on={tab === "listing"} onclick={() => (tab = "listing")}>Liste</button>
        <button class:on={tab === "trans"} onclick={() => (tab = "trans")}>Transcendance{trans.opportunities.length ? ` · ${trans.opportunities.length}` : ""}</button>
      </div>{/if}
      {#if tab === "trans" && !comp}
        <div class="trans">
          <div class="mut note">
            Les lots de cet objet avec le verrou de transcendance et un EXO montrent ce que vaut une rune. Les candidats sont des lots non verrouillés
            sans effet en sur-max ; profit = prix de vente − {AH_TAX * 100} % de taxe − prix du lot − prix de la rune (onglet Prix enregistrés).
          </div>
          {#if saved.length}
            <div class="ghead">Candidats enregistrés</div>
            {#each saved as sv (sv.key)}
              {@const live = stillListed(sv)}
              <div class="opp">
                <div class="oh">
                  <b class="ok">+{kamas(sv.profit)}</b>
                  <span class="inv" title="Prix du lot + prix de la rune">investissement {kamas(sv.buy + sv.runeCost)}</span>
                  <button class="link" onclick={() => (selGid = sv.gid)}>{sv.itemName}</button>
                  <span>ajouter {sv.rune} (+{sv.value} {sv.label})</span>
                  <span class="mut">achat {kamas(sv.buy)} · rune {kamas(sv.runeCost)} · vente ~{kamas(sv.sale)} · enregistré {ago(sv.savedAt)}</span>
                  {#if live === true}<span class="ok">toujours en vente</span>{:else if live === false}<span class="warn">absent du dernier scan</span>{/if}
                  <button class="mini" onclick={() => persistSaved(saved.filter((x) => x.key !== sv.key))}>Retirer ✕</button>
                </div>
                <ItemCard row={sv.row} collapsible open={openSet.has("s" + sv.key)} ontoggle={() => toggleOpen("s" + sv.key)} />
              </div>
            {/each}
          {/if}
          {#if trans.targets.length === 0}
            <div class="mut">Aucun lot transcendé de cet objet dans la liste : rien à comparer.</div>
          {:else}
            <div class="ghead">Lots transcendés vus</div>
            {#each trans.targets as t (targetKey(t))}
              <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
              <div class="trow" class:foc={focus === targetKey(t)} title="Cliquer pour ne voir que les candidats de cette rune" onclick={() => (focus = focus === targetKey(t) ? null : targetKey(t))}>
                <b>+{t.value} {effLabel(t.a)}</b>
                <span class="mut">{t.lots} {t.lots === 1 ? "lot" : "lots"} · à partir de {kamas(t.sale)} ·</span>
                {#if !t.rune}<span class="warn">aucune rune correspondante</span>
                {:else if t.runeCost == null}<span class="warn">{runeName(t.rune.name)} : prix inconnu, ouvrez-la à l'hôtel de vente</span>
                {:else}<span class="mut">{runeName(t.rune.name)} {kamas(t.runeCost)}</span>
                  {#if t.stale}<span class="warn" title="Ouvrez la rune à l'hôtel de vente pour actualiser son prix">⚠ prix de la rune vieux de plus d'un jour</span>{/if}{/if}
              </div>
            {/each}
            {#if trans.unpricedCount}<div class="mut note">{trans.unpricedCount} lots candidats ne peuvent pas être classés tant que leur rune n'a pas de prix enregistré.</div>{/if}
            <div class="ghead">Candidats rentables{#if focus} <button class="mini" onclick={() => (focus = null)}>Toutes les runes ✕</button>{/if}</div>
            {#each trans.opportunities as o (o.row.uid ?? o.buy + "-" + o.row.density)}
              {@const t = o.best!.target}
              {@const cid = lotKey(o.row, rows.indexOf(o.row))}
              <div class="opp">
                <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
                <div class="oh sel" class:picked={cand?.id === cid} title="Cliquer pour mettre en avant ce lot et les lots avec la même rune sur le graphique" onclick={() => (cand = cand?.id === cid ? null : { id: cid, a: t.a, value: t.value })}>
                  <b class="ok">+{kamas(o.best!.profit)}</b>
                  <span class="inv" title="Prix du lot + prix de la rune">investissement {kamas(o.buy + (t.runeCost ?? 0))}</span>
                  <span>ajouter {t.rune?.name} (+{t.value} {effLabel(t.a)})</span>
                  {#if t.stale}<span class="warn" title="Prix de la rune vieux de plus d'un jour">⚠ prix de rune périmé</span>{/if}
                  <button class="mini" class:on={isSaved(o)} title="Garder ce candidat pour y revenir plus tard" onclick={(ev) => (ev.stopPropagation(), toggleSaved(o))}>{isSaved(o) ? "Enregistré ✓" : "Enregistrer"}</button>
                  <span class="mut">achat {kamas(o.buy)} · rune {kamas(t.runeCost ?? 0)} · vente ~{kamas(t.sale)} · densité {o.z >= 0 ? "+" : ""}{o.z.toFixed(1)}σ</span>
                </div>
                <ItemCard row={o.row} collapsible open={openSet.has("t" + (o.row.uid ?? ""))} ontoggle={() => toggleOpen("t" + (o.row.uid ?? ""))} />
              </div>
            {:else}
              <div class="mut">Aucun candidat rentable pour le moment.</div>
            {/each}
          {/if}
        </div>
      {:else}
      {#if deals.length}
        <div class="deals">
          <button class="dhead" aria-expanded={dealsOpen} onclick={() => (dealsOpen = !dealsOpen)}>
            <span class="chev" class:shut={!dealsOpen}>▾</span> Bonnes affaires <span class="mut">· {deals.length} {deals.length === 1 ? "lot se démarque" : "lots se démarquent"} au-dessus de la tendance des prix</span>
          </button>
          {#each dealsOpen ? deals : [] as d (d.key)}
            {@const alt = trans.alternative(d.row)}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <div class="deal" class:faded={lacksFade(d.row)} title="Cliquer pour aller au lot dans la liste" onclick={() => focusLot(d.key)}>
              {#if alt}
                <div class="warn altw">⚠ Objet similaire réalisable par Transcendance : lot {kamas(alt.buy)} + {alt.rune.name} {kamas(alt.runeCost)} = {kamas(alt.cost)} (économie de {kamas(alt.saving)})</div>
              {/if}
              <ItemCard row={d.row} companion={comp} {fade} onfade={toggleFade} pinned={isPin(d.row)} onpin={comp ? undefined : () => togglePin(d.row)} versus={versusFor(d.row)} />
            </div>
          {/each}
        </div>
      {/if}
      {#each rows as r, i (lotKey(r, i))}
        {@const key = lotKey(r, i)}
        <div id="lot-{key}" class="lot" class:picked={picked === key} class:faded={lacksFade(r)} animate:flip={{ duration: 320 }}>
          <ItemCard row={r} companion={comp} collapsible open={allOpen || openSet.has(key)} ontoggle={() => toggleOpen(key)} {fade} onfade={toggleFade} pinned={isPin(r)} onpin={comp ? undefined : () => togglePin(r)} versus={versusFor(r)} />
        </div>
      {:else}
        <div class="mut">Aucun lot ne correspond au filtre</div>
      {/each}
      {/if}
    {/if}
  </SmoothScroll>

  <div class="filter">
    {#if item}
      {#if comp}
        <div class="fh">Filtrer les {item.type?.toLowerCase() ?? "compagnon"}s</div>
        {@const lv = flist(item.gid).find((z) => z.a === LEVEL_EFFECT)}
        <div class="frow lv" class:on={active(LEVEL_EFFECT)}>
          <div class="lb">Niveau</div>
          <input type="number" placeholder="min" value={lv?.min ?? ""} oninput={(e) => (filterOf(item.gid, LEVEL_EFFECT).min = e.currentTarget.value)} />
          <input type="number" placeholder="max" value={lv?.max ?? ""} oninput={(e) => (filterOf(item.gid, LEVEL_EFFECT).max = e.currentTarget.value)} />
          <button class="mini" disabled={!active(LEVEL_EFFECT)} onclick={() => clearEffect(LEVEL_EFFECT)}>Effacer</button>
        </div>
        <label class="leg"><input type="checkbox" bind:checked={legendOnly} /> Statut légendaire uniquement</label>
      {:else}
      <div class="fh">Filtrer les effets</div>
      <div class="btns">
        <button onclick={() => preset("min")} title="min = valeur la plus basse de la plage de base de chaque effet">Valeurs min</button>
        <button onclick={() => preset("damage")} title="parfait (max de base) pour les effets de dommages">Dommages</button>
        <button onclick={() => preset("resist")} title="parfait (max de base) pour les % de résistance">Résistances</button>
        <button onclick={() => preset("clear")}>Effacer</button>
      </div>
      <div class="btns"><EffectPicker effects={pickable} onpick={(id) => filterOf(item.gid, id)} /></div>
      {#each groups as g (g.key)}
        <div class="ghead">{g.label}</div>
        {#each g.items as x (x.a)}
          {@const y = flist(item.gid).find((z) => z.a === x.a)}
          <div class="frow" class:on={active(x.a)}>
            <div class="lb">{x.label}</div>
            <small class="rg">{rangeLabel(x.def)}</small>
            <input type="number" placeholder="min" value={y?.min ?? ""} oninput={(e) => (filterOf(item.gid, x.a).min = e.currentTarget.value)} />
            <input type="number" placeholder="max" value={y?.max ?? ""} oninput={(e) => (filterOf(item.gid, x.a).max = e.currentTarget.value)} />
            <button class="mini" onclick={() => setMin(x.a, x.def)} title="Remplir le min avec le bas de la plage de base">Définir min</button>
            <button class="mini" onclick={() => setPerf(x.a, x.def)} title="Remplir le min avec le max de la plage de base (jet parfait)">Définir perf</button>
            <button class="mini" disabled={!active(x.a)} onclick={() => clearEffect(x.a)}>Effacer</button>
          </div>
        {/each}
      {:else}
        <div class="mut">Aucun effet</div>
      {/each}
      {/if}
    {/if}
  </div>
</div>

<style>
  .view { flex: 1; min-height: 0; display: grid; grid-template-columns: 280px 1fr 470px; }
  .ilist { overflow: auto; background: var(--panel); border-right: 1px solid var(--line); }
  .deal-ico { color: var(--ok); font-size: 10px; vertical-align: middle; }
  .ilist .sbar { display: flex; gap: 6px; padding: 6px 10px; cursor: default; position: sticky; top: 0; z-index: 2; background: var(--panel); }
  .sbar input { flex: 1; min-width: 0; padding: 3px 6px; }
  .thead { display: flex; width: 100%; align-items: center; gap: 6px; border: 0; border-bottom: 1px solid var(--line); border-radius: 0; background: var(--bg); color: inherit; padding: 5px 10px; font: 600 11px system-ui; letter-spacing: 0.04em; text-transform: uppercase; text-align: left; position: sticky; top: 36px; z-index: 1; }
  .chev { display: inline-block; transition: transform 0.2s; }
  .chev.shut { transform: rotate(-90deg); }
  .ilist div { padding: 6px 10px; border-bottom: 1px solid var(--line); cursor: pointer; font-family: system-ui, sans-serif; }
  .ilist .irow:hover { background: var(--bg); }
  .ilist div.sel { outline: 1px solid var(--acc); }
  .ilist .empty { cursor: default; }
  .view > :global(.wrap) { font-family: system-ui, sans-serif; }
  .sort { display: inline-flex; gap: 6px; align-items: center; }
  .arrow { display: inline-block; transition: transform 0.32s cubic-bezier(0.3, 1.4, 0.5, 1); }
  .arrow.desc { transform: rotate(180deg); }
  .head { display: flex; align-items: center; gap: 10px; margin-bottom: 10px; flex-wrap: wrap; }
  .head .fh { margin: 0; flex: 1; }
  .filter { overflow: auto; background: var(--panel); border-left: 1px solid var(--line); padding: 10px; font-family: system-ui, sans-serif; font-size: 12px; }
  .ghead { margin: 12px 0 2px; font-size: 10px; letter-spacing: 0.08em; text-transform: uppercase; color: var(--mut); }
  .mini { padding: 2px 7px; font-size: 11px; }
  .mini:disabled { opacity: 0.35; cursor: default; border-color: var(--line); }
  .graph { margin-bottom: 12px; }
  .head .on { border-color: var(--acc); color: var(--acc); }
  .deals { margin-bottom: 14px; padding: 8px 10px 2px; border: 1px solid color-mix(in srgb, var(--ok) 45%, var(--line)); border-radius: 10px; background: color-mix(in srgb, var(--ok) 5%, transparent); }
  .dhead { display: flex; gap: 6px; align-items: center; width: 100%; border: 0; background: none; font: 600 13px system-ui; color: var(--ok); margin-bottom: 8px; padding: 0; text-align: left; }
  .dhead .mut { font-weight: 400; }
  .deal { cursor: pointer; }
  .deal, .lot { transition: opacity 0.25s; }
  .deal.faded, .lot.faded { opacity: 0.2; }
  .lot.picked :global(.card) { border-color: var(--acc); animation: pulse 1.6s ease-out; }
  @keyframes pulse { from { box-shadow: 0 0 0 0 color-mix(in srgb, var(--acc) 70%, transparent); } to { box-shadow: 0 0 0 14px transparent; } }
  .tabs { display: flex; gap: 6px; margin-bottom: 10px; }
  .tabs .on { border-color: var(--acc); color: var(--acc); }
  .trans .note { margin: 4px 0 8px; font-size: 12px; }
  .trow { padding: 3px 6px; font-size: 12px; cursor: pointer; border-radius: 6px; border: 1px solid transparent; }
  .trow:hover { background: var(--bg); }
  .trow.foc { border-color: var(--acc); }
  .altw { font-size: 12px; margin-bottom: 4px; }
  .warn { color: var(--warn, orange); }
  .link { border: 0; background: none; color: var(--acc); padding: 0; text-decoration: underline; }
  .oh .on { border-color: var(--acc); color: var(--acc); }
  .opp { margin-bottom: 12px; }
  .oh { display: flex; gap: 10px; align-items: baseline; flex-wrap: wrap; font-size: 12px; margin-bottom: 4px; }
  .oh.sel { cursor: pointer; padding: 3px 6px; border: 1px solid transparent; border-radius: 6px; }
  .oh.sel:hover { background: var(--bg); }
  .oh.picked { border-color: var(--acc); }
  .inv { font-weight: 600; }
  .oh .ok { color: var(--ok); font-size: 14px; }
  .btns { display: flex; gap: 6px; flex-wrap: wrap; margin-bottom: 8px; }
  .frow.lv { grid-template-columns: 1fr 52px 52px auto; }
  .leg { display: block; margin: 8px 0; }
  .frow { display: grid; grid-template-columns: 1fr auto 52px 52px auto auto auto; gap: 4px; align-items: center; margin: 3px 0; }
  .frow input { width: 100%; padding: 2px 4px; }
  .frow .lb { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .frow .rg { color: var(--mut); text-align: right; white-space: nowrap; padding-right: 2px; }
  .frow.on .lb { font-weight: 700; color: var(--acc); }
</style>
