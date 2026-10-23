<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { app } from "../../lib/app.svelte";
  import { cached } from "../../lib/api";
  import { Columns } from "../../lib/columns.svelte";
  import { ago, kamas } from "../../lib/fmt";
  import { readJSON, writeJSON } from "../../lib/persist.svelte";
  import { auction } from "../../lib/stores/auction.svelte";
  import { prices } from "../../lib/stores/prices.svelte";
  import { settings } from "../../lib/stores/settings.svelte";
  import { isLegendary, levelOf, xpOfLevel } from "../../lib/companion";
  import { analyse, isGoodDeal } from "../../lib/density";
  import { feeder, foodGids, levelBase } from "../../lib/familiar";
  import { mountInfo, neuroneGids, neuronsOf, resourcePlural } from "../../lib/mounts";
  import type { EffectMeta } from "../../lib/effects";
  import { benefit, craftCost, type Recipes } from "../../lib/craft";
  import { isTranscended, transcendance, type Rune } from "../../lib/transcend";
  import { byUnitPrice, lotKeys, unitPrice } from "../../lib/price";
  import Collapsible from "../../lib/components/Collapsible.svelte";
  import CompanionTable from "../../lib/components/CompanionTable.svelte";
  import DensityChart from "../../lib/components/DensityChart.svelte";
  import Grip from "../../lib/components/Grip.svelte";
  import ItemCard from "../../lib/components/ItemCard.svelte";
  import ItemIcon from "../../lib/components/ItemIcon.svelte";
  import LotTable from "../../lib/components/LotTable.svelte";
  import SmoothScroll from "../../lib/components/SmoothScroll.svelte";
  import CraftSummary from "./CraftSummary.svelte";
  import FilterPanel from "./FilterPanel.svelte";
  import ItemList from "./ItemList.svelte";
  import TranscendView from "./TranscendView.svelte";
  import { effectLabel, Filters } from "./filters.svelte";

  /**
   * HDV tab: scanned items (ItemList) | the open item's lots: craft summary, price / density graph, good deals, lot table or
   * Transcendance tab (TranscendView) | filters and the pinned lot (FilterPanel). The filters live in `filt` (filters.svelte.ts).
   */

  const cols = new Columns("karrelage.cols.items", { list: 280, filter: 470 }); // resizable: list | listing (flexible) | filter
  const items = auction.items; // shared store: every scanned listing, kept up to date for the whole session
  const SEL_KEY = "karrelage.auction.sel"; // the open item is restored when the tab is opened again
  let selGid = $state<number | null>(readJSON<number | null>(SEL_KEY, null));
  $effect(() => writeJSON(SEL_KEY, selGid));
  let picker = $state.raw<EffectMeta[]>([]); // effects found on level 150-200 equipment (what the picker offers)
  let meta = $state.raw<Record<number, EffectMeta>>({}); // every labelled effect (labels/groups of whatever shows up in a lot)
  const filt = new Filters(() => item);
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
  /** Highlighted effect: lots without it as an EXO or above its base max are faded out (a plain base roll does not count). */
  const lacksFade = (r: any) => fade !== null && !r.effects.some((e: any) => e.action === fade && (e.status === "added" || e.status === "above"));
  const toggleFade = (a: number) => (fade = fade === a ? null : a);
  let tab = $state<"listing" | "trans">("listing");
  let levelOpen = $state(true);
  let dealsOpen = $state(false); // "Good deals" starts collapsed
  let cand = $state<{ id: string | number; a: number; value: number } | null>(null); // selected candidate: its point + the lots with the same rune applied stand out
  let focus = $state<string | null>(null); // transcended effect (targetKey) the candidates are limited to
  let runes = $state.raw<Rune[]>([]);
  const stored = prices.map; // stored prices (the rune prices feed the transcendance tab)
  let recipes = $state.raw<Recipes>({ recipes: {}, names: {} });
  let allOpen = $state(false);
  let sortDir = $state(1);

  let legendOnly = $state(false); // companions: only legendary ones
  const list = $derived([...items.values()].sort((a, b) => b.updated - a.updated));
  const item = $derived(selGid != null ? items.get(selGid) : undefined);

  /** Familier / montilier listing: tracked by level (price follows the xp curve), no stats / pin / transcendance. */
  const comp = $derived(!!item?.companion);
  /** What a lot is compared with: the pinned lot (never shown against itself). One object per pin, so cards don't re-pair on every render. */
  const versus = $derived(pin && !comp ? { pin: pin.row, name: pin.item.name } : null);
  const versusFor = (r: any) => (isPin(r) ? null : versus);

  // ---- companions: cheapest way to reach a level, buying a lot and feeding it (resource prices = stored prices)
  let foodIds = $state.raw<Record<string, number>>({});
  let levelTarget = $state(100);
  interface LevelOpt { row: any; level: number; price: number; feed: number; total: number; plan: ReturnType<ReturnType<typeof feeder>["plan"]> }
  const levelling = $derived.by(() => {
    if (!comp || !item?.feedable) return null; // only familiars / montiliers are fed
    const f = feeder(foodIds, stored);
    const target = Math.min(Math.max(Math.round(levelTarget) || 1, 1), 100);
    const opts: LevelOpt[] = [];
    let direct: { row: any; price: number } | null = null; // cheapest lot already at the target level
    for (const row of item.rows) {
      const level = levelOf(row);
      const price = unitPrice(row);
      if (level == null || !Number.isFinite(price)) continue;
      if (level >= target) {
        if (!direct || price < direct.price) direct = { row, price };
        continue;
      }
      const plan = f.plan(levelBase(target) - levelBase(level));
      if (plan) opts.push({ row, level, price, feed: plan.cost, total: price + plan.cost, plan });
    }
    opts.sort((a, b) => a.total - b.total);
    const best = opts[0] ?? null;
    return { target, direct, opts: opts.slice(0, 1), best, deal: !!best && (!direct || best.total < direct.price), saving: best && direct ? direct.price - best.total : null, priced: f.priced };
  });
  const effLabel = (a: number) => effectLabel(meta, a);
  /** Craft cost of the open item (recursive, from the stored prices) and what selling it would earn. */
  const craft = $derived(item && !comp ? craftCost(item.gid, recipes, stored) : null);
  const sale = $derived(item ? Math.min(Infinity, ...item.rows.map(unitPrice)) : Infinity);
  const gain = $derived(craft?.cost != null && Number.isFinite(sale) ? benefit(sale, craft.cost) : null);

  /** Stable key of each lot of the open item (uid, else a content hash): independent of the filters and the sort. */
  const keyMap = $derived(item ? lotKeys(item.rows) : new Map<any, string>());
  const keyOf = (r: any) => keyMap.get(r) ?? "";

  const rows = $derived.by(() => {
    if (!item) return [];
    return item.rows
      .filter((r: any) => (!(comp && legendOnly) || isLegendary(r)) && filt.matches(item.gid, r))
      .sort(byUnitPrice(sortDir));
  });
  const rowSet = $derived(new Set(rows));

  // dragodindes / muldos / volkornes: extraction into neurones / ambres / cornes (heads-up on lots worth extracting)
  let neurGids = $state.raw<Record<string, number>>({});
  const minfo = $derived(comp ? mountInfo(item?.name) : null);
  const neurons = $derived(minfo?.neurons ?? null);
  const resource = $derived(minfo ? resourcePlural(minfo.neurone) : "Neurones");
  const neuroneUnit = $derived(minfo && neurGids[minfo.neurone] != null ? prices.price(neurGids[minfo.neurone]) : null);
  const extractable = $derived(
    neurons != null && neuroneUnit != null ? rows.filter((r: any) => Number.isFinite(unitPrice(r)) && benefit(neuroneUnit * neuronsOf(r, neurons), unitPrice(r)).profit > 0).length : 0,
  );
  const lotsOf = (l: any[]) => l.map((r) => ({ row: r, key: keyOf(r) }));
  /** Scroll the lot list to a lot. */
  async function revealLot(id: string | number) {
    await tick();
    document.getElementById(`lot-${id}`)?.scrollIntoView({ behavior: "smooth", block: "center" });
  }
  async function pickLot(id: string | number) {
    picked = id;
    await revealLot(id);
  }

  function toggleOpen(key: string | number) {
    openSet.has(key) ? openSet.delete(key) : openSet.add(key);
  }
  /** Table view: unfolding a lot also marks it on the graph; folding it back clears the mark. */
  function toggleRow(key: string) {
    toggleOpen(key);
    picked = openSet.has(key) || allOpen ? key : picked === key ? null : picked;
  }
  /** Height of the sticky graph: the table header sticks right under it. */
  let graphH = $state(0);

  /** Graph dots: price (per unit) against the lot's density. Same lots as the list, so every dot has a card. */
  /** Rune focus (Transcendance tab): the graph fades the lots that lack the focused effect (like the highlight). */
  const focusEffect = $derived(tab === "trans" && !comp && focus ? Number(focus.split(":")[0]) : null);
  const mount = $derived(!!mountInfo(item?.name));
  /** Height of a dot: the lot's density; companions: the total xp of its level (the price follows it). */
  const yOf = (r: any, companion: boolean) => (companion ? (levelOf(r) != null ? xpOfLevel(levelOf(r)!) : NaN) : r.density);
  /** Positions only (no fade / focus flags), so highlighting a lot never refits the trend. */
  const pointRows = $derived(rows.filter((r: any) => Number.isFinite(unitPrice(r)) && Number.isFinite(yOf(r, comp))));
  const points = $derived(
    pointRows.map((r: any) => ({
      id: keyOf(r),
      x: unitPrice(r),
      y: yOf(r, comp),
      label: r.name,
      added: comp ? isLegendary(r) : !mount && r.effects.some((e: any) => e.status === "added"), // companions: blue dot = legendary; mounts have no EXO
      missing: !comp && !mount && r.effects.some((e: any) => e.status === "removed"),
    })),
  );
  const fit = $derived(points.length ? analyse(points) : null);
  /** Dots drawn faded: lots without the highlighted effect, or (Transcendance tab) not matching the rune focus / candidate. */
  const fadedIds = $derived.by(() => {
    const out = new Set<string>();
    if (fade === null && !cand && focusEffect === null) return out;
    for (const r of pointRows) {
      const sameRune = cand && (keyOf(r) === cand.id || (r.locked && r.effects.some((e: any) => e.status === "added" && e.action === cand!.a && e.hi === cand!.value)));
      const has = cand ? !!sameRune : focusEffect === null || r.effects.some((e: any) => e.action === focusEffect && e.status !== "removed");
      if (lacksFade(r) || !has) out.add(keyOf(r));
    }
    return out;
  });

  /** Lots standing out above the price trend per scanned item (all its lots, unfiltered): the green icon in the left list.
   *  Cached per item object (a scan replaces the item whole, so only the new one is analysed) and threshold. */
  const dealCache = new WeakMap<object, { z: number; n: number }>();
  function dealCount(it: any): number {
    const z = settings.hdv.goodZ;
    let n = dealCache.get(it)?.z === z ? dealCache.get(it)!.n : undefined;
    if (n === undefined) {
      const pts = it.rows.map((r: any) => ({ x: unitPrice(r), y: yOf(r, !!it.companion) })).filter((p: any) => Number.isFinite(p.x) && Number.isFinite(p.y));
      n = 0;
      if (pts.length >= 5) {
        const f = analyse(pts);
        for (let i = 0; i < pts.length; i++) if (isGoodDeal(f, i, pts.length)) n++;
      }
      dealCache.set(it, { z, n });
    }
    return n;
  }

  /** Lots far above the price trend (same rule as the green dots), best first. */
  const deals = $derived.by(() => {
    if (!fit) return [];
    const f = fit;
    return pointRows
      .map((r: any, i: number) => ({ key: points[i].id, row: r, z: f.z[i], good: isGoodDeal(f, i, points.length) }))
      .filter((d: any) => d.good)
      .sort((a: any, b: any) => b.z - a.z)
      .slice(0, 6);
  });

  /** Only computed when read (deriveds are lazy): the Transcendance tab, the deals' "similar item" hint, and the tab count when some lot is transcended. */
  const trans = $derived(transcendance(comp ? [] : rows, runes, stored, focus));
  const transCount = $derived(!comp && rows.some(isTranscended) ? trans.opportunities.length : 0);
  /** Table view: the good deals (green lines). */
  const goodKeys = $derived(new Set<string>(fit ? points.filter((_: any, i: number) => isGoodDeal(fit!, i, points.length)).map((p: any) => p.id) : []));

  /** Clicking a dot: unroll the lot and scroll the list down to it (nothing is filtered out). */
  async function focusLot(id: string | number) {
    picked = id;
    openSet.add(id);
    tab = "listing"; // from the Transcendance tab: switch so the lot is rendered
    await revealLot(id);
  }
  /** A lot asked for from elsewhere (a notification, `app.showLot`): open its item, unfold it and scroll to it. Filters of
   *  the item that hide it are cleared. Waits for the scanned listings. */
  let listed = $state(false);
  $effect(() => {
    const f = app.hdvFocus;
    if (f && listed) untrack(() => void showFocus(f));
  });
  async function showFocus(f: { gid: number; lot: string }) {
    app.hdvFocus = null;
    if (!items.has(f.gid)) return;
    selGid = f.gid;
    tab = "listing";
    legendOnly = false;
    await tick(); // the selection effect resets the highlight and the rune focus first
    if (!rows.some((r: any) => keyOf(r) === f.lot)) filt.reset(f.gid);
    if (!item?.rows.some((r: any) => keyOf(r) === f.lot)) return; // sold since: the item alone
    openSet.add(f.lot);
    picked = f.lot;
    await revealLot(f.lot);
  }

  /** An item just captured from the game: show it (a refresh of the item already open keeps its scroll position). */
  async function showNew(i: any) {
    if (selGid === i.gid) return;
    selGid = i.gid;
    picked = null;
    await tick();
    document.querySelector(".view .scroller")?.scrollTo({ top: 0 });
  }

  onMount(() => {
    let alive = true;
    const live = <T,>(f: (v: T) => void) => (v: T) => void (alive && f(v)); // answers after the tab closed are dropped
    cached<EffectMeta[]>("auction", "effects").then(live((e) => (picker = e))).catch(() => {});
    cached<EffectMeta[]>("auction", "effect_meta").then(live((l) => (meta = Object.fromEntries(l.map((e) => [e.id, e]))))).catch(() => {});
    cached<Recipes>("auction", "recipes").then(live((r) => (recipes = r))).catch(() => {});
    neuroneGids().then(live((m) => (neurGids = m))).catch(() => {});
    foodGids().then(live((m) => (foodIds = m))).catch(() => {});
    cached<Rune[]>("prices", "runes").then(live((l) => (runes = l))).catch(() => {});
    void prices.ensure();
    // the shared stores never let an older snapshot replace a newer scan / price (see lib/stores)
    auction.ensure().then(live(() => {
      listed = true;
      if (app.hdvFocus) return; // showFocus picks the item and scrolls
      if ((selGid === null || !items.has(selGid)) && list.length) selGid = list[0].gid;
      void tick().then(() => document.querySelector(".ilist .irow.sel")?.scrollIntoView({ block: "center" })); // back to the item that was open
    }));
    const off = auction.subscribe((c) => {
      if (c.kind === "clear") selGid = null;
      else for (const item of c.values) {
        if (c.live) void showNew(item);
        else if (selGid === null) selGid = item.gid;
      }
    });
    return () => {
      alive = false;
      off();
    };
  });
