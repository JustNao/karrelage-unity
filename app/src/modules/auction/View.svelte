<script lang="ts">
  import { onMount, tick } from "svelte";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";
  import { ago, call, kamas, on } from "../../lib/api";
  import { flip } from "svelte/animate";
  import DensityChart from "../../lib/components/DensityChart.svelte";
  import EffectPicker from "../../lib/components/EffectPicker.svelte";
  import ItemCard from "../../lib/components/ItemCard.svelte";
  import SmoothScroll from "../../lib/components/SmoothScroll.svelte";
  import { analyse, isGoodDeal } from "../../lib/density";
  import { rangeLabel, sections, type EffectMeta } from "../../lib/effects";
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
  });
  // Pinned lot (kept across items): every other lot shown gets a diff against it.
  let pin = $state.raw<{ item: any; row: any } | null>(null);
  const isPin = (r: any) => !!pin && !!item && pin.item.gid === item.gid && pin.row.uid === r.uid;
  const togglePin = (r: any) => (pin = isPin(r) ? null : { item, row: r });
  /** What a lot is compared with: the pinned lot (never shown against itself). */
  const versusFor = (r: any) => (pin && !isPin(r) ? { pin: pin.row, name: pin.item.name, unitPrice } : null);
  const lacksFade = (r: any) => fade !== null && !r.effects.some((e: any) => e.action === fade && e.status !== "removed");
  const toggleFade = (a: number) => (fade = fade === a ? null : a);
  let tab = $state<"listing" | "trans">("listing");
  let dealsOpen = $state(false); // "Good deals" starts collapsed
  let focus = $state<string | null>(null); // transcended effect (targetKey) the candidates are limited to
  let runes = $state.raw<Rune[]>([]);
  let stored = new SvelteMap<number, StoredPrice>(); // stored prices (the rune prices feed the transcendance tab)
  let allOpen = $state(false);
  let sortDir = $state(1);

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
  const typeGroups = $derived.by(() => {
    const m = new Map<string, any[]>();
    for (const i of list) {
      const t = i.type ?? "?";
      if (!m.has(t)) m.set(t, []);
      m.get(t)!.push(i);
    }
    return [...m.entries()].sort((a, b) => a[0].localeCompare(b[0], "fr")).map(([type, items]) => ({ type, items }));
  });
  const item = $derived(selGid != null ? items.get(selGid) : undefined);

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
  const points = $derived(
    rows
      .map((r: any, i: number) => ({
        id: lotKey(r, i),
        x: unitPrice(r),
        y: r.density,
        label: r.name,
        added: r.effects.some((e: any) => e.status === "added"),
        missing: r.effects.some((e: any) => e.status === "removed"),
        faded: lacksFade(r),
      }))
      .filter((p: any) => Number.isFinite(p.x) && Number.isFinite(p.y)),
  );

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

  const trans = $derived(transcendance(rows, unitPrice, runes, stored, focus));
  const runeName = (n: string) => n;

  /** Clicking a dot: unroll the lot and scroll the list down to it (nothing is filtered out). */
  async function focusLot(id: string | number) {
    picked = id;
    openSet.add(id);
    await tick();
    document.getElementById(`lot-${id}`)?.scrollIntoView({ behavior: "smooth", block: "center" });
  }
  async function forget() {
    if (confirm("Forget all stored items?")) await call("auction", "clear");
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
    {#each typeGroups as g (g.type)}
      {@const shut = collapsed.has(g.type)}
      <button class="thead" aria-expanded={!shut} onclick={() => toggleType(g.type)}>
        <span class="chev" class:shut>▾</span> {g.type} <span class="mut">{g.items.length}</span>
      </button>
      {#if !shut}
        {#each g.items as i (i.gid)}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="irow" class:sel={selGid === i.gid} onclick={() => (selGid = i.gid)}>
            <b>{i.name}</b> <span class="mut">niv. {i.level ?? "?"}</span><br />
            <span class="mut">{i.lots} lots · {i.min_price ? "min " + kamas(i.min_price) + " kamas" : "—"} · {ago(i.updated)} ago{i.demo ? " · demo" : ""}</span>
          </div>
        {/each}
      {/if}
    {:else}
      <div class="mut empty">Open an item in the auction house: it appears here (latest listing per item).</div>
    {/each}
  </div>

  <SmoothScroll>
    {#if item}
      <div class="head">
        <div class="fh">{item.name} <span class="mut">{item.type ?? ""} · gid {item.gid} · updated {ago(item.updated)} ago · {rows.length}/{item.lots} lots</span></div>
        <button class="sort" onclick={() => (sortDir = -sortDir)} title="Click to flip the price order">
          Price <span class="arrow" class:desc={sortDir === -1}>↑</span>
        </button>
        <button onclick={() => ((allOpen = !allOpen), openSet.clear())}>{allOpen ? "Collapse all" : "Expand all"}</button>
        {#if pin}<button class="on" onclick={() => (pin = null)} title="Stop comparing lots with the pinned lot">Unpin {pin.item.name}</button>{/if}
        <button onclick={forget}>Forget all items</button>
      </div>
      {#if points.length >= 3}
        <div class="graph"><DensityChart {points} selected={picked} onselect={focusLot} /></div>
      {/if}
      <div class="tabs">
        <button class:on={tab === "listing"} onclick={() => (tab = "listing")}>Listing</button>
        <button class:on={tab === "trans"} onclick={() => (tab = "trans")}>Transcendance{trans.opportunities.length ? ` · ${trans.opportunities.length}` : ""}</button>
      </div>
      {#if tab === "trans"}
        <div class="trans">
          <div class="mut note">
            Lots of this item with the transcendance lock and an added effect show what a rune is worth. Candidates are unlocked lots
            without an overmax effect; profit = sale price − {AH_TAX * 100}% tax − lot price − rune price (Stored prices tab).
          </div>
          {#if trans.targets.length === 0}
            <div class="mut">No transcended lot of this item in the listing: nothing to compare with.</div>
          {:else}
            <div class="ghead">Transcended lots seen</div>
            {#each trans.targets as t (targetKey(t))}
              <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
              <div class="trow" class:foc={focus === targetKey(t)} title="Click to show only candidates for this rune" onclick={() => (focus = focus === targetKey(t) ? null : targetKey(t))}>
                <b>+{t.value} {effLabel(t.a)}</b>
                <span class="mut">{t.lots} {t.lots === 1 ? "lot" : "lots"} · from {kamas(t.sale)} ·</span>
                {#if !t.rune}<span class="warn">no matching rune</span>
                {:else if t.runeCost == null}<span class="warn">{runeName(t.rune.name)}: price unknown, open it in the auction house</span>
                {:else}<span class="mut">{runeName(t.rune.name)} {kamas(t.runeCost)}</span>
                  {#if t.stale}<span class="warn" title="Open the rune in the auction house to refresh its price">⚠ rune price older than a day</span>{/if}{/if}
              </div>
            {/each}
            {#if trans.unpricedCount}<div class="mut note">{trans.unpricedCount} candidate lots cannot be ranked until their rune has a stored price.</div>{/if}
            <div class="ghead">Profitable candidates{#if focus} <button class="mini" onclick={() => (focus = null)}>Show all runes ✕</button>{/if}</div>
            {#each trans.opportunities as o (o.row.uid ?? o.buy + "-" + o.row.density)}
              {@const t = o.best!.target}
              <div class="opp">
                <div class="oh">
                  <b class="ok">+{kamas(o.best!.profit)}</b>
                  <span>add {t.rune?.name} (+{t.value} {effLabel(t.a)})</span>
                  {#if t.stale}<span class="warn" title="Rune price older than a day">⚠ stale rune price</span>{/if}
                  <span class="mut">buy {kamas(o.buy)} · rune {kamas(t.runeCost ?? 0)} · sell ~{kamas(t.sale)} · density {o.z >= 0 ? "+" : ""}{o.z.toFixed(1)}σ</span>
                </div>
                <ItemCard row={o.row} collapsible open={openSet.has("t" + (o.row.uid ?? ""))} ontoggle={() => toggleOpen("t" + (o.row.uid ?? ""))} />
              </div>
            {:else}
              <div class="mut">No profitable candidate right now.</div>
            {/each}
          {/if}
        </div>
      {:else}
      {#if deals.length}
        <div class="deals">
          <button class="dhead" aria-expanded={dealsOpen} onclick={() => (dealsOpen = !dealsOpen)}>
            <span class="chev" class:shut={!dealsOpen}>▾</span> Good deals <span class="mut">· {deals.length} {deals.length === 1 ? "lot stands" : "lots stand"} out above the price trend</span>
          </button>
          {#each dealsOpen ? deals : [] as d (d.key)}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <div class="deal" class:faded={lacksFade(d.row)} title="Click to jump to the lot in the list" onclick={() => focusLot(d.key)}>
              <ItemCard row={d.row} {fade} onfade={toggleFade} pinned={isPin(d.row)} onpin={() => togglePin(d.row)} versus={versusFor(d.row)} />
            </div>
          {/each}
        </div>
      {/if}
      {#each rows as r, i (lotKey(r, i))}
        {@const key = lotKey(r, i)}
        <div id="lot-{key}" class="lot" class:picked={picked === key} class:faded={lacksFade(r)} animate:flip={{ duration: 320 }}>
          <ItemCard row={r} collapsible open={allOpen || openSet.has(key)} ontoggle={() => toggleOpen(key)} {fade} onfade={toggleFade} pinned={isPin(r)} onpin={() => togglePin(r)} versus={versusFor(r)} />
        </div>
      {:else}
        <div class="mut">No lot matches the filter</div>
      {/each}
      {/if}
    {/if}
  </SmoothScroll>

  <div class="filter">
    {#if item}
      <div class="fh">Filter effects</div>
      <div class="btns">
        <button onclick={() => preset("min")} title="min = lowest value of each effect's default range">Min values</button>
        <button onclick={() => preset("damage")} title="perfect (default max) for damage effects">Damage</button>
        <button onclick={() => preset("resist")} title="perfect (default max) for % resistances">Résistances</button>
        <button onclick={() => preset("clear")}>Clear</button>
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
            <button class="mini" onclick={() => setMin(x.a, x.def)} title="Fill min with the low end of the default range">Set min</button>
            <button class="mini" disabled={!active(x.a)} onclick={() => clearEffect(x.a)}>Clear</button>
          </div>
        {/each}
      {:else}
        <div class="mut">No effects</div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .view { flex: 1; min-height: 0; display: grid; grid-template-columns: 280px 1fr 470px; }
  .ilist { overflow: auto; background: var(--panel); border-right: 1px solid var(--line); }
  .thead { display: flex; width: 100%; align-items: center; gap: 6px; border: 0; border-bottom: 1px solid var(--line); border-radius: 0; background: var(--bg); color: inherit; padding: 5px 10px; font: 600 11px system-ui; letter-spacing: 0.04em; text-transform: uppercase; text-align: left; position: sticky; top: 0; z-index: 1; }
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
  .warn { color: var(--warn, orange); }
  .opp { margin-bottom: 12px; }
  .oh { display: flex; gap: 10px; align-items: baseline; flex-wrap: wrap; font-size: 12px; margin-bottom: 4px; }
  .oh .ok { color: var(--ok); font-size: 14px; }
  .btns { display: flex; gap: 6px; flex-wrap: wrap; margin-bottom: 8px; }
  .frow { display: grid; grid-template-columns: 1fr auto 52px 52px auto auto; gap: 4px; align-items: center; margin: 3px 0; }
  .frow input { width: 100%; padding: 2px 4px; }
  .frow .lb { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .frow .rg { color: var(--mut); text-align: right; white-space: nowrap; padding-right: 2px; }
  .frow.on .lb { font-weight: 700; color: var(--acc); }
</style>
