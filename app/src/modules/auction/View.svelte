<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";
  import { ago, call, kamas, on } from "../../lib/api";
  import { flip } from "svelte/animate";
  import ItemCard from "../../lib/components/ItemCard.svelte";
  import SmoothScroll from "../../lib/components/SmoothScroll.svelte";

  interface Filter { a: number; min: string | number; max: string | number }
  interface Eff { id: number; label: string; group: string }

  let items = new SvelteMap<number, any>();
  let selGid = $state<number | null>(null);
  let allEffects = $state<Eff[]>([]);
  let filters = $state<Record<number, Filter[]>>({}); // gid -> filters
  let openSet = new SvelteSet<number>();
  let allOpen = $state(false);
  let sortDir = $state(1);
  let freeEff = $state("");

  const effById = $derived(new Map(allEffects.map((e) => [e.id, e])));
  const list = $derived([...items.values()].sort((a, b) => b.updated - a.updated));
  const item = $derived(selGid != null ? items.get(selGid) : undefined);

  const effLabel = (a: number) => effById.get(a)?.label ?? `effect ${a}`;
  const unitPrice = (r: any) => {
    const v = r.prices.filter((p: any) => p.price > 0).map((p: any) => p.price / p.qty);
    return v.length ? Math.min(...v) : Infinity;
  };
  const lotVal = (r: any, a: number) => Math.max(0, ...r.effects.filter((e: any) => e.action === a && e.status !== "removed").map((e: any) => e.hi ?? 0));
  const flist = (gid: number) => filters[gid] ?? [];

  /** All effects of the item: defaults first, then any seen in the lots, then manually added ones. */
  const effectRows = $derived.by(() => {
    if (!item) return [];
    const seen = new Map<number, { a: number; def: number[] | null }>();
    for (const d of item.defaults ?? []) seen.set(d.a, { a: d.a, def: [d.min, d.max] });
    for (const r of item.rows) for (const e of r.effects) if (e.status !== "removed" && !seen.has(e.action)) seen.set(e.action, { a: e.action, def: e.range ?? null });
    for (const x of flist(item.gid)) if (!seen.has(x.a)) seen.set(x.a, { a: x.a, def: null });
    return [...seen.values()];
  });

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
    if (p === "add") {
      const m = freeEff.match(/#(\d+)$/);
      const a = m ? +m[1] : allEffects.find((x) => x.label.toLowerCase() === freeEff.trim().toLowerCase())?.id;
      if (a) filterOf(item.gid, a);
      return void (freeEff = "");
    }
    for (const x of effectRows) {
      if (!x.def) continue; // presets only touch the item's default effects (perfect = default max)
      const y = filterOf(item.gid, x.a);
      if (p === "min") y.min = x.def[0];
      else if (p === effById.get(x.a)?.group) y.min = x.def[1];
    }
  }

  function toggleOpen(uid: number) {
    openSet.has(uid) ? openSet.delete(uid) : openSet.add(uid);
  }
  async function forget() {
    if (confirm("Forget all stored items?")) await call("auction", "clear");
  }
  function put(i: any) {
    items.set(i.gid, i);
    if (selGid === null) selGid = i.gid;
  }

  onMount(() => {
    call<Eff[]>("auction", "effects").then((e) => (allEffects = e));
    call<any[]>("auction", "items").then((l) => {
      l.forEach(put);
      if (selGid === null && l.length) selGid = l[0].gid;
    });
    const offs = [
      on("auction:item", put),
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
    {#each list as i (i.gid)}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class:sel={selGid === i.gid} onclick={() => (selGid = i.gid)}>
        <b>{i.name}</b> <span class="mut">niv. {i.level ?? "?"}</span><br />
        <span class="mut">{i.type ?? ""} · {i.lots} lots · {i.min_price ? "min " + kamas(i.min_price) + " kamas" : "—"} · {ago(i.updated)} ago{i.demo ? " · demo" : ""}</span>
      </div>
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
        <button onclick={forget}>Forget all items</button>
      </div>
      {#each rows as r (r.uid ?? JSON.stringify(r.prices) + r.name)}
        <div animate:flip={{ duration: 320 }}>
          <ItemCard row={r} collapsible open={allOpen || openSet.has(r.uid)} ontoggle={() => toggleOpen(r.uid)} />
        </div>
      {:else}
        <div class="mut">No lot matches the filter</div>
      {/each}
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
      <div class="btns">
        <input list="effList" placeholder="Add any effect…" bind:value={freeEff} style="flex:1;min-width:0" />
        <button onclick={() => preset("add")}>Add</button>
      </div>
      <datalist id="effList">{#each allEffects as e}<option value="{e.label} #{e.id}"></option>{/each}</datalist>
      {#each effectRows as x (x.a)}
        {@const y = flist(item.gid).find((z) => z.a === x.a)}
        <div class="frow" class:on={active(x.a)}>
          <div class="lb" title="effect {x.a}">{effLabel(x.a)}</div>
          <small class="rg">{x.def ? `[${x.def[0]}–${x.def[1]}]` : ""}</small>
          <input type="number" placeholder="min" value={y?.min ?? ""} oninput={(e) => (filterOf(item.gid, x.a).min = e.currentTarget.value)} />
          <input type="number" placeholder="max" value={y?.max ?? ""} oninput={(e) => (filterOf(item.gid, x.a).max = e.currentTarget.value)} />
        </div>
      {:else}
        <div class="mut">No effects</div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .view { flex: 1; min-height: 0; display: grid; grid-template-columns: 280px 1fr 320px; }
  .ilist { overflow: auto; background: var(--panel); border-right: 1px solid var(--line); }
  .ilist div { padding: 6px 10px; border-bottom: 1px solid var(--line); cursor: pointer; font-family: system-ui, sans-serif; }
  .ilist div:hover { background: var(--bg); }
  .ilist div.sel { outline: 1px solid var(--acc); }
  .ilist .empty { cursor: default; }
  .view > :global(.wrap) { font-family: system-ui, sans-serif; }
  .sort { display: inline-flex; gap: 6px; align-items: center; }
  .arrow { display: inline-block; transition: transform 0.32s cubic-bezier(0.3, 1.4, 0.5, 1); }
  .arrow.desc { transform: rotate(180deg); }
  .head { display: flex; align-items: center; gap: 10px; margin-bottom: 10px; flex-wrap: wrap; }
  .head .fh { margin: 0; flex: 1; }
  .filter { overflow: auto; background: var(--panel); border-left: 1px solid var(--line); padding: 10px; font-family: system-ui, sans-serif; font-size: 12px; }
  .btns { display: flex; gap: 6px; flex-wrap: wrap; margin-bottom: 8px; }
  .frow { display: grid; grid-template-columns: 1fr auto 54px 54px; gap: 4px; align-items: center; margin: 3px 0; }
  .frow input { width: 100%; padding: 2px 4px; }
  .frow .lb { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .frow .rg { color: var(--mut); text-align: right; white-space: nowrap; padding-right: 2px; }
  .frow.on .lb { font-weight: 700; color: var(--acc); }
</style>
