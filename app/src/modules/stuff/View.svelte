<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";
  import { call } from "../../lib/api";
  import { kamas } from "../../lib/fmt";
  import { auction } from "../../lib/stores/auction.svelte";
  import { prices } from "../../lib/stores/prices.svelte";
  import { inventory } from "../../lib/stores/inventory.svelte";
  import ConfirmButton from "../../lib/components/ConfirmButton.svelte";
  import Grip from "../../lib/components/Grip.svelte";
  import ProgressBar from "../../lib/components/ProgressBar.svelte";
  import SideList from "../../lib/components/SideList.svelte";
  import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { Columns } from "../../lib/columns.svelte";
  import { SCAN_LIST_KEY, type ScanItem } from "../../lib/scanlist";
  import type { Recipes } from "../../lib/craft";
  import ItemCard from "../../lib/components/ItemCard.svelte";
  import LotTable from "../../lib/components/LotTable.svelte";
  import SmoothScroll from "../../lib/components/SmoothScroll.svelte";
  import {
    CARAC, FM_STATS, className, FRESH_SECS, HP_STAT, PANELS, SLOT_LAYOUT, SLOT_TITLES, STAT_ICON_CODES, STAT_LABEL, compute, fmOf, itemStats, lotStats, lotUnit, slotPrices, solve, sumSlots,
    type Build, type Info, type Lot, type Resolved, type SolveResult, type StatRow,
  } from "../../lib/stuff";

  const cols = new Columns("karrelage.cols.stuff", { left: 230 });

  let builds = $state<Build[]>([]);
  let sel = $state<string | null>(null);
  let link = $state("");
  let busy = $state(false);
  let err = $state("");
  let renaming = $state<string | null>(null);
  let draft = $state("");
  let info = $state.raw<Info>({ items: {}, sets: {} });
  let recipes = $state.raw<Recipes>({ recipes: {}, names: {} });
  let icons = new SvelteMap<string, string>();
  let result = $state<SolveResult | null | "none">(null);
  let solving = $state(false);
  let progress = $state(0);
  let cancelled = $state(false);
  let worker: Worker | null = null;
  let now = $state(Date.now() / 1000);

  const build = $derived(builds.find((b) => b.id === sel) ?? null);
  const slotIds = $derived(build ? Object.keys(build.slots) : []);

  const lotsOf = (slot: string): Lot[] => {
    const gid = build?.slots[slot]?.gid;
    return gid != null ? ((auction.items.get(gid)?.rows ?? []) as Lot[]) : [];
  };
  const fresh = (slot: string) => {
    const gid = build?.slots[slot]?.gid;
    const t = gid != null ? auction.items.get(gid)?.updated : null;
    return t != null && now - t <= FRESH_SECS;
  };
  const chosenLot = (slot: string) => lotsOf(slot).find((l) => l.uid === build?.chosen[slot]) ?? null;
  /** The user's own copy an owned slot counts at (still in the inventory), else undefined = perfect roll. */
  const ownLot = (slot: string) => {
    if (!build || !owned.has(slot)) return undefined;
    const it = inventory.byUid(build.own_uid?.[slot]);
    return it && it.gid === build.slots[slot].gid ? it : undefined;
  };

  const resolveWith = (real: boolean): Resolved[] =>
    slotIds.map((slot) => {
      const gid = build!.slots[slot].gid;
      // an item counted off the price is taken at the roll of the user's own copy, else at its perfect roll (max level for a familiar / mount)
      const lot = real ? (ignored.has(slot) ? ownLot(slot) ?? null : chosenLot(slot)) : null;
      return { slot, gid, lot, stats: lot ? lotStats(lot) : itemStats(info.items[gid] ?? ({ effects: [] } as any)) };
    });
  const real = $derived(build ? compute(build, info, resolveWith(true)) : null);
  const max = $derived(build ? compute(build, info, resolveWith(false)) : null);
  const anyChosen = $derived(!!build && slotIds.some((s) => chosenLot(s) || ownLot(s)));

  const DOFUS_SLOTS = ["d1", "d2", "d3", "d4", "d5", "d6"];
  const DOFUS_TYPE = 23; // itemtypes: Dofus (trophées 151 and prysmaradites share the slots and are not touched)
  const dofusSlots = $derived(DOFUS_SLOTS.filter((s) => info.items[build?.slots[s]?.gid ?? -1]?.type_id === DOFUS_TYPE));
  const owned = $derived(new Set(build?.owned ?? []));
  /** Slots not bought: the items the user already owns, counted at their perfect roll. */
  const ignored = $derived(owned);
  /** The search needs a scanned listing for every item that is bought. */
  const unscanned = $derived(slotIds.filter((s) => !ignored.has(s) && !fresh(s)));
  const perSlot = $derived(
    Object.fromEntries(slotIds.map((s) => [s, slotPrices(build!.slots[s].gid, lotsOf(s), build!.chosen[s], recipes, prices.map)])),
  );
  const craft = $derived(sumSlots(slotIds, ignored, (s) => perSlot[s].craft));
  const estSlots = $derived(slotIds.filter((s) => !ignored.has(s) && perSlot[s].estimated));
  const first = $derived(sumSlots(slotIds, ignored, (s) => perSlot[s].first));
  const picked = $derived(sumSlots(slotIds, ignored, (s) => perSlot[s].chosen));
  const names = (l: string[]) => l.map((s) => SLOT_TITLES[s]).join(", ");

  /** Items of the build not scanned in the last hour (red border), one entry per item. */
  const toScan = $derived([...new Map(slotIds.filter((s) => !ignored.has(s) && !fresh(s)).map((s) => [build!.slots[s].gid, { gid: build!.slots[s].gid, name: build!.slots[s].name, group: s === "fa" || s === "mo" ? "companion" : "equip" } as ScanItem])).values()]);
  async function openScanList() {
    try { localStorage.setItem(SCAN_LIST_KEY, JSON.stringify(toScan)); } catch {}
    const open = await WebviewWindow.getByLabel("scanlist");
    if (open) { await open.setFocus(); return; } // it picks the new list up through the storage event
    new WebviewWindow("scanlist", { url: "scanlist.html", title: "À scanner", width: 280, height: 380, decorations: false, transparent: true, shadow: false, alwaysOnTop: true, skipTaskbar: true, resizable: false });
  }
  const caracCode = (stat: number) => CARAC.find(([, st]) => st === stat)?.[0];
  const icon = (kind: "item" | "stat", key: string | number) => icons.get(`${kind}:${key}`);
  const val = (c: typeof real, s: number) => c?.totals.get(s) ?? 0;

  /** Request token: only the latest build's data is applied (clicking A then B quickly must not end up with A's items). */
  let infoReq = 0;
  async function loadInfo(b: Build) {
    const req = ++infoReq;
    const gids = Object.values(b.slots).map((s) => s.gid);
    const got = await call<Info>("stuff", "item_info", { gids });
    if (req !== infoReq) return;
    info = got;
    const pics = await call<Record<string, string>>("stuff", "icons", { item: Object.values(b.slots).map((s) => s.picture), stat: STAT_ICON_CODES });
    if (req !== infoReq) return;
    // only the current build's icons are kept (they are data URLs: the map would otherwise grow with every build opened)
    for (const k of [...icons.keys()]) if (k.startsWith("item:") && !(k in pics)) icons.delete(k);
    for (const [k, v] of Object.entries(pics)) icons.set(k, v);
  }

  async function select(id: string) {
    sel = id; result = null;
    try { await loadInfo(builds.find((b) => b.id === id)!); } catch (e) { if (sel === id) err = String(e); }
  }

  async function add() {
    if (!link.trim() || busy) return;
    busy = true; err = "";
    try {
      const b = await call<Build>("stuff", "import", { url: link });
      builds = [...builds, b];
      link = "";
      await select(b.id);
    } catch (e) { err = String(e); }
    busy = false;
  }

  const patch = (p: Partial<Pick<Build, "targets" | "chosen" | "ignored" | "owned" | "own_uid" | "fm">>) => {
    if (!build) return;
    Object.assign(build, p);
    call("stuff", "save_state", { id: build.id, patch: p }).catch((e) => (err = String(e)));
  };
  const rename = async (b: Build) => {
    const name = draft.trim();
    renaming = null;
    if (!name || name === b.name) return;
    try { await call("stuff", "rename", { id: b.id, name }); b.name = name; } catch (e) { err = String(e); }
  };
  const del = async (b: Build) => {
    await call("stuff", "delete", { id: b.id });
    builds = builds.filter((x) => x.id !== b.id);
    if (sel === b.id) sel = builds[0]?.id ?? null;
  };
  const refresh = async (b: Build) => {
    try {
      const fresh = await call<Build>("stuff", "refresh", { id: b.id });
      builds = builds.map((x) => (x.id === b.id ? fresh : x));
      if (sel === b.id) await loadInfo(fresh); // another build opened meanwhile: its data stays
    } catch (e) { err = String(e); }
  };

  /** "Sans les Dofus": marks every Dofus slot as owned (same as clicking each one), or unmarks them all. */
  const allDofusOwned = $derived(dofusSlots.length > 0 && dofusSlots.every((s) => owned.has(s)));
  const setDofusOwned = (on: boolean) => {
    const rest = (build!.owned ?? []).filter((s) => !dofusSlots.includes(s));
    const own = { ...build!.own_uid };
    for (const s of dofusSlots) if (on && !owned.has(s)) setCopy(own, s, firstCopy(s)); else if (!on) delete own[s];
    patch({ owned: on ? [...rest, ...dofusSlots] : rest, own_uid: own }); result = null;
  };
  /** The copy of a slot's item the character has (worn first), if any. */
  const firstCopy = (slot: string) => inventory.ofGid(build!.slots[slot].gid)[0]?.uid ?? null;
  const setCopy = (own: Record<string, number>, slot: string, uid: number | null) => { if (uid == null) delete own[slot]; else own[slot] = uid; };
  /** Owned at the roll of copy `uid` ("perfect" = perfect roll), or bought (`null`). */
  const setOwned = (slot: string, how: number | "perfect" | null) => {
    const o = (build!.owned ?? []).filter((s) => s !== slot);
    const own = { ...build!.own_uid };
    setCopy(own, slot, typeof how === "number" ? how : null);
    patch({ owned: how == null ? o : [...o, slot], own_uid: own });
    result = null;
  };
  /** ✕: owned (the character's own copy when it has one, else the perfect roll) or bought again. */
  const toggleOwned = (slot: string) => setOwned(slot, owned.has(slot) ? null : firstCopy(slot) ?? "perfect");

  /** Popup of a slot (click on its item): perfect roll, one of the character's copies, or to buy. Fixed position, so the scroll area does not clip it. */
  let pop = $state<{ slot: string; left: number; top?: number; bottom?: number; maxH: number } | null>(null);
  const POP_W = 330;
  function openPop(slot: string, el: HTMLElement) {
    if (pop?.slot === slot) { pop = null; return; }
    const r = el.getBoundingClientRect();
    const left = Math.max(8, Math.min(r.left, window.innerWidth - POP_W - 8));
    const below = window.innerHeight - r.bottom - 12;
    pop = below >= 260 || below >= r.top ? { slot, left, top: r.bottom + 4, maxH: below } : { slot, left, bottom: window.innerHeight - r.top + 4, maxH: r.top - 12 };
  }
  const closePop = (e: Event) => { if (pop && !(e.target instanceof Element && e.target.closest(".pop, .slot .ic"))) pop = null; };
  /** PA / PM / PO minimums start at the Dofusbook value of the build (once per build, so clearing them sticks). */
  const AUTOFILL = [1, 23, 19];
  const AUTOFILL_KEY = "karrelage.stuff.autofilled";
  $effect(() => {
    if (!build || !max || !slotIds.every((s) => info.items[build!.slots[s].gid])) return; // data of the items not loaded yet
    let done: string[] = [];
    try { done = JSON.parse(localStorage.getItem(AUTOFILL_KEY) ?? "[]"); } catch {}
    if (done.includes(build.id)) return;
    const targets = { ...build.targets };
    for (const s of AUTOFILL) if (targets[s] == null && (max.totals.get(s) ?? 0) > 0) targets[s] = max.totals.get(s)!;
    patch({ targets });
    try { localStorage.setItem(AUTOFILL_KEY, JSON.stringify([...done, build.id])); } catch {}
  });
  const setTarget = (stat: number, v: string) => {
    const t = { ...build!.targets };
    const n = parseInt(v, 10);
    if (Number.isFinite(n) && n > 0) t[stat] = n; else delete t[stat];
    patch({ targets: t });
    result = null;
  };

  const setFm = (code: string, v: string) => {
    const fm = { ...build!.fm };
    const n = parseInt(v, 10);
    if (Number.isFinite(n) && n !== 0) fm[code] = n; else delete fm[code];
    patch({ fm });
    result = null;
  };
  let fmAdd = $state("");
  const fmRows = $derived(Object.entries(build?.fm ?? {}).filter(([c, v]) => v && FM_STATS[c]));
  const fmFree = $derived(Object.entries(FM_STATS).filter(([c]) => !build?.fm?.[c]));

  /** The search runs in a worker (the UI stays responsive), reports its progress and can be cancelled. */
  function run() {
    solving = true; cancelled = false; progress = 0; result = null;
    const lots = Object.fromEntries(slotIds.map((s) => [s, lotsOf(s).map((l) => ({ uid: l.uid, gid: l.gid, prices: l.prices, effects: l.effects }))]));
    worker = new Worker(new URL("../../lib/stuffWorker.ts", import.meta.url), { type: "module" });
    worker.onmessage = (e: MessageEvent<{ progress?: number; done?: SolveResult | null }>) => {
      if (e.data.progress != null) progress = e.data.progress;
      else if ("done" in e.data) {
        result = e.data.done ?? "none";
        if (result !== "none") patch({ chosen: { ...build!.chosen, ...result.picks } }); // the combination found is applied right away
        stop();
      }
    };
    worker.onerror = (e) => { err = e.message || "erreur du calcul"; stop(); };
    const own = Object.fromEntries(slotIds.flatMap((s) => { const l = ownLot(s); return l ? [[s, { uid: l.uid, gid: l.gid, prices: [], effects: l.effects }]] : []; }));
    worker.postMessage({ build: { ...$state.snapshot(build!), ignored: [...ignored] }, info, lots, own, targets: $state.snapshot(build!.targets) });
  }
  function stop() { worker?.terminate(); worker = null; solving = false; }
  function cancel() { cancelled = true; stop(); }

  const rowsOf = (map: Record<string, number | null | undefined>) =>
    slotIds.flatMap((s) => {
      if (ignored.has(s)) return []; // owned: own copy or perfect roll, not a lot of the output
      const lot = map[s] != null ? lotsOf(s).find((l) => l.uid === map[s]) : null;
      return lot ? [{ slot: s, name: build!.slots[s].name, picture: build!.slots[s].picture, price: lotUnit(lot)!, lot }] : [];
    });
  /** The lots of the combination found, else the lots chosen in the build. */
  const tableRows = $derived(build ? rowsOf(result && result !== "none" ? result.picks : build.chosen) : []);
  /** Lots of the output unfolded in the table (by slot). */
  const openLots = new SvelteSet<string>();
  const toggleLot = (slot: string) => (openLots.has(slot) ? openLots.delete(slot) : openLots.add(slot));

  onMount(() => {
    const tick = setInterval(() => (now = Date.now() / 1000), 15000);
    let alive = true;
    call<Build[]>("stuff", "list").then((l) => { if (!alive) return; builds = l; if (l[0]) select(l[0].id); });
    call<Recipes>("auction", "recipes").then((r) => (recipes = r));
    void auction.ensure();
    void prices.ensure();
    void inventory.ensure();
    return () => {
      alive = false;
      clearInterval(tick);
      infoReq++; // a pending item_info / icons answer is dropped
      stop(); // the solver must not keep running (and patch the build) once the tab is gone
    };
  });
