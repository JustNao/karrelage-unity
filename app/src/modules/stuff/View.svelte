<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";
  import { call, kamas, on } from "../../lib/api";
  import Grip from "../../lib/components/Grip.svelte";
  import PanelHead from "../../lib/components/PanelHead.svelte";
  import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { Columns } from "../../lib/columns.svelte";
  import { SCAN_LIST_KEY, type ScanItem } from "../../lib/scanlist";
  import type { Recipes } from "../../lib/craft";
  import ItemCard from "../../lib/components/ItemCard.svelte";
  import SmoothScroll from "../../lib/components/SmoothScroll.svelte";
  import type { StoredPrice } from "../../lib/transcend";
  import {
    CARAC, FM_STATS, FRESH_SECS, PANELS, SLOT_LAYOUT, SLOT_TITLES, STAT_ICON_CODES, STAT_LABEL, compute, fmOf, itemStats, lotStats, lotUnit, slotPrices, solve, sumSlots,
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
  let auction = new SvelteMap<number, any>();
  let prices = new SvelteMap<number, StoredPrice>();
  let picker = $state<string | null>(null);
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
    return gid != null ? ((auction.get(gid)?.rows ?? []) as Lot[]) : [];
  };
  const fresh = (slot: string) => {
    const gid = build?.slots[slot]?.gid;
    const t = gid != null ? auction.get(gid)?.updated : null;
    return t != null && now - t <= FRESH_SECS;
  };
  const chosenLot = (slot: string) => lotsOf(slot).find((l) => l.uid === build?.chosen[slot]) ?? null;

  const resolveWith = (real: boolean): Resolved[] =>
    slotIds.map((slot) => {
      const gid = build!.slots[slot].gid;
      const lot = real ? chosenLot(slot) : null;
      return { slot, gid, lot, stats: lot ? lotStats(lot) : itemStats(info.items[gid] ?? ({ effects: [] } as any)) };
    });
  const real = $derived(build ? compute(build, info, resolveWith(true)) : null);
  const max = $derived(build ? compute(build, info, resolveWith(false)) : null);
  const anyChosen = $derived(!!build && slotIds.some((s) => chosenLot(s)));

  const DOFUS = ["d1", "d2", "d3", "d4", "d5", "d6"];
  let noDofus = $state((() => { try { return localStorage.getItem("karrelage.stuff.noDofus") === "1"; } catch { return false; } })());
  $effect(() => { try { localStorage.setItem("karrelage.stuff.noDofus", noDofus ? "1" : "0"); } catch {} });
  /** Slots left out of the prices: the ones marked with "€", plus the Dofus when the toggle is on (they still count in stats). */
  const DOFUS_TYPE = 23; // itemtypes: Dofus (trophées 151 and prysmaradites share the same slots and stay priced)
  const dofusSlots = $derived(DOFUS.filter((s) => info.items[build?.slots[s]?.gid ?? -1]?.type_id === DOFUS_TYPE));
  const ignored = $derived(new Set([...(build?.ignored ?? []), ...(noDofus ? dofusSlots : [])]));
  const perSlot = $derived(
    Object.fromEntries(slotIds.map((s) => [s, slotPrices(build!.slots[s].gid, lotsOf(s), build!.chosen[s], recipes, prices)])),
  );
  const craft = $derived(sumSlots(slotIds, ignored, (s) => perSlot[s].craft));
  const estSlots = $derived(slotIds.filter((s) => !ignored.has(s) && perSlot[s].estimated));
  const first = $derived(sumSlots(slotIds, ignored, (s) => perSlot[s].first));
  const picked = $derived(sumSlots(slotIds, ignored, (s) => perSlot[s].chosen));
  const names = (l: string[]) => l.map((s) => SLOT_TITLES[s]).join(", ");

  /** Items of the build not scanned in the last 30 minutes (red border), one entry per item. */
  const toScan = $derived([...new Map(slotIds.filter((s) => !fresh(s)).map((s) => [build!.slots[s].gid, { gid: build!.slots[s].gid, name: build!.slots[s].name, group: s === "fa" || s === "mo" ? "companion" : "equip" } as ScanItem])).values()]);
  async function openScanList() {
    try { localStorage.setItem(SCAN_LIST_KEY, JSON.stringify(toScan)); } catch {}
    const open = await WebviewWindow.getByLabel("scanlist");
    if (open) { await open.setFocus(); return; } // it picks the new list up through the storage event
    new WebviewWindow("scanlist", { url: "scanlist.html", title: "À scanner", width: 280, height: 380, decorations: false, transparent: true, shadow: false, alwaysOnTop: true, skipTaskbar: true, resizable: false });
  }
  const caracCode = (stat: number) => CARAC.find(([, st]) => st === stat)?.[0];
  const icon = (kind: "item" | "stat", key: string | number) => icons.get(`${kind}:${key}`);
  const val = (c: typeof real, s: number) => c?.totals.get(s) ?? 0;

  async function loadInfo(b: Build) {
    const gids = Object.values(b.slots).map((s) => s.gid);
    info = await call<Info>("stuff", "item_info", { gids });
    const got = await call<Record<string, string>>("stuff", "icons", { item: Object.values(b.slots).map((s) => s.picture), stat: STAT_ICON_CODES });
    for (const [k, v] of Object.entries(got)) icons.set(k, v);
  }

  async function select(id: string) {
    sel = id; picker = null; result = null;
    try { await loadInfo(builds.find((b) => b.id === id)!); } catch (e) { err = String(e); }
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

  const patch = (p: Partial<Pick<Build, "targets" | "chosen" | "ignored" | "fm">>) => {
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
    if (!confirm(`Supprimer « ${b.name} » ?`)) return;
    await call("stuff", "delete", { id: b.id });
    builds = builds.filter((x) => x.id !== b.id);
    if (sel === b.id) sel = builds[0]?.id ?? null;
  };
  const refresh = async (b: Build) => {
    try {
      const fresh = await call<Build>("stuff", "refresh", { id: b.id });
      builds = builds.map((x) => (x.id === b.id ? fresh : x));
      await loadInfo(fresh);
    } catch (e) { err = String(e); }
  };

  const pick = (slot: string, uid: number | null) => { patch({ chosen: { ...build!.chosen, [slot]: uid } }); picker = null; result = null; };
  const toggleIgnore = (slot: string) => patch({ ignored: ignored.has(slot) ? build!.ignored.filter((s) => s !== slot) : [...build!.ignored, slot] });
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
      else if ("done" in e.data) { result = e.data.done ?? "none"; stop(); }
    };
    worker.onerror = (e) => { err = e.message || "erreur du calcul"; stop(); };
    worker.postMessage(JSON.parse(JSON.stringify({ build: { ...build!, ignored: [...ignored] }, info, lots, targets: build!.targets })));
  }
  function stop() { worker?.terminate(); worker = null; solving = false; }
  function cancel() { cancelled = true; stop(); }
  const apply = () => {
    if (!result || result === "none") return;
    patch({ chosen: { ...build!.chosen, ...result.picks } });
    result = null;
  };

  const lotLine = (l: Lot) =>
    l.effects
      .filter((e: any) => e.status !== "removed" && e.group && e.group !== "weapon" && e.group !== "other")
      .slice(0, 6)
      .map((e: any) => `${e.hi ?? ""} ${STAT_LABEL.get(e.stat) ?? e.text}`)
      .join(" · ");
  /** Lots whose effects are unfolded in the list under the build (like the HDV: folded = only exo / above-max effects). */
  let openLots = new SvelteSet<string>();
  const toggleLot = (slot: string) => (openLots.has(slot) ? openLots.delete(slot) : openLots.add(slot));
  const rowsOf = (map: Record<string, number | null | undefined>) =>
    slotIds.flatMap((s) => {
      const lot = map[s] != null ? lotsOf(s).find((l) => l.uid === map[s]) : null;
      return lot ? [{ slot: s, name: build!.slots[s].name, price: lotUnit(lot)!, lot }] : [];
    });
  /** The lots of the combination found, else the lots chosen in the build. */
  const tableRows = $derived(build ? rowsOf(result && result !== "none" ? result.picks : build.chosen) : []);
  const cheapest = (slot: string) => [...lotsOf(slot)].filter((l) => lotUnit(l) != null).sort((a, b) => lotUnit(a)! - lotUnit(b)!).slice(0, 40);

  onMount(() => {
    const tick = setInterval(() => (now = Date.now() / 1000), 15000);
    call<Build[]>("stuff", "list").then((l) => { builds = l; if (l[0]) select(l[0].id); });
    call<Recipes>("auction", "recipes").then((r) => (recipes = r));
    call<any[]>("auction", "items").then((l) => l.forEach((e) => auction.set(e.gid, e)));
    call<StoredPrice[]>("prices", "list").then((l) => l.forEach((p) => prices.set(p.gid, p)));
    const offs = [
      on("auction:item", (e: any) => auction.set(e.gid, e)),
      on("auction:cleared", () => auction.clear()),
      on<StoredPrice>("prices:price", (p) => prices.set(p.gid, p)),
      on<StoredPrice[]>("prices:bulk", (l) => l.forEach((p) => prices.set(p.gid, p))),
      on("prices:cleared", () => prices.clear()),
    ];
    return () => { clearInterval(tick); offs.forEach((o) => o.then((f) => f())); };
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
    <div class="slot" class:fresh={fresh(id)} class:stale={!fresh(id)} class:ign={ignored.has(id)}>
      <button class="ic" title="{sl.name} — {SLOT_TITLES[id]}{lot ? ` (lot à ${kamas(lotUnit(lot)!)})` : ''}" onclick={() => (picker = picker === id ? null : id)}>
        {#if icon("item", sl.picture)}<img src={icon("item", sl.picture)} alt={sl.name} />{:else}<span class="mut">{SLOT_TITLES[id]}</span>{/if}
      </button>
      <button class="ig" class:on={ignored.has(id)} title={ignored.has(id) ? "Remettre dans le prix" : "Ignorer dans le prix (compte toujours dans les stats)"} onclick={() => toggleIgnore(id)}>€</button>
      {#if lot}<span class="tag">lot</span>{/if}
    </div>
  {:else}
    <div class="slot empty"></div>
  {/if}
{/snippet}

<div class="grid cgrid" class:anim={!cols.dragging} style:grid-template-columns="{cols.px("left")}px 5px 1fr">
  <aside class="side" class:cz={cols.collapsed.left}>
    <PanelHead {cols} name="left" title="Stuffs" />
    <div class="sbar">
      <input type="search" placeholder="Lien Dofusbook…" bind:value={link} onkeydown={(e) => e.key === "Enter" && add()} />
      <button class="bmini" onclick={add} disabled={busy}>{busy ? "…" : "Ajouter"}</button>
    </div>
    {#if err}<div class="bad err">{err}</div>{/if}
    {#each builds as b, i (b.id)}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="irow" class:first={i === 0} class:sel={b.id === sel} onclick={() => renaming !== b.id && select(b.id)}>
        {#if renaming === b.id}
          <input class="rn" bind:value={draft} onclick={(e) => e.stopPropagation()} onkeydown={(e) => { if (e.key === "Enter") rename(b); if (e.key === "Escape") renaming = null; }} onblur={() => rename(b)} />
        {:else}
          <span class="nm"><b>{b.name}</b><br /><span class="mut">niv. {b.level} · {Object.keys(b.slots).length} objets</span></span>
          <button class="bmini" title="Renommer" onclick={(e) => { e.stopPropagation(); renaming = b.id; draft = b.name; }}>✎</button>
          <button class="bmini" title="Supprimer" onclick={(e) => { e.stopPropagation(); del(b); }}>✕</button>
        {/if}
      </div>
    {:else}
      <div class="mut empty">Collez un lien Dofusbook ci-dessus pour charger un stuff.</div>
    {/each}
  </aside>
  <Grip {cols} name="left" dir={1} />

  <main class="main">
    {#if !build}
      <div class="mut pad">Aucun stuff sélectionné.</div>
    {:else}
      <div class="bar">
        <div class="ttl"><b>{build.name}</b> <span class="mut">niv. {build.level}</span>
          <button class="mini" title="Recharger depuis Dofusbook" onclick={() => refresh(build!)}>↻</button>
          <a class="mut" href={build.url} target="_blank" rel="noreferrer">Dofusbook</a>
          <button class="scan" onclick={openScanList} disabled={!toScan.length} title="Ouvre une petite fenêtre flottante avec les objets à scanner à l'HDV">À scanner ({toScan.length})</button></div>
      </div>

      <SmoothScroll>
        <div class="sheetwrap"><div class="sheet">
          <section class="stats left">
            <div class="box">
              <div class="st hp"><b>{real?.hp}</b><span class="fm"></span>{#if icon("stat", "pv")}<img src={icon("stat", "pv")} alt="" />{:else}<span class="ph"></span>{/if}<span class="lab">PdV</span></div>
              {#each PANELS.main as s (s.stat)}{@render statRow(s)}{/each}
            </div>
            <div class="box">
              <div class="st hd"><span class="lab"></span><span class="tgw"></span><span class="c4 mut">+⚡</span><span class="c4 mut">Base</span><span class="c4 mut">Parcho</span></div>
              {#each PANELS.carac as s (s.stat)}{@render statRow(s)}{/each}
            </div>
            <div class="box">{#each PANELS.secondary as s (s.stat)}{@render statRow(s)}{/each}</div>
          </section>

          <section class="center">
            <div class="build">
            <div class="doll">
              <div class="col">{#each SLOT_LAYOUT.left as s}{@render slot(s)}{/each}</div>
              <div class="char"><div class="lvl">Niveau {build.level}</div><div class="mut">classe {build.class}</div></div>
              <div class="col">{#each SLOT_LAYOUT.right as s}{@render slot(s)}{/each}</div>
            </div>
            <div class="row">{#each SLOT_LAYOUT.bottom.filter((s) => build?.slots[s]) as s}{@render slot(s)}{/each}</div>
            <div class="legend mut"><span class="dot fresh"></span> scanné il y a moins de 30 min <span class="dot stale"></span> à scanner · € = ignoré dans le prix</div>

            {#if picker}
              <div class="picker">
                <div class="ph2"><b>{SLOT_TITLES[picker]}</b> — {build.slots[picker].name} <button class="mini" onclick={() => (picker = null)}>✕</button></div>
                {#if !lotsOf(picker).length}
                  <div class="mut">Aucun lot scanné : ouvrez l'objet à l'hôtel de vente.</div>
                {:else}
                  <button class="lotb" class:on={build.chosen[picker] == null} onclick={() => pick(picker!, null)}>Max théorique (aucun lot choisi)</button>
                  {#each cheapest(picker) as l (l.uid)}
                    <button class="lotb" class:on={build.chosen[picker] === l.uid} onclick={() => pick(picker!, l.uid)}><b>{kamas(lotUnit(l)!)}</b> <span class="mut">{lotLine(l)}</span></button>
                  {/each}
                {/if}
              </div>
            {/if}
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
                  <button class="mini" title="Retirer" onclick={() => setFm(code, "0")}>✕</button>
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
              <label class="small"><input type="checkbox" bind:checked={noDofus} /> Sans les Dofus (hors trophées) dans le prix</label>
            </div>
            <div class="box solver">
              <div class="fh">Combinaison la moins chère</div>
              <div class="mut small">Renseignez des minimums dans les cases « min » des stats, puis lancez la recherche parmi les lots scannés. La forgemagie du stuff est cherchée sous forme d'exo sur les objets.</div>
              {#if solving}
                <div class="prog" role="progressbar" aria-valuenow={Math.round(progress * 100)} aria-valuemin="0" aria-valuemax="100"><div class="fill" style:width="{Math.round(progress * 100)}%"></div></div>
                <div class="small mut">Calcul… {Math.round(progress * 100)} %</div>
                <button onclick={cancel}>Annuler</button>
              {:else}
                <button onclick={run} disabled={!Object.keys(build.targets).length}>Rechercher</button>
                {#if cancelled}<span class="small mut">Calcul annulé.</span>{/if}
              {/if}
              {#if result === "none"}<div class="bad small">Aucune combinaison ne satisfait ces minimums avec les lots scannés{#if fmRows.length} (la forgemagie du stuff doit être portée par des objets : exo {fmRows.map(([c, v]) => `+${v} ${FM_STATS[c].label}`).join(", ")}){/if}.</div>
              {:else if result}
                <div class="small">Coût : <b>{kamas(result.cost)}</b>{#if result.approx} <span class="mut">(recherche limitée, résultat approché)</span>{/if}</div>
                <div class="small mut">{Object.keys(result.picks).length} emplacement(s) choisis{#if result.fixedSlots.length} · gardés tels quels : {names(result.fixedSlots)}{/if}</div>
                <button onclick={apply}>Appliquer</button>
              {/if}
                                      {#if tableRows.length}
                <div class="fh tt">{result && result !== "none" ? "Combinaison trouvée" : "Lots choisis"}</div>
                <div class="lotlist">
                  {#each tableRows as r (r.slot)}
                    <div class="lrow">
                      <div class="lh"><span class="mut">{SLOT_TITLES[r.slot]}</span> <b>{r.name}</b></div>
                      <ItemCard row={r.lot} flags={false} collapsible open={openLots.has(r.slot)} ontoggle={() => toggleLot(r.slot)} />
                    </div>
                  {/each}
                </div>
                <div class="tot">Total <b>{kamas(tableRows.reduce((s, r) => s + r.price, 0))}</b></div>
              {/if}
            </div>
          </section>
        </div>
      </SmoothScroll>
    {/if}
  </main>
</div>

<style>
  .grid { flex: 1; min-height: 0; display: grid; font-family: var(--font); }
  .main { min-height: 0; display: flex; flex-direction: column; padding: 8px 10px 0; min-width: 0; }
  .main > :global(.wrap) { flex: 1; }
  /* builds list: same look as the HDV item list */
  .side { overflow: auto; min-height: 0; min-width: 0; background: var(--panel); border-right: 1px solid var(--line); font-family: var(--font); }
  .side .sbar { display: flex; gap: 6px; padding: 6px 10px; position: sticky; top: 30px; z-index: 3; background: var(--panel); }
  .side .sbar input { flex: 1; min-width: 0; padding: 3px 6px; }
  .side .irow { display: flex; align-items: center; gap: 4px; padding: 6px 10px; border-bottom: 1px solid var(--line); cursor: pointer; }
  .side .irow:hover { background: var(--bg); }
  .side .irow.sel { outline: 1px solid var(--acc); outline-offset: -1px; position: relative; z-index: 2; }
  .side .irow.first { border-top: 1px solid var(--line); }
  .side .empty { padding: 6px 10px; }
  .side .err { padding: 0 10px 6px; }
  .bmini { padding: 2px 7px; font-size: 11px; }
  .bmini:disabled { opacity: 0.35; cursor: default; }
  .err { font-size: 12px; margin-bottom: 6px; }
  .nm { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rn { flex: 1; min-width: 0; }
  .mini { border: 0; background: none; padding: 0 4px; color: var(--mut); }
  .mini:hover { color: var(--fg); }
  .pad { padding: 20px; }
  .bar { display: flex; flex-wrap: wrap; gap: 8px 18px; align-items: center; padding: 6px 10px; margin-bottom: 8px; background: var(--panel); border: 1px solid var(--line); border-radius: 6px; }
  .ttl { flex: 1; min-width: 220px; }
  .prog { height: 6px; border-radius: 3px; background: var(--line); overflow: hidden; margin: 6px 0 2px; }
  .prog .fill { height: 100%; background: var(--acc); transition: width 0.1s linear; }
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
  .lotlist { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 8px; margin-top: 6px; }
  .lh { font-size: 12px; margin-bottom: 2px; }
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
  .slot .ic { width: 100%; height: 100%; padding: 2px; display: flex; align-items: center; justify-content: center; border-width: 2px; border-radius: 8px; background: var(--panel); font-size: 10px; }
  .slot.fresh .ic { border-color: var(--ok); }
  .slot.stale .ic { border-color: var(--bad); }
  .slot.ign .ic { opacity: 0.55; }
  .slot img { max-width: 100%; max-height: 100%; }
  .ig { position: absolute; right: -4px; bottom: -4px; width: 18px; height: 18px; padding: 0; border-radius: 9px; font-size: 10px; line-height: 1; color: var(--mut); }
  .ig.on { background: var(--warn); color: #000; border-color: var(--warn); }
  .tag { position: absolute; left: 2px; top: 2px; font-size: 9px; background: var(--acc); color: #000; border-radius: 3px; padding: 0 3px; }
  .legend { font-size: 11px; }
  .dot { display: inline-block; width: 9px; height: 9px; border-radius: 50%; margin: 0 3px 0 8px; }
  .dot.fresh { background: var(--ok); }
  .dot.stale { background: var(--bad); }
  .picker { width: 100%; max-height: 300px; overflow: auto; border: 1px solid var(--line); border-radius: 6px; padding: 6px; display: flex; flex-direction: column; gap: 3px; background: var(--panel); }
  .lotb { text-align: left; font-size: 12px; padding: 3px 6px; }
  .lotb.on { border-color: var(--acc); }
  .good { color: var(--ok); }
  .bad { color: var(--bad); }
</style>