</script>

<div class="view cgrid" class:anim={!cols.dragging} style:grid-template-columns="{cols.px("list")}px 5px 1fr 5px {cols.px("filter")}px">
  <ItemList {cols} bind:sel={selGid} {dealCount} />

  <Grip {cols} name="list" dir={1} />

  <SmoothScroll toTop>
    {#if item}
      <div class="head">
        <ItemIcon icon={item.icon} class="hic" />
        <div class="fh">{item.name} <span class="mut">{item.level != null ? "niv. " + item.level + " · " : ""}{item.type ?? ""}{app.dev ? " · gid " + item.gid : ""} · mis à jour {ago(item.updated)} · {rows.length}/{item.lots} lots</span></div>
        <button class="sort" onclick={() => (sortDir = -sortDir)} title="Cliquer pour inverser l'ordre des prix">
          Prix <span class="arrow" class:desc={sortDir === -1}>↑</span>
        </button>
        <button onclick={() => ((allOpen = !allOpen), openSet.clear())}>{allOpen ? "Tout replier" : "Tout déplier"}</button>
        {#if pin}<button class="on" onclick={() => (pin = null)} title="Arrêter de comparer les lots avec le lot épinglé">Désépingler {pin.item.name}</button>{/if}
      </div>
      {#if craft}<CraftSummary {craft} {sale} {gain} />{/if}
      {#if points.length >= 3}
        <div class="graph" bind:offsetHeight={graphH}><DensityChart {points} {fit} faded={fadedIds} equipment={!comp && !mount} selected={tab === "trans" && cand ? cand.id : picked} onselect={focusLot} /></div>
      {/if}
      {#if !comp}<div class="tabs">
        <button class:on={tab === "listing"} onclick={() => (tab = "listing")}>Liste</button>
        <button class:on={tab === "trans"} onclick={() => (tab = "trans")}>Transcendance{transCount ? ` · ${transCount}` : ""}</button>
      </div>{/if}
      {#if tab === "trans" && !comp}
        <TranscendView
          {trans}
          {item}
          {keyOf}
          {effLabel}
          open={(k) => openSet.has(k)}
          ontoggle={toggleOpen}
          bind:focus
          bind:cand
          {fade}
          onfade={toggleFade}
          onselect={(gid) => (selGid = gid)}
        />
      {:else}
      {#if comp && levelling && levelling.deal}
        <div class="deals">
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div class="dhead lvh" role="button" tabindex="0" aria-expanded={levelOpen} onclick={() => (levelOpen = !levelOpen)}>
            <span class="chev" class:shut={!levelOpen}>▾</span> Monter au niveau
            <input class="lvt" type="number" min="1" max="100" bind:value={levelTarget} onclick={(e) => e.stopPropagation()} />
            <span class="mut">· Plus rentable de partir du niveau {levelling.best!.level}</span>
          </div>
          {#if levelOpen}
            {#each levelling.opts as o (keyOf(o.row))}
              {@const listed = rowSet.has(o.row)}
              <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
              <div class="deal" title={listed ? "Cliquer pour aller au lot dans la liste" : ""} onclick={() => listed && focusLot(keyOf(o.row))}>
                <div class="lvl"><b>{kamas(Math.round(o.total))}</b> <span class="mut">= niveau {o.level} à {kamas(o.price)} + {kamas(Math.round(o.feed))} de nourriture</span></div>
                <div class="mut">{o.plan?.feeds.map((x) => `${x.meals} repas ${x.food} (${x.units} unité${x.units > 1 ? "s" : ""})`).join(" + ")}</div>
              </div>
            {/each}
          {/if}
        </div>
      {/if}
      {#if deals.length}
        <div class="deals">
          <Collapsible head="dhead" title="Bonnes affaires" count="· {deals.length} {deals.length === 1 ? 'lot se démarque' : 'lots se démarquent'} au-dessus de la tendance des prix" bind:open={dealsOpen}>
          {#if comp}
            <CompanionTable lots={lotsOf(deals.map((d: any) => d.row)).map((l) => ({ ...l, key: "d-" + l.key }))} {neurons} {neuroneUnit} {resource} faded={lacksFade} onpick={(k) => pickLot(String(k).slice(2))} />
          {:else}
          {#each deals as d (d.key)}
            {@const alt = trans.alternative(d.row)}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <div class="deal" class:faded={lacksFade(d.row)} title="Cliquer pour aller au lot dans la liste" onclick={() => focusLot(d.key)}>
              {#if alt}
                <div class="warn altw">⚠ Objet similaire réalisable par Transcendance : lot {kamas(alt.buy)} + {alt.rune.name} {kamas(alt.runeCost)} = {kamas(alt.cost)} (économie de {kamas(alt.saving)})</div>
              {/if}
              <ItemCard row={d.row} companion={false} {fade} onfade={toggleFade} pinned={isPin(d.row)} onpin={() => togglePin(d.row)} versus={versusFor(d.row)} />
            </div>
          {/each}
          {/if}
          </Collapsible>
        </div>
      {/if}
      {#if comp}
        {#if extractable}
          <div class="warn altw">⚠ {extractable} lot{extractable > 1 ? "s" : ""} rentable{extractable > 1 ? "s" : ""} à extraire en {resource.toLowerCase()}</div>
        {/if}
        <CompanionTable lots={lotsOf(rows)} {neurons} {neuroneUnit} {resource} {picked} faded={lacksFade} onpick={pickLot} />
      {:else}
        <LotTable lots={lotsOf(rows)} good={goodKeys} transcend={false} {picked} open={(k) => allOpen || openSet.has(k)} faded={lacksFade} ontoggle={toggleRow} stickTop={points.length >= 3 ? graphH : 0} {fade} onfade={toggleFade} pinned={isPin} onpin={togglePin} versus={versusFor} />
      {/if}
      {/if}
    {/if}
  </SmoothScroll>

  <Grip {cols} name="filter" dir={-1} />

  <FilterPanel {cols} {item} {comp} {filt} {meta} {picker} {pin} onunpin={() => (pin = null)} bind:legendOnly />
</div>

<style>
  .view { flex: 1; min-height: 0; display: grid; }
  :global(.ii.hic) { width: 36px; height: 36px; margin: -6px 0; }
  .view > :global(.wrap) { font-family: var(--font); }
  .sort { display: inline-flex; gap: 6px; align-items: center; }
  .arrow { display: inline-block; transition: transform 0.32s cubic-bezier(0.3, 1.4, 0.5, 1); }
  .arrow.desc { transform: rotate(180deg); }
  .head { display: flex; align-items: center; gap: 10px; margin-bottom: 10px; flex-wrap: wrap; }
  .head .fh { margin: 0; flex: 1; }
  /* the graph stays at the top of the scroll area (flush, over the scroller's padding) while the lots scroll under it */
  .graph { position: sticky; top: calc(-1 * var(--pad, 12px)); z-index: 3; margin-bottom: 6px; padding: 6px 0; background: var(--bg); }
  .head .on { border-color: var(--acc); color: var(--acc); }
  .lvt { width: 56px; padding: 1px 4px; margin: 0 4px; }
  .lvl { padding: 0; }
  .dhead.lvh { margin-bottom: 2px; }
  .deals { margin-bottom: 14px; padding: 8px 10px 2px; border: 1px solid color-mix(in srgb, var(--ok) 45%, var(--line)); border-radius: 10px; background: color-mix(in srgb, var(--ok) 5%, transparent); }
  .dhead { display: flex; gap: 6px; align-items: center; width: 100%; border: 0; background: none; font: 600 13px var(--font); color: var(--ok); margin-bottom: 8px; padding: 0; text-align: left; }
  .dhead .mut { font-weight: 400; }
  .deals :global(.chd.dhead) { font: 600 13px var(--font); color: var(--ok); margin-bottom: 8px; padding: 0; }
  .deals :global(.chd.dhead .mut) { font-weight: 400; }
  .deal { cursor: pointer; }
  .deal { transition: opacity 0.25s; }
  .deal.faded { opacity: 0.2; }
  .tabs { display: flex; gap: 6px; margin-bottom: 10px; }
  .tabs .on { border-color: var(--acc); color: var(--acc); }
  .altw { font-size: 12px; margin-bottom: 4px; }
</style>
