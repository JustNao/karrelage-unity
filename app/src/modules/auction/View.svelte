<script lang="ts">
  import { onMount, tick } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { app } from "../../lib/app.svelte";
  import Grip from "../../lib/components/Grip.svelte";
  import PanelHead from "../../lib/components/PanelHead.svelte";
  import { Columns } from "../../lib/columns.svelte";
  import { call } from "../../lib/api";
  import { ago, kamas, pct, signedKamas } from "../../lib/fmt";
  import { auction } from "../../lib/stores/auction.svelte";
  import { prices } from "../../lib/stores/prices.svelte";
  import Collapsible from "../../lib/components/Collapsible.svelte";
  import ConfirmButton from "../../lib/components/ConfirmButton.svelte";
  import SearchBar, { matcher } from "../../lib/components/SearchBar.svelte";
  import SideList from "../../lib/components/SideList.svelte";
  import { flip } from "svelte/animate";
  import DensityChart from "../../lib/components/DensityChart.svelte";
  import EffectPicker from "../../lib/components/EffectPicker.svelte";
  import ItemCard from "../../lib/components/ItemCard.svelte";
  import SmoothScroll from "../../lib/components/SmoothScroll.svelte";
  import VirtualList from "../../lib/components/VirtualList.svelte";
  import { LEVEL_EFFECT, isLegendary, levelOf, xpOfLevel } from "../../lib/companion";
  import { analyse, isGoodDeal } from "../../lib/density";
  import { feeder, foodGids, levelBase } from "../../lib/familiar";
  import CompanionTable from "../../lib/components/CompanionTable.svelte";
  import { mountInfo, neuroneGids, neuronsOf, resourcePlural } from "../../lib/mounts";
  import { FLAG_EFFECTS, rangeLabel, sections, type EffectMeta } from "../../lib/effects";
  import { STAT_ICON } from "../../lib/stuff";
  import { benefit, craftCost, type CraftLine, type Recipes } from "../../lib/craft";
  import { AH_TAX, isTranscended, targetKey, transcendance, type Rune } from "../../lib/transcend";
  import { byUnitPrice, lotKeys, unitPrice } from "../../lib/price";

  interface Filter { a: number; min: string | number; max: string | number }

  const cols = new Columns("karrelage.cols.items", { list: 280, filter: 470 }); // resizable: list | listing (flexible) | filter
  const items = auction.items; // shared store: every scanned listing, kept up to date for the whole session
  const SEL_KEY = "karrelage.auction.sel"; // the open item is restored when the tab is opened again
  let selGid = $state<number | null>((() => { try { return JSON.parse(localStorage.getItem(SEL_KEY) ?? "null"); } catch { return null; } })());
  $effect(() => { try { localStorage.setItem(SEL_KEY, JSON.stringify(selGid)); } catch {} });
  let picker = $state.raw<EffectMeta[]>([]); // effects found on level 150-200 equipment (what the picker offers)
  let statIcons = $state.raw<Record<string, string>>({}); // Dofusbook stat icons (data URLs, cached by the stuff module)
  const iconOf = (a: number) => { const st = meta[a]?.stat; return st != null ? statIcons[STAT_ICON.get(st) ?? ""] : undefined; };
  let meta = $state.raw<Record<number, EffectMeta>>({}); // every labelled effect (labels/groups of whatever shows up in a lot)
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
  const lacksFade = (r: any) => fade !== null && !r.effects.some((e: any) => e.action === fade && e.status !== "removed");
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
  /** Left panel: items grouped by type, each group collapsible (collapsed groups are remembered). */
  const COLLAPSE_KEY = "karrelage.collapsedTypes";
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
    const match = matcher(search);
    for (const i of list) {
      if (!match(i.name, i.type)) continue;
      const t = i.type ?? "?";
      if (!m.has(t)) m.set(t, []);
      m.get(t)!.push(i);
    }
    return [...m.entries()].sort((a, b) => a[0].localeCompare(b[0], "fr")).map(([type, items]) => ({ type, items }));
  });
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
  const effLabel = (a: number) => meta[a]?.label ?? `effect ${a}`;
  /** Craft cost of the open item (recursive, from the stored prices) and what selling it would earn. */
  const craft = $derived(item && !comp ? craftCost(item.gid, recipes, stored) : null);
  const sale = $derived(item ? Math.min(Infinity, ...item.rows.map(unitPrice)) : Infinity);
  const gain = $derived(craft?.cost != null && Number.isFinite(sale) ? benefit(sale, craft.cost) : null);
  const lotVal = (r: any, a: number) => Math.max(0, ...r.effects.filter((e: any) => e.action === a && e.status !== "removed").map((e: any) => e.hi ?? 0));
  const flist = (gid: number) => filters[gid] ?? [];

  /** Stable key of each lot of the open item (uid, else a content hash): independent of the filters and the sort. */
  const keyMap = $derived(item ? lotKeys(item.rows) : new Map<any, string>());
  const keyOf = (r: any) => keyMap.get(r) ?? "";

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
  const groups = $derived([
    // the effects the user added come first, in their own section
    ...(effectRows.some((x) => x.def == null) ? [{ key: "added", label: "Effets ajoutés", items: effectRows.filter((x) => x.def == null).map((x) => ({ ...x, label: effLabel(x.a) })) }] : []),
    ...sections(effectRows.filter((x) => x.def != null).map((x) => ({ ...x, group: meta[x.a]?.group ?? "other", order: meta[x.a]?.order ?? 9999, label: effLabel(x.a) }))),
  ]
  );

  const rows = $derived.by(() => {
    if (!item) return [];
    const f = flist(item.gid).filter((x) => x.min !== "" || x.max !== "");
    return item.rows
      .filter((r: any) => !(comp && legendOnly) || isLegendary(r))
      .filter((r: any) => f.every((x) => {
        if (FLAG_EFFECTS.has(x.a)) return x.min === "" || r.effects.some((e: any) => e.action === x.a && e.status !== "removed");
        const v = lotVal(r, x.a);
        return (x.min === "" || v >= +x.min) && (x.max === "" || v <= +x.max);
      }))
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
  /** From this many lots on, the list is virtualized (and not animated). */
  const VIRTUAL_FROM = 50;
  let vlist = $state<ReturnType<typeof VirtualList> | null>(null);
  /** Scroll the lot list to a lot (rendering it first when the list is virtualized). */
  async function revealLot(id: string | number) {
    await tick();
    if (!comp && vlist) return void vlist.scrollToKey(id);
    document.getElementById(`lot-${id}`)?.scrollIntoView({ behavior: "smooth", block: "center" });
  }
  async function pickLot(id: string | number) {
    picked = id;
    await revealLot(id);
  }


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
   *  Cached per item object: a scan replaces the item whole, so only the new one is analysed. */
  const dealCache = new WeakMap<object, number>();
  function dealCount(it: any): number {
    let n = dealCache.get(it);
    if (n === undefined) {
      const pts = it.rows.map((r: any) => ({ x: unitPrice(r), y: yOf(r, !!it.companion) })).filter((p: any) => Number.isFinite(p.x) && Number.isFinite(p.y));
      n = 0;
      if (pts.length >= 5) {
        const f = analyse(pts);
        for (let i = 0; i < pts.length; i++) if (isGoodDeal(f, i, pts.length)) n++;
      }
      dealCache.set(it, n);
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

  // ---- saved transcendance candidates: a snapshot kept across scans (the lot may be sold by the time you come back)
  /** `row` is a summary of the lot (uid, priced stacks, lock and what the card shows of each effect), not the whole row. */
  interface Saved { key: string; gid: number; itemName: string; savedAt: number; row: any; label: string; value: number; rune: string; buy: number; runeCost: number; sale: number; profit: number }
  const SAVED_KEY = "karrelage.savedCandidates";
  const lotSummary = (r: any) => ({
    uid: r.uid ?? null,
    gid: r.gid ?? null,
    locked: !!r.locked,
    prices: (r.prices ?? []).filter((p: any) => p.price > 0),
    effects: (r.effects ?? []).map(({ action, text, status, hi, range, group, order, density, stat, known }: any) => ({ action, text, status, hi, range, group, order, density, stat, known })),
  });
  const loadSaved = (): Saved[] => {
    try {
      const l: Saved[] = JSON.parse(localStorage.getItem(SAVED_KEY) ?? "[]").map((s: Saved) => ({ ...s, row: lotSummary(s.row) })); // older entries kept the full row
      if (l.length) localStorage.setItem(SAVED_KEY, JSON.stringify(l));
      return l;
    } catch {
      return [];
    }
  };
  let saved = $state.raw<Saved[]>(loadSaved());
  const persistSaved = (l: Saved[]) => {
    saved = l;
    try {
      localStorage.setItem(SAVED_KEY, JSON.stringify(l));
    } catch {}
  };
  const candKey = (o: any) => `${item.gid}:${keyOf(o.row)}`;
  const isSaved = (o: any) => saved.some((s) => s.key === candKey(o));
  function toggleSaved(o: any) {
    const key = candKey(o);
    if (isSaved(o)) return persistSaved(saved.filter((s) => s.key !== key));
    const t = o.best.target;
    persistSaved([
      { key, gid: item.gid, itemName: item.name, savedAt: Date.now() / 1000, row: lotSummary(o.row), label: effLabel(t.a), value: t.value, rune: t.rune?.name ?? "?", buy: o.buy, runeCost: t.runeCost ?? 0, sale: t.sale, profit: o.best.profit },
      ...saved,
    ]);
  }
  /** Is the saved lot still in the latest scan of its item? null = the item has not been scanned (here) since. */
  const stillListed = (sv: Saved) => {
    const it = items.get(sv.gid);
    return it && sv.row.uid != null ? it.rows.some((r: any) => r.uid === sv.row.uid) && it.updated > sv.savedAt : null;
  };
  /** Only computed when read (deriveds are lazy): the Transcendance tab, the deals' "similar item" hint, and the tab count when some lot is transcended. */
  const trans = $derived(transcendance(comp ? [] : rows, runes, stored, focus));
  const transCount = $derived(!comp && rows.some(isTranscended) ? trans.opportunities.length : 0);
  const runeName = (n: string) => n;

  /** Clicking a dot: unroll the lot and scroll the list down to it (nothing is filtered out). */
  async function focusLot(id: string | number) {
    picked = id;
    openSet.add(id);
    tab = "listing"; // from the Transcendance tab: switch so the lot is rendered
    await revealLot(id);
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
    call<EffectMeta[]>("auction", "effects").then(live((e) => (picker = e)));
    call<EffectMeta[]>("auction", "effect_meta").then(live((l) => (meta = Object.fromEntries(l.map((e) => [e.id, e])))));
    call<Record<string, string>>("stuff", "stat_icons").then(live((m) => (statIcons = m))).catch(() => {});
    call<Recipes>("auction", "recipes").then(live((r) => (recipes = r)));
    neuroneGids().then(live((m) => (neurGids = m))).catch(() => {});
    foodGids().then(live((m) => (foodIds = m))).catch(() => {});
    call<Rune[]>("prices", "runes").then(live((l) => (runes = l)));
    void prices.ensure();
    // the shared stores never let an older snapshot replace a newer scan / price (see lib/stores)
    auction.ensure().then(live(() => {
      if ((selGid === null || !items.has(selGid)) && list.length) selGid = list[0].gid;
      void tick().then(() => document.querySelector(".ilist .irow.sel")?.scrollIntoView({ block: "center" })); // back to the item that was open
    }));
    const off = auction.subscribe((c) => {
      if (c.kind === "clear") selGid = null;
      else if (c.live) void showNew(c.item);
      else if (selGid === null) selGid = c.item.gid;
    });
    return () => {
      alive = false;
      off();
    };
  });
</script>

<div class="view cgrid" class:anim={!cols.dragging} style:grid-template-columns="{cols.px("list")}px 5px 1fr 5px {cols.px("filter")}px">
  <SideList {cols} name="list" title="Objets">
    {#snippet bar()}
      <SearchBar bind:value={search} placeholder="Rechercher un objet…" grow compact>
        {#snippet after()}<ConfirmButton mini label="Nettoyer" title="Oublier tous les objets enregistrés" onconfirm={() => call("auction", "clear")} />{/snippet}
      </SearchBar>
    {/snippet}
    {#each typeGroups as g (g.type)}
      <Collapsible head="thead" title={g.type} count={g.items.length} open={!collapsed.has(g.type)} ontoggle={() => toggleType(g.type)}>
        {#each g.items as i (i.gid)}
          {@const dc = dealCount(i)}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="irow" class:sel={selGid === i.gid} onclick={() => (selGid = i.gid)}>
            <b>{i.name}</b> <span class="mut">niv. {i.level ?? "?"}</span>
            {#if dc}<span class="deal-ico" title="{dc} {dc === 1 ? 'bonne affaire' : 'bonnes affaires'} dans cette liste">●</span>{/if}<br />
            <span class="mut">{i.lots} lots · {i.min_price ? "min " + kamas(i.min_price) : "—"} · {ago(i.updated)}{i.demo ? " · demo" : ""}</span>
          </div>
        {/each}
      </Collapsible>
    {:else}
      <div class="mut empty">{search ? "Aucun objet ne correspond à la recherche." : "Ouvrez un objet à l'hôtel de vente : il apparaît ici (dernière liste par objet)."}</div>
    {/each}
  </SideList>

  <Grip {cols} name="list" dir={1} />

  <SmoothScroll toTop>
    {#if item}
      <div class="head">
        <div class="fh">{item.name} <span class="mut">{item.level != null ? "niv. " + item.level + " · " : ""}{item.type ?? ""}{app.dev ? " · gid " + item.gid : ""} · mis à jour {ago(item.updated)} · {rows.length}/{item.lots} lots</span></div>
        <button class="sort" onclick={() => (sortDir = -sortDir)} title="Cliquer pour inverser l'ordre des prix">
          Prix <span class="arrow" class:desc={sortDir === -1}>↑</span>
        </button>
        <button onclick={() => ((allOpen = !allOpen), openSet.clear())}>{allOpen ? "Tout replier" : "Tout déplier"}</button>
        {#if pin}<button class="on" onclick={() => (pin = null)} title="Arrêter de comparer les lots avec le lot épinglé">Désépingler {pin.item.name}</button>{/if}
      </div>
      {#if craft}
        <div class="craft" class:pos={!!gain && gain.profit >= 0}>
          <div class="crow">
            <span>Craft <b>{craft.cost != null ? kamas(Math.round(craft.cost)) : "—"}</b></span>
            {#if Number.isFinite(sale)}<span class="mut">Vente min {kamas(Math.round(sale))}</span>{/if}
            {#if gain}
              <span class={gain.profit >= 0 ? "gain" : "loss"} title="Après {AH_TAX * 100} % de taxe de vente : prix de vente − coût de craft">
                Bénéfice <b>{signedKamas(gain.profit)}</b> ({pct(gain.pct)})
              </span>
            {/if}
            {#if craft.missing.length}<span class="warn" title={craft.missing.join(", ")}>prix manquant : {craft.missing.slice(0, 3).join(", ")}{craft.missing.length > 3 ? "…" : ""}</span>{/if}
          </div>
          <details>
            <summary class="mut">Ingrédients</summary>
            {#snippet tree(lines: CraftLine[])}
              <ul>
                {#each lines as l}
                  <li>
                    {l.qty}× {l.name}
                    <span class="mut">{l.unit != null ? kamas(Math.round(l.unit)) + " l'unité" + (l.how === "craft" ? " · crafté" : "") : "prix inconnu"}</span>
                    {#if l.sub}{@render tree(l.sub)}{/if}
                  </li>
                {/each}
              </ul>
            {/snippet}
            {@render tree(craft.lines)}
          </details>
        </div>
      {/if}
      {#if points.length >= 3}
        <div class="graph"><DensityChart {points} {fit} faded={fadedIds} equipment={!comp && !mount} selected={tab === "trans" && cand ? cand.id : picked} onselect={focusLot} /></div>
      {/if}
      {#if !comp}<div class="tabs">
        <button class:on={tab === "listing"} onclick={() => (tab = "listing")}>Liste</button>
        <button class:on={tab === "trans"} onclick={() => (tab = "trans")}>Transcendance{transCount ? ` · ${transCount}` : ""}</button>
      </div>{/if}
      {#if tab === "trans" && !comp}
        <div class="trans">
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
                  {#if t.avgOnly}<span class="warn" title="Ouvrez la rune à l'hôtel de vente pour relever son prix">⚠ prix moyen sur 7 jours seulement</span>{:else if t.stale}<span class="warn" title="Ouvrez la rune à l'hôtel de vente pour actualiser son prix">⚠ prix de la rune vieux de plus d'un jour</span>{/if}{/if}
              </div>
            {/each}
            {#if trans.unpricedCount}<div class="mut note">{trans.unpricedCount} lots candidats ne peuvent pas être classés tant que leur rune n'a pas de prix enregistré.</div>{/if}
            <div class="ghead">Candidats rentables{#if focus} <button class="mini" onclick={() => (focus = null)}>Toutes les runes ✕</button>{/if}</div>
            {#each trans.opportunities as o (keyOf(o.row))}
              {@const t = o.best!.target}
              {@const cid = keyOf(o.row)}
              <div class="opp">
                <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
                <div class="oh sel" class:picked={cand?.id === cid} title="Cliquer pour mettre en avant ce lot et les lots avec la même rune sur le graphique" onclick={() => (cand = cand?.id === cid ? null : { id: cid, a: t.a, value: t.value })}>
                  <b class="ok">+{kamas(o.best!.profit)}</b>
                  <span class="inv" title="Prix du lot + prix de la rune">investissement {kamas(o.buy + (t.runeCost ?? 0))}</span>
                  <span>ajouter {t.rune?.name} (+{t.value} {effLabel(t.a)})</span>
                  {#if t.avgOnly}<span class="warn" title="Prix moyen sur 7 jours, jamais relevé à l'hôtel de vente">⚠ prix de rune moyen</span>{:else if t.stale}<span class="warn" title="Prix de la rune vieux de plus d'un jour">⚠ prix de rune périmé</span>{/if}
                  <button class="mini" class:on={isSaved(o)} title="Garder ce candidat pour y revenir plus tard" onclick={(ev) => (ev.stopPropagation(), toggleSaved(o))}>{isSaved(o) ? "Enregistré ✓" : "Enregistrer"}</button>
                  <span class="mut">achat {kamas(o.buy)} · rune {kamas(t.runeCost ?? 0)} · vente ~{kamas(t.sale)} · densité {o.z >= 0 ? "+" : ""}{o.z.toFixed(1)}σ</span>
                </div>
                <ItemCard row={o.row} collapsible open={openSet.has("t" + cid)} ontoggle={() => toggleOpen("t" + cid)} />
              </div>
            {:else}
              <div class="mut">Aucun candidat rentable pour le moment.</div>
            {/each}
          {/if}
        </div>
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
      {#snippet lotCard(r: any)}
        {@const key = keyOf(r)}
        <div class="lot" class:picked={picked === key} class:faded={lacksFade(r)}>
          <ItemCard row={r} companion={comp} collapsible open={allOpen || openSet.has(key)} ontoggle={() => toggleOpen(key)} {fade} onfade={toggleFade} pinned={isPin(r)} onpin={comp ? undefined : () => togglePin(r)} versus={versusFor(r)} />
        </div>
      {/snippet}
      {#if rows.length >= VIRTUAL_FROM}
        <!-- long listings: only the lots on screen are rendered, no reorder animation -->
        <VirtualList bind:this={vlist} items={rows} key={keyOf} id={(r) => "lot-" + keyOf(r)} estimate={85} row={lotCard} />
      {:else}
        {#each rows as r (keyOf(r))}
          <div id="lot-{keyOf(r)}" animate:flip={{ duration: 320 }}>{@render lotCard(r)}</div>
        {:else}
          <div class="mut">Aucun lot ne correspond au filtre</div>
        {/each}
      {/if}
      {/if}
      {/if}
    {/if}
  </SmoothScroll>

  <Grip {cols} name="filter" dir={-1} />

  <div class="filter" class:cz={cols.collapsed.filter}>
    <PanelHead {cols} name="filter" title="Filtres" dir={-1} />
    {#if pin}
      <div class="pinned">
        <div class="fh">Objet épinglé <span class="mut">· {pin.item.name}{pin.item.level != null ? " niv. " + pin.item.level : ""}</span></div>
        <ItemCard row={pin.row} companion={!!pin.item.companion} pinned onpin={() => (pin = null)} />
      </div>
    {/if}
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
        <div class="ghead">
          <span>{g.label}</span>
          <span class="gbtns">
            {#if g.key !== "added"}
              <button class="mini" onclick={() => g.items.forEach((x) => setMin(x.a, x.def))} title="Remplir le min de tous les effets de cette section avec le bas de leur plage de base">Jet min</button>
              <button class="mini" onclick={() => g.items.forEach((x) => setPerf(x.a, x.def))} title="Remplir le min de tous les effets de cette section avec le max de leur plage de base (jet parfait)">Jet parfait</button>
            {/if}
            <button class="mini" disabled={!g.items.some((x) => active(x.a))} onclick={() => g.items.forEach((x) => clearEffect(x.a))} title="Effacer les filtres de cette section">Effacer</button>
          </span>
        </div>
        {#each g.items as x (x.a)}
          {@const y = flist(item.gid).find((z) => z.a === x.a)}
          <div class="frow" class:on={active(x.a)}>
            <div class="lb">{#if iconOf(x.a)}<img class="sic" src={iconOf(x.a)} alt="" />{/if}{x.label}</div>
            {#if FLAG_EFFECTS.has(x.a)}
              <label class="tog"><input type="checkbox" checked={active(x.a)} onchange={(e) => (filterOf(item.gid, x.a).min = e.currentTarget.checked ? "1" : "")} /> Uniquement les lots qui l'ont</label>
            {:else}
            <small class="rg">{rangeLabel(x.def)}</small>
            <input type="number" placeholder="min" value={y?.min ?? ""} oninput={(e) => (filterOf(item.gid, x.a).min = e.currentTarget.value)} />
            <input type="number" placeholder="max" value={y?.max ?? ""} oninput={(e) => (filterOf(item.gid, x.a).max = e.currentTarget.value)} />
            {#if x.def == null}
              <button class="mini" onclick={() => (filterOf(item.gid, x.a).min = "1")} title="Remplir le min avec 1 (lots qui ont cet effet)">+1</button>
            {:else}
              <button class="mini" onclick={() => setMin(x.a, x.def)} title="Remplir le min avec le bas de la plage de base">Jet min</button>
              <button class="mini" onclick={() => setPerf(x.a, x.def)} title="Remplir le min avec le max de la plage de base (jet parfait)">Jet parfait</button>
            {/if}
            <button class="mini" disabled={!active(x.a)} onclick={() => clearEffect(x.a)}>Effacer</button>
            {/if}
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
  .view { flex: 1; min-height: 0; display: grid; }
  .deal-ico { display: inline-block; width: 8px; height: 8px; margin-left: 4px; border-radius: 50%; background: var(--ok); vertical-align: middle; position: relative; top: -1px; font-size: 0; }
  .craft { margin: 0 0 10px; padding: 6px 10px; border: 1px solid var(--bad); border-radius: 8px; font-family: var(--font); font-size: 13px; }
  .craft.pos { border-color: var(--ok); }
  .craft .crow { display: flex; gap: 14px; flex-wrap: wrap; align-items: baseline; }
  .craft .gain b, .craft .gain { color: var(--ok); }
  .craft .loss b, .craft .loss { color: var(--bad); }
  .craft details { margin-top: 2px; font-size: 12px; }
  .craft summary { cursor: pointer; }
  .craft ul { margin: 2px 0; padding-left: 18px; }
  /* item types of the left list: sticky under the search bar */
  .view :global(.chd.thead) { border-bottom: 1px solid var(--line); background: var(--bg); padding: 5px 10px; font: 600 11px var(--font); letter-spacing: 0.04em; text-transform: uppercase; position: sticky; top: 66px; z-index: 1; }
  .view > :global(.wrap) { font-family: var(--font); }
  .sort { display: inline-flex; gap: 6px; align-items: center; }
  .arrow { display: inline-block; transition: transform 0.32s cubic-bezier(0.3, 1.4, 0.5, 1); }
  .arrow.desc { transform: rotate(180deg); }
  .head { display: flex; align-items: center; gap: 10px; margin-bottom: 10px; flex-wrap: wrap; }
  .head .fh { margin: 0; flex: 1; }
  .filter :global(.phead) { padding-left: 0; padding-right: 0; }
  .filter { overflow: auto; background: var(--panel); border-left: 1px solid var(--line); padding: 0 10px 10px; font-family: var(--font); font-size: 12px; }
  .filter .pinned { margin-bottom: 12px; }
  .filter .ghead { display: flex; align-items: center; justify-content: flex-start; gap: 10px; }
  .ghead { margin: 12px 0 2px; font-size: 10px; letter-spacing: 0.08em; text-transform: uppercase; color: var(--mut); }
  .gbtns { display: flex; gap: 4px; text-transform: none; letter-spacing: 0; }
  .graph { margin-bottom: 12px; }
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
  .link { border: 0; background: none; color: var(--acc); padding: 0; text-decoration: underline; }
  .oh .on { border-color: var(--acc); color: var(--acc); }
  /* one frame around the gain line and the lot; the card inside drops its own edge */
  .opp { margin-bottom: 12px; padding: 6px; border: 1px solid var(--line); border-radius: 10px; }
  .opp :global(.card) { border: 0; padding: 0 4px 4px; margin-bottom: 0; }
  .opp .oh { margin-bottom: 0; }
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
  .frow .tog { grid-column: 2 / -1; display: flex; gap: 6px; align-items: center; }
  .frow .tog input { width: auto; }
  .frow .sic { width: 16px; height: 16px; vertical-align: -3px; margin-right: 5px; }
  .frow .lb { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .frow .rg { color: var(--mut); text-align: right; white-space: nowrap; padding-right: 2px; }
  .frow.on .lb { font-weight: 700; color: var(--acc); }
</style>
