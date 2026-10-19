<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteMap } from "svelte/reactivity";
  import { call, kamas, on } from "../../lib/api";
  import { Columns } from "../../lib/columns.svelte";
  import type { Recipes } from "../../lib/craft";
  import SmoothScroll from "../../lib/components/SmoothScroll.svelte";
  import type { StoredPrice } from "../../lib/transcend";
  import {
    FM_STATS, FRESH_SECS, PANELS, SLOT_LAYOUT, SLOT_TITLES, STAT_ICON_CODES, STAT_LABEL, compute, fmOf, itemStats, lotStats, lotUnit, slotPrices, solve, sumSlots,
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

  const ignored = $derived(new Set(build?.ignored ?? []));
  const perSlot = $derived(
    Object.fromEntries(slotIds.map((s) => [s, slotPrices(build!.slots[s].gid, lotsOf(s), build!.chosen[s], recipes, prices)])),
  );
  const craft = $derived(sumSlots(slotIds, ignored, (s) => perSlot[s].craft));
  const first = $derived(sumSlots(slotIds, ignored, (s) => perSlot[s].first));
  const picked = $derived(sumSlots(slotIds, ignored, (s) => perSlot[s].chosen));
  const names = (l: string[]) => l.map((s) => SLOT_TITLES[s]).join(", ");

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

  function run() {
    solving = true; result = null;
    setTimeout(() => {
      try { result = solve(build!, info, lotsOf, build!.targets) ?? "none"; } catch (e) { err = String(e); }
      solving = false;
    }, 20);
  }
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
    {#if s.icon && icon("stat", s.icon)}<img src={icon("stat", s.icon)} alt="" />{:else}<span class="ph"></span>{/if}
    <span class="lab">{s.label}</span>
    <b class:bad={anyChosen && v < m} class:good={anyChosen && v >= m} title={anyChosen ? `Max théorique : ${m}` : ""}>{v}</b>
    {#if fm}<span class="fm" title="Forgemagie">+{fm}</span>{/if}
    {#if anyChosen && v !== m}<span class="mut">/ {m}</span>{/if}
    <button class="pf" title="Viser le maximum atteignable avec les objets ({m})" onclick={() => setTarget(s.stat, String(m))}>jet perf</button>
    <input class="tg" inputmode="numeric" placeholder="min" value={build?.targets[s.stat] ?? ""} onchange={(e) => setTarget(s.stat, e.currentTarget.value)} title="Valeur minimale voulue pour la recherche de la combinaison la moins chère" />
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

<div class="grid" style:grid-template-columns="{cols.w.left}px 5px 1fr">
  <aside class="side">
    <div class="fh">Stuffs</div>
    <div class="add">
      <input placeholder="Lien Dofusbook…" bind:value={link} onkeydown={(e) => e.key === "Enter" && add()} />
      <button onclick={add} disabled={busy}>{busy ? "…" : "Ajouter"}</button>
    </div>
    {#if err}<div class="bad err">{err}</div>{/if}
    <SmoothScroll>
      {#each builds as b (b.id)}
        <div class="bi" class:on={b.id === sel}>
          {#if renaming === b.id}
            <input class="rn" bind:value={draft} onkeydown={(e) => { if (e.key === "Enter") rename(b); if (e.key === "Escape") renaming = null; }} onblur={() => rename(b)} />
          {:else}
            <button class="nm" onclick={() => select(b.id)}>{b.name}</button>
            <button class="mini" title="Renommer" onclick={() => { renaming = b.id; draft = b.name; }}>✎</button>
            <button class="mini" title="Supprimer" onclick={() => del(b)}>✕</button>
          {/if}
        </div>
      {:else}
        <div class="mut">Collez un lien Dofusbook ci-dessus pour charger un stuff.</div>
      {/each}
    </SmoothScroll>
  </aside>
  <div class="grip" role="separator" onpointerdown={(e) => cols.drag("left", 1, e)}></div>

  <main class="main">
    {#if !build}
      <div class="mut pad">Aucun stuff sélectionné.</div>
    {:else}
      <div class="bar">
        <div class="ttl"><b>{build.name}</b> <span class="mut">niv. {build.level}</span>
          <button class="mini" title="Recharger depuis Dofusbook" onclick={() => refresh(build!)}>↻</button>
          <a class="mut" href={build.url} target="_blank" rel="noreferrer">Dofusbook</a></div>
        <div class="price" title={craft.missingSlots.length ? `Sans prix : ${names(craft.missingSlots)}` : ""}>
          <span class="mut">Craft</span> <b>{kamas(craft.total)}</b>
          {#if craft.missingSlots.length}<span class="bad">{craft.missingSlots.length} sans prix</span>{/if}</div>
        <div class="price" title={first.missingSlots.length ? `Jamais scanné : ${names(first.missingSlots)}` : ""}>
          <span class="mut">Premier jet</span> <b>{kamas(first.total)}</b>
          {#if first.missingSlots.length}<span class="bad">{first.missingSlots.length} sans lot</span>{/if}</div>
        {#if anyChosen}
          <div class="price" title={picked.missingSlots.length ? `Pas de lot choisi : ${names(picked.missingSlots)}` : ""}>
            <span class="mut">Sélection</span> <b>{kamas(picked.total)}</b>
            {#if picked.missingSlots.length}<span class="mut">{picked.missingSlots.length} non choisi</span>{/if}</div>
        {/if}
      </div>

      <SmoothScroll>
        <div class="sheet">
          <section class="stats">
            <div class="box">
              <div class="st hp"><span class="lab">PdV</span><b>{real?.hp}</b></div>
              {#each PANELS.main as s (s.stat)}{@render statRow(s)}{/each}
            </div>
            <div class="box">{#each PANELS.carac as s (s.stat)}{@render statRow(s)}{/each}</div>
            <div class="box">{#each PANELS.secondary as s (s.stat)}{@render statRow(s)}{/each}</div>
          </section>

          <section class="center">
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
          </section>

          <section class="stats">
            <div class="box">{#each PANELS.damage as s (s.stat)}{@render statRow(s)}{/each}</div>
            <div class="box">{#each PANELS.resist as s (s.stat)}{@render statRow(s)}{/each}</div>
            {#if real && real.unmapped.size}
              <div class="box mut small">Autres effets : {[...real.unmapped].map(([s, v]) => `${v} (#${s})`).join(", ")}</div>
            {/if}
            <div class="box">
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
              <select bind:value={fmAdd} onchange={() => { if (fmAdd) { setFm(fmAdd, "1"); fmAdd = ""; } }}>
                <option value="">+ Ajouter une stat…</option>
                {#each fmFree as [code, f] (code)}<option value={code}>{f.label}</option>{/each}
              </select>
              <div class="mut small">Les bonus s'ajoutent aux stats (en bleu) ; modifiables.</div>
            </div>
            <div class="box">
              <div class="fh">Combinaison la moins chère</div>
              <div class="mut small">Renseignez des minimums dans les cases « min » des stats, puis lancez la recherche parmi les lots scannés.</div>
              <button onclick={run} disabled={solving || !Object.keys(build.targets).length}>{solving ? "Calcul…" : "Rechercher"}</button>
              {#if result === "none"}<div class="bad small">Aucune combinaison ne satisfait ces minimums avec les lots scannés.</div>
              {:else if result}
                <div class="small">Coût : <b>{kamas(result.cost)}</b>{#if result.approx} <span class="mut">(recherche limitée, résultat approché)</span>{/if}</div>
                <div class="small mut">{Object.keys(result.picks).length} emplacement(s) choisis{#if result.fixedSlots.length} · gardés tels quels : {names(result.fixedSlots)}{/if}</div>
                <button onclick={apply}>Appliquer</button>
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
  .side, .main { min-height: 0; display: flex; flex-direction: column; padding: 8px 10px 0; min-width: 0; }
  .side > :global(.wrap), .main > :global(.wrap) { flex: 1; }
  .add { display: flex; gap: 6px; margin-bottom: 8px; }
  .add input { flex: 1; min-width: 0; }
  .err { font-size: 12px; margin-bottom: 6px; }
  .bi { display: flex; align-items: center; gap: 2px; padding: 3px 4px; border: 1px solid transparent; border-radius: 6px; }
  .bi.on { background: var(--panel); border-color: var(--line); }
  .nm { flex: 1; text-align: left; border: 0; background: none; padding: 2px 4px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rn { flex: 1; min-width: 0; }
  .mini { border: 0; background: none; padding: 0 4px; color: var(--mut); }
  .mini:hover { color: var(--fg); }
  .pad { padding: 20px; }
  .bar { display: flex; flex-wrap: wrap; gap: 8px 18px; align-items: center; padding: 6px 10px; margin-bottom: 8px; background: var(--panel); border: 1px solid var(--line); border-radius: 6px; }
  .ttl { flex: 1; min-width: 220px; }
  .ttl a { margin-left: 8px; font-size: 12px; }
  .price { white-space: nowrap; }
  .price .bad { font-size: 11px; margin-left: 4px; }
  .sheet { display: grid; grid-template-columns: 300px max-content 300px; justify-content: center; gap: 14px; align-items: start; padding-bottom: 12px; }
  .stats { display: flex; flex-direction: column; gap: 8px; }
  .box { background: var(--panel); border: 1px solid var(--line); border-radius: 6px; padding: 6px 8px; }
  .st { display: flex; align-items: center; gap: 6px; padding: 2px 0; font-size: 12px; }
  .st img, .st .ph { width: 16px; height: 16px; flex: none; }
  .st .lab { flex: 1; color: var(--mut); }
  .st b { min-width: 34px; text-align: right; }
  .st.hp b { font-size: 14px; }
  .fm { color: #4aa3ff; font-size: 11px; }
  .pf { padding: 0 4px; font-size: 10px; color: var(--mut); line-height: 16px; }
  .pf:hover { color: var(--fg); }
  .tg { width: 44px; padding: 1px 4px; font-size: 11px; text-align: right; }
  .small { font-size: 12px; margin: 4px 0; }
  .center { display: flex; flex-direction: column; align-items: center; gap: 10px; }
  .doll { display: flex; gap: 14px; align-items: stretch; width: 100%; justify-content: center; }
  .col { display: flex; flex-direction: column; gap: 8px; }
  .char { width: 190px; min-height: 300px; display: flex; flex-direction: column; align-items: center; justify-content: center; border: 1px dashed var(--line); border-radius: 10px; }
  .lvl { font: 600 16px var(--font); }
  .row { display: flex; gap: 8px; flex-wrap: wrap; justify-content: center; }
  .slot { position: relative; width: 56px; height: 56px; }
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