</script>

{#snippet statRow(s: StatRow)}
  {@const v = val(real, s.stat)}
  {@const m = val(max, s.stat)}
  {@const fm = build ? fmOf(build, s.stat) : 0}
  <div class="st">
    <b class:bad={anyChosen && v < m} class:good={anyChosen && v >= m} title={anyChosen ? `Max théorique : ${m}` : ""}>{v}</b>
    <span class="fm" title="Forgemagie">{fm ? `+${fm}` : ""}</span>
    {#if s.icon && icon("stat", s.icon)}<img src={icon("stat", s.icon)} alt="" />{:else}<span class="ph"></span>{/if}
    <span class="lab">{s.label}</span>
    {#if anyChosen && v !== m}<span class="mut">/ {m}</span>{/if}
    <button class="pf" title="Viser le maximum atteignable avec les objets ({m})" onclick={() => setTarget(s.stat, String(m))}>Max</button>
    <input class="tg" inputmode="numeric" placeholder="min" value={build?.targets[s.stat] ?? ""} onchange={(e) => setTarget(s.stat, e.currentTarget.value)} title="Valeur minimale voulue pour la recherche de la combinaison la moins chère" />
    {#if caracCode(s.stat)}
      {@const code = caracCode(s.stat)!}
      <span class="c4">{[10, 15, 13, 14].includes(s.stat) ? v + val(real, 25) : ""}</span>
      <span class="c4">{build?.carac[`base_${code}`] ?? 0}</span>
      <span class="c4">{build?.carac[`scroll_${code}`] ?? 0}</span>
    {/if}
  </div>
{/snippet}

{#snippet slot(id: string)}
  {#if build?.slots[id]}
    {@const sl = build.slots[id]}
    {@const lot = chosenLot(id)}
    {@const copies = inventory.ofGid(sl.gid)}
    {@const mine = ownLot(id)}
    <div class="slot" class:fresh={!owned.has(id) && fresh(id)} class:stale={!owned.has(id) && !fresh(id)} class:own={owned.has(id)} class:mine={!!mine} class:ign={ignored.has(id)}>
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="ic" title="{sl.name} — {SLOT_TITLES[id]}{mine ? ' (mon objet)' : owned.has(id) ? ' (jet parfait)' : lot ? ` (lot à ${kamas(lotUnit(lot)!)})` : ''}" onclick={(e) => openPop(id, e.currentTarget)}>
        {#if icon("item", sl.picture)}<img src={icon("item", sl.picture)} alt={sl.name} />{:else}<span class="mut">{SLOT_TITLES[id]}</span>{/if}
      </div>
      {#if copies.length}<span class="inv" title="Dans l'inventaire du personnage ({copies.some((c) => c.equipped) ? 'équipé' : 'sac'}{copies.length > 1 ? `, ${copies.length} exemplaires` : ''})"></span>{/if}
      <button class="ow" class:on={owned.has(id)} title={owned.has(id) ? "Je ne possède plus cet objet" : copies.length ? "Je possède déjà cet objet (mon exemplaire, pas acheté)" : "Je possède déjà cet objet (jet parfait, pas acheté)"} onclick={() => toggleOwned(id)}>✕</button>
    </div>
  {:else}
    <div class="slot empty"></div>
  {/if}
{/snippet}

<div class="grid cgrid" class:anim={!cols.dragging} style:grid-template-columns="{cols.px("left")}px 5px 1fr">
  <SideList {cols} name="left" title="Stuffs">
    {#snippet bar()}
      <input class="link" type="search" placeholder="Lien Dofusbook…" bind:value={link} onkeydown={(e) => e.key === "Enter" && add()} />
      <button class="mini" onclick={add} disabled={busy}>{busy ? "…" : "Ajouter"}</button>
    {/snippet}
    {#if err}<div class="bad err">{err}</div>{/if}
    {#each builds as b, i (b.id)}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="irow brow" class:first={i === 0} class:sel={b.id === sel} onclick={() => renaming !== b.id && select(b.id)}>
        {#if renaming === b.id}
          <input class="rn" bind:value={draft} onclick={(e) => e.stopPropagation()} onkeydown={(e) => { if (e.key === "Enter") rename(b); if (e.key === "Escape") renaming = null; }} onblur={() => rename(b)} />
        {:else}
          <span class="nm"><b>{b.name}</b><br /><span class="mut">niv. {b.level} · {Object.keys(b.slots).length} objets</span></span>
          <button class="mini" title="Renommer" onclick={(e) => { e.stopPropagation(); renaming = b.id; draft = b.name; }}>✎</button>
          <ConfirmButton mini label="✕" confirmLabel="Supprimer ?" title="Supprimer ce stuff" onconfirm={() => del(b)} />
        {/if}
      </div>
    {:else}
      <div class="mut empty">Collez un lien Dofusbook ci-dessus pour charger un stuff.</div>
    {/each}
  </SideList>
  <Grip {cols} name="left" dir={1} />

  <main class="main">
    {#if !build}
      <div class="mut pad">Aucun stuff sélectionné.</div>
    {:else}
      <div class="bar">
        <div class="ttl"><b>{build.name}</b> <span class="mut">niv. {build.level}</span>
          <button class="ghost" title="Recharger depuis Dofusbook" onclick={() => refresh(build!)}>↻</button>
          <a class="mut" href={build.url} target="_blank" rel="noreferrer">Dofusbook</a>
          <button class="scan" onclick={openScanList} disabled={!toScan.length} title="Ouvre une petite fenêtre flottante avec les objets à scanner à l'HDV">À scanner ({toScan.length})</button></div>
      </div>

      <SmoothScroll>
        <div class="sheetwrap"><div class="sheet">
          <section class="stats left">
            <div class="box">
              {#each PANELS.main as s (s.stat)}{@render statRow(s)}{/each}
            </div>
            <div class="box">
              <div class="st hd"><span class="lab"></span><span class="tgw"></span><span class="c4 mut">+⚡</span><span class="c4 mut">Base</span><span class="c4 mut">Parcho</span></div>
              <div class="st hp"><b class:bad={anyChosen && (real?.hp ?? 0) < (max?.hp ?? 0)} class:good={anyChosen && (real?.hp ?? 0) >= (max?.hp ?? 0)} title={anyChosen ? `Max théorique : ${max?.hp}` : ""}>{real?.hp}</b><span class="fm" title="Forgemagie">{fmOf(build, 11) ? `+${fmOf(build, 11)}` : ""}</span>{#if icon("stat", "pv")}<img src={icon("stat", "pv")} alt="" />{:else}<span class="ph"></span>{/if}<span class="lab">PdV</span>
                {#if anyChosen && real && max && real.hp !== max.hp}<span class="mut">/ {max.hp}</span>{/if}
                <button class="pf" title="Viser le maximum atteignable avec les objets ({max?.hp})" onclick={() => setTarget(HP_STAT, String(max?.hp ?? 0))}>Max</button>
                <input class="tg" inputmode="numeric" placeholder="min" value={build.targets[HP_STAT] ?? ""} onchange={(e) => setTarget(HP_STAT, e.currentTarget.value)} title="PdV minimum voulu pour la recherche de la combinaison la moins chère" />
                <span class="c4"></span><span class="c4">{build.carac.base_vi ?? 0}</span><span class="c4">{build.carac.scroll_vi ?? 0}</span>
              </div>
              {#each PANELS.carac as s (s.stat)}{@render statRow(s)}{/each}
            </div>
            <div class="box">{#each PANELS.secondary as s (s.stat)}{@render statRow(s)}{/each}</div>
          </section>

          <section class="center">
            <div class="build">
            <div class="doll">
              <div class="col">{#each SLOT_LAYOUT.left as s}{@render slot(s)}{/each}</div>
              <div class="char"><div class="lvl">Niveau {build.level}</div><div class="mut">{className(build.class)}</div></div>
              <div class="col">{#each SLOT_LAYOUT.right as s}{@render slot(s)}{/each}</div>
            </div>
            <div class="row">{#each SLOT_LAYOUT.bottom.filter((s) => build?.slots[s]) as s}{@render slot(s)}{/each}</div>
            <div class="legend mut"><span class="dot fresh"></span> scanné il y a moins d'1 h <span class="dot stale"></span> à scanner <span class="dot inv"></span> dans mon inventaire · ✕ = je possède déjà l'objet · clic = jet parfait ou mon exemplaire</div>

            </div>
          </section>

          <section class="stats right">
            <div class="box two dmg">
              <div>{#each PANELS.damage.slice(0, 6) as s (s.stat)}{@render statRow(s)}{/each}</div>
              <div>{#each PANELS.damage.slice(6) as s (s.stat)}{@render statRow(s)}{/each}</div>
            </div>
            <div class="box two res">
              <div>{#each PANELS.resist.filter((s) => !s.label.startsWith("%")) as s (s.stat)}{@render statRow(s)}{/each}</div>
              <div>{#each PANELS.resist.filter((s) => s.label.startsWith("%")) as s (s.stat)}{@render statRow(s)}{/each}</div>
            </div>
            <div class="box fmb">
              <div class="fh">Forgemagie</div>
              {#each fmRows as [code, v] (code)}
                <div class="st">
                  <span class="lab">{FM_STATS[code].label}</span>
                  <span class="fm">+</span><input class="tg" inputmode="numeric" value={v} onchange={(e) => setFm(code, e.currentTarget.value)} />
                  <button class="ghost" title="Retirer" onclick={() => setFm(code, "0")}>✕</button>
                </div>
              {:else}
                <div class="mut small">Aucune forgemagie dans ce stuff.</div>
              {/each}
              <select class="fmsel" bind:value={fmAdd} onchange={() => { if (fmAdd) { setFm(fmAdd, "1"); fmAdd = ""; } }}>
                <option value="">+ Ajouter une stat…</option>
                {#each fmFree as [code, f] (code)}<option value={code}>{f.label}</option>{/each}
              </select>
              <div class="mut small">Les bonus s'ajoutent aux stats (en bleu) ; modifiables.</div>
              {#if real && real.unmapped.size}
              <div class="mut small">Autres effets : {[...real.unmapped].map(([s, v]) => `${v} (#${s})`).join(", ")}</div>
            {/if}
            </div>
          </section>
        </div>
          <hr class="split" />
          <section class="bottom">
            <div class="box prices">
              <div class="fh">Prix</div>
              <div class="price" title={craft.missingSlots.length ? `Sans prix : ${names(craft.missingSlots)}` : ""}>
          <span class="mut">Craft</span> <b>{kamas(craft.total)}</b>
          {#if craft.missingSlots.length}<span class="bad">{craft.missingSlots.length} sans prix</span>{/if}
          {#if estSlots.length}<span class="mut" title="Objets non craftables au prix moyen : {names(estSlots)}">dont {estSlots.length} estimé{estSlots.length > 1 ? "s" : ""}</span>{/if}</div>
              <div class="price" title={first.missingSlots.length ? `Jamais scanné : ${names(first.missingSlots)}` : ""}>
          <span class="mut">Premier jet</span> <b>{kamas(first.total)}</b>
          {#if first.missingSlots.length}<span class="bad">{first.missingSlots.length} sans lot</span>{/if}</div>
              {#if anyChosen}
                <div class="price" title={picked.missingSlots.length ? `Pas de lot choisi : ${names(picked.missingSlots)}` : ""}>
            <span class="mut">Sélection</span> <b>{kamas(picked.total)}</b>
            {#if picked.missingSlots.length}<span class="mut">{picked.missingSlots.length} non choisi</span>{/if}</div>
        {/if}
              <label class="small"><input type="checkbox" checked={allDofusOwned} disabled={!dofusSlots.length} onchange={(e) => setDofusOwned(e.currentTarget.checked)} /> Sans les Dofus (hors trophées) dans le prix</label>
                          </div>
            <div class="box solver">
              <div class="fh">Combinaison la moins chère</div>
              <div class="mut small">Renseignez des minimums dans les cases « min » des stats, puis lancez la recherche parmi les lots scannés.</div>
              {#if solving}
                <div class="prog"><ProgressBar value={progress} height={6} speed={0.1} /></div>
                <div class="small mut">Calcul… {Math.round(progress * 100)} %</div>
                <button onclick={cancel}>Annuler</button>
              {:else}
                <button onclick={run} disabled={!Object.keys(build.targets).length || unscanned.length > 0}>Rechercher</button>
                {#if unscanned.length}<div class="small bad">À scanner d'abord (ou à marquer comme possédé avec ✕) : {names(unscanned)}.</div>{/if}
                {#if cancelled}<span class="small mut">Calcul annulé.</span>{/if}
              {/if}
              {#if result === "none"}<div class="bad small">Aucune combinaison ne satisfait ces minimums avec les lots scannés.</div>
              {:else if result}
                <div class="small">Coût : <b>{kamas(result.cost)}</b>{#if result.approx} <span class="mut">(recherche limitée, résultat approché)</span>{/if}</div>
                <div class="small mut">{Object.keys(result.picks).length} emplacement(s) choisis{#if result.fixedSlots.length} · gardés tels quels : {names(result.fixedSlots)}{/if}</div>
              {/if}
                                      {#if tableRows.length}
                <div class="fh tt">{result && result !== "none" ? "Combinaison trouvée" : "Lots choisis"}</div>
                {#snippet itemCell(r: (typeof tableRows)[number])}
                  {#if icon("item", r.picture)}<img class="lic" src={icon("item", r.picture)} alt="" />{/if}<span class="mut">{SLOT_TITLES[r.slot]}</span> <b>{r.name}</b>
                {/snippet}
                <LotTable
                  lots={tableRows.map((r) => ({ row: r.lot, key: r.slot, meta: r }))}
                  deals={false}
                  transcend={false}
                  lead={{ label: "Objet", cell: itemCell, sortKey: (r) => r.name }}
                  open={(k) => openLots.has(k)}
                  ontoggle={toggleLot}
                />
                <div class="tot">Total <b>{kamas(tableRows.reduce((s, r) => s + r.price, 0))}</b></div>
              {/if}
            </div>
          </section>
        </div>
      </SmoothScroll>
    {/if}
  </main>
</div>

<svelte:window onpointerdown={closePop} onkeydown={(e) => e.key === "Escape" && (pop = null)} onresize={() => (pop = null)} />
{#if pop && build?.slots[pop.slot]}
  {@const id = pop.slot}
  {@const sl = build.slots[id]}
  {@const mine = ownLot(id)}
  <div class="pop" style:left="{pop.left}px" style:top={pop.top != null ? `${pop.top}px` : null} style:bottom={pop.bottom != null ? `${pop.bottom}px` : null} style:max-height="{pop.maxH}px" style:width="{POP_W}px">
    <div class="phd"><b>{sl.name}</b> <span class="mut">{SLOT_TITLES[id]}</span></div>
    <button class="opt" class:on={owned.has(id) && !mine} onclick={() => { setOwned(id, "perfect"); pop = null; }}>
      Jet parfait <span class="mut">— je l'ai déjà, compté au maximum</span>
    </button>
    {#each inventory.ofGid(sl.gid) as it (it.uid)}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="opt copy" class:on={mine?.uid === it.uid} role="button" tabindex="0" onclick={() => { setOwned(id, it.uid); pop = null; }}>
        <div>Mon objet <span class="mut">— {it.equipped ? "équipé" : "dans le sac"}</span></div>
        <ItemCard row={it} flags={false} priced={false} />
      </div>
    {:else}
      <div class="mut small">{inventory.updated ? "Le personnage n'a pas cet objet (équipé ou dans le sac)." : "Inventaire inconnu : connectez votre personnage pendant que Karrelage tourne."}</div>
    {/each}
    <button class="opt" class:on={!owned.has(id)} onclick={() => { setOwned(id, null); pop = null; }}>
      À acheter <span class="mut">— je ne l'ai pas</span>
    </button>
  </div>
{/if}

<style>
  .grid { flex: 1; min-height: 0; display: grid; font-family: var(--font); }
  .main { min-height: 0; display: flex; flex-direction: column; padding: 8px 10px 0; min-width: 0; }
  .main > :global(.wrap) { flex: 1; }
  /* builds list (SideList, same look as the HDV item list) */
  .link { flex: 1; min-width: 0; padding: 3px 6px; }
  .irow.brow { display: flex; align-items: center; gap: 4px; }
  .err { padding: 0 10px 6px; }
  .err { font-size: 12px; margin-bottom: 6px; }
  .nm { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rn { flex: 1; min-width: 0; }
  /* icon buttons without a frame */
  .ghost { border: 0; background: none; padding: 0 4px; color: var(--mut); }
  .ghost:hover { color: var(--fg); }
  .pad { padding: 20px; }
  .bar { display: flex; flex-wrap: wrap; gap: 8px 18px; align-items: center; padding: 6px 10px; margin-bottom: 8px; background: var(--panel); border: 1px solid var(--line); border-radius: 6px; }
  .ttl { flex: 1; min-width: 220px; }
  .prog { margin: 6px 0 2px; line-height: 0; }
  .scan { margin-left: 10px; padding: 2px 8px; font-size: 12px; }
  .ttl a { margin-left: 8px; font-size: 12px; }
  .price { white-space: nowrap; }
  .price .bad { font-size: 11px; margin-left: 4px; }
  .sheetwrap { container-type: inline-size; }
  .sheet { display: grid; grid-template-columns: 460px max-content 620px; justify-content: safe center; gap: 14px; align-items: start; padding-bottom: 12px; }
  /* medium: stats + build on top, damage and resistances on one row, prices and forgemagie at the bottom */
  @container (max-width: 1719px) {
    .sheet { grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); grid-template-areas: "left build" "dmg res" "fmb fmb"; justify-content: stretch; }
    .stats.right, .center { display: contents; }
    .left { grid-area: left; }
    .build { grid-area: build; }
    .fmb { grid-area: fmb; }
    .dmg { grid-area: dmg; }
    .res { grid-area: res; }
  }
  @container (max-width: 1099px) {
    .two { grid-template-columns: minmax(0, 1fr); }
  }
  /* narrow: everything stacked */
  @container (max-width: 920px) {
    .sheet { grid-template-columns: minmax(0, 1fr); grid-template-areas: "left" "build" "dmg" "res" "fmb"; }
  }
  .stats { display: flex; flex-direction: column; gap: 8px; }
  .box { background: var(--panel); border: 1px solid var(--line); border-radius: 6px; padding: 6px 8px; }
  .st { display: flex; align-items: center; gap: 4px; padding: 2px 0; font-size: 12px; }
  .st img, .st .ph { width: 21px; height: 21px; flex: none; }
  .st .lab { flex: 1; color: var(--mut); }
  .st b { min-width: 36px; text-align: right; }
    .fm { color: #4aa3ff; font-size: 12px; min-width: 20px; text-align: left; flex: none; }
  .pf { height: 22px; padding: 0 6px; font: inherit; color: var(--mut); }
  .pf:hover { color: var(--fg); }
  .fmsel { width: 100%; margin-top: 4px; }
  .st .lab { white-space: nowrap; }
  .tgw { width: 44px; flex: none; }
  .c4 { min-width: 44px; text-align: right; flex: none; }
  .st.hd { font-size: 12px; }
  .tg { width: 44px; height: 22px; padding: 0 4px; font: inherit; text-align: right; }
  .two { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 0 12px; }
  .two > div { min-width: 0; }
  .split { border: 0; border-top: 1px solid var(--line); margin: 6px 0 12px; }
  .bottom { display: grid; grid-template-columns: minmax(260px, 320px) minmax(0, 1fr); gap: 14px; align-items: start; padding-bottom: 16px; }
  .lic { width: 22px; height: 22px; vertical-align: middle; margin-right: 6px; }
  .tot { margin-top: 8px; text-align: right; }
  .tt { margin-top: 12px; }
  @container (max-width: 920px) { .bottom { grid-template-columns: minmax(0, 1fr); } }
  .prices .price { margin: 2px 0; }
  .small { font-size: 12px; margin: 4px 0; }
  .center { display: flex; flex-direction: column; align-items: center; gap: 10px; }
  .build { display: flex; flex-direction: column; align-items: center; gap: 10px; min-width: 0; }
  .doll { display: flex; gap: 14px; align-items: stretch; width: 100%; justify-content: center; }
  .col { display: flex; flex-direction: column; gap: 8px; }
  .char { width: 190px; min-height: 300px; display: flex; flex-direction: column; align-items: center; justify-content: center; border: 1px dashed var(--line); border-radius: 10px; }
  .lvl { font: 600 16px var(--font); }
  .row { display: flex; gap: 8px; flex-wrap: wrap; justify-content: center; }
  .slot { position: relative; width: 73px; height: 73px; }
  .slot.empty { visibility: hidden; }
  .slot .ic { cursor: pointer; box-sizing: border-box; border: 2px solid var(--line); width: 100%; height: 100%; padding: 2px; display: flex; align-items: center; justify-content: center; border-width: 2px; border-radius: 8px; background: var(--panel); font-size: 10px; }
  .slot.fresh .ic { border-color: var(--ok); }
  .slot.stale .ic { border-color: var(--bad); }
  .slot.ign .ic { opacity: 0.55; }
  .slot.own .ic { border-color: var(--mut); }
  .slot.mine .ic { border-color: #4aa3ff; }
  .inv { position: absolute; right: 4px; top: 4px; width: 8px; height: 8px; border-radius: 50%; background: #4aa3ff; }
  .dot.inv { background: #4aa3ff; }
  .pop { position: fixed; z-index: 50; overflow-y: auto; box-sizing: border-box; padding: 8px; display: flex; flex-direction: column; gap: 6px; background: var(--panel); border: 1px solid var(--line); border-radius: 8px; box-shadow: 0 6px 24px rgba(0, 0, 0, 0.45); font-size: 12px; font-family: var(--font); }
  .phd { padding: 0 2px 2px; }
  .opt { text-align: left; padding: 5px 8px; font: inherit; border: 1px solid var(--line); border-radius: 6px; background: none; cursor: pointer; }
  .opt:hover { border-color: var(--mut); }
  .opt.on { border-color: #4aa3ff; }
  .copy { display: flex; flex-direction: column; gap: 4px; }
  .slot img { max-width: 100%; max-height: 100%; }
  .ow { position: absolute; left: -4px; bottom: -4px; width: 18px; height: 18px; padding: 0; border-radius: 9px; font-size: 10px; line-height: 1; color: var(--mut); }
  .ow.on { background: var(--mut); color: #000; border-color: var(--mut); }
  .legend { font-size: 11px; }
  .dot { display: inline-block; width: 9px; height: 9px; border-radius: 50%; margin: 0 3px 0 8px; }
  .dot.fresh { background: var(--ok); }
  .dot.stale { background: var(--bad); }
</style>
