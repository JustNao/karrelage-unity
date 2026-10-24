<script lang="ts">
  import { onMount } from "svelte";
  import { call, cached, listenAll } from "../../lib/api";
  import { kamas } from "../../lib/fmt";
  import { auction } from "../../lib/stores/auction.svelte";
  import { prices } from "../../lib/stores/prices.svelte";
  import { inventory } from "../../lib/stores/inventory.svelte";
  import Grip from "../../lib/components/Grip.svelte";
  import { Columns } from "../../lib/columns.svelte";
  import { openScanList, type ScanItem } from "../../lib/scanlist";
  import { readJSON, writeJSON } from "../../lib/persist.svelte";
  import type { Recipes } from "../../lib/craft";
  import SmoothScroll from "../../lib/components/SmoothScroll.svelte";
  import {
    CARAC, FM_STATS, className, FRESH_SECS, HP_STAT, PANELS, SLOT_LAYOUT, SLOT_TITLES, compute, fmOf, itemStats, lotStats, slotPrices, sumSlots,
    type Build, type Info, type Lot, type Resolved, type StatRow,
  } from "../../lib/stuff";
  import BuildList from "./BuildList.svelte";
  import Modal from "../../lib/components/Modal.svelte";
  import { playerBuildLink } from "../../lib/dofusbookExport";
  import PricePanel, { type OutRow } from "./PricePanel.svelte";
  import SlotPopup, { POP_W, type PopAt } from "./SlotPopup.svelte";
  import { Solver } from "./solver.svelte";

  /**
   * Stuff tab: imported builds (BuildList) | the sheet (stats, slots, forgemagie), then prices and the cheapest-combination
   * search (PricePanel, run by `solver`). Slot popup: SlotPopup.
   */
  import { unitPriceOrNull } from "../../lib/price";
  import { iconOfItem, requestIcon } from "../../lib/stores/itemIcons.svelte";
  import { iconOfCode } from "../../lib/stores/statIcons.svelte";

  const cols = new Columns("karrelage.cols.stuff", { left: 230 });

  let builds = $state<Build[]>([]);
  let sel = $state<string | null>(null);
  let err = $state("");
  let info = $state.raw<Info>({ items: {}, sets: {} });
  let recipes = $state.raw<Recipes>({ recipes: {}, names: {} });
  const solver = new Solver();
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

  /** A player's build (inspected in game): the exact roll the player wears in that slot. */
  const rollOf = (slot: string): Lot | null => build?.rolls?.[slot] ?? null;

  const resolveWith = (real: boolean): Resolved[] =>
    slotIds.map((slot) => {
      const gid = build!.slots[slot].gid;
      // an item counted off the price is taken at the roll of the user's own copy, else at its perfect roll (max level for a familiar / mount);
      // a bought one at the chosen lot, else (player's build) at the player's roll
      const lot = real ? (ignored.has(slot) ? ownLot(slot) ?? null : chosenLot(slot) ?? rollOf(slot)) : null;
      return { slot, gid, lot, stats: lot ? lotStats(lot) : itemStats(info.items[gid] ?? ({ effects: [] } as any)) };
    });
  const real = $derived(build ? compute(build, info, resolveWith(true)) : null);
  const max = $derived(build ? compute(build, info, resolveWith(false)) : null);
  const anyChosen = $derived(!!build && slotIds.some((s) => chosenLot(s) || ownLot(s) || rollOf(s)));

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

  /** Items of the build not scanned in the last hour (red border), one entry per item. */
  const toScan = $derived([...new Map(slotIds.filter((s) => !ignored.has(s) && !fresh(s)).map((s) => [build!.slots[s].gid, { gid: build!.slots[s].gid, name: build!.slots[s].name, group: s === "fa" || s === "mo" ? "companion" : "equip" } as ScanItem])).values()]);
  const caracCode = (stat: number) => CARAC.find(([, st]) => st === stat)?.[0];
  /** Item icons (game iconId = Dofusbook picture) and stat icons come from the shared icon stores. */
  const icon = (kind: "item" | "stat", key: string | number) => (kind === "item" ? iconOfItem(+key) : iconOfCode(String(key)));
  const val = (c: typeof real, s: number) => c?.totals.get(s) ?? 0;

  /** Request token: only the latest build's data is applied (clicking A then B quickly must not end up with A's items). */
  let infoReq = 0;
  async function loadInfo(b: Build) {
    const req = ++infoReq;
    const gids = Object.values(b.slots).map((s) => s.gid);
    const got = await call<Info>("stuff", "item_info", { gids });
    if (req !== infoReq) return;
    info = got;
    for (const s of Object.values(b.slots)) requestIcon(s.picture);
  }

  async function select(id: string) {
    sel = id; solver.result = null;
    try { await loadInfo(builds.find((b) => b.id === id)!); } catch (e) { if (sel === id) err = String(e); }
  }

  const patch = (p: Partial<Pick<Build, "targets" | "chosen" | "ignored" | "owned" | "own_uid" | "fm">>) => {
    if (!build) return;
    Object.assign(build, p);
    call("stuff", "save_state", { id: build.id, patch: p }).catch((e) => (err = String(e)));
  };
  /** Dofusbook export link of a player's build, shown in its modal (null = closed). */
  let exported = $state<string | null>(null);
  let copied = $state<boolean | null>(false); // null = the clipboard refused
  async function copyExport() {
    try { await navigator.clipboard.writeText(exported!); copied = true; } catch { copied = null; }
  }
  const selectAll = (n: HTMLInputElement) => { n.focus(); n.select(); };
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
    patch({ owned: on ? [...rest, ...dofusSlots] : rest, own_uid: own }); solver.result = null;
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
    solver.result = null;
  };
  /**
   * "Utiliser mes objets": the slots whose item the character has. One copy -> owned at that copy, automatically;
   * several copies -> the user picks one (popup of the slot), listed in a warning until done.
   */
  const mineSlots = $derived(slotIds.filter((s) => inventory.ofGid(build!.slots[s].gid).length > 0));
  const copiesOf = (slot: string) => inventory.ofGid(build!.slots[slot].gid);
  const singleSlots = $derived(mineSlots.filter((s) => copiesOf(s).length === 1));
  const multiSlots = $derived(mineSlots.filter((s) => copiesOf(s).length > 1));
  /** A slot counts one of the character's copies. */
  const usesCopy = (slot: string) => owned.has(slot) && copiesOf(slot).some((c) => c.uid === build!.own_uid?.[slot]);
  const useMine = $derived(singleSlots.length > 0 && singleSlots.every(usesCopy));
  /** Several copies, none chosen yet: the user has to pick it. */
  const toPick = $derived(useMine ? multiSlots.filter((s) => !usesCopy(s)) : []);
  const setUseMine = (on: boolean) => {
    const own = { ...build!.own_uid };
    let o = [...(build!.owned ?? [])];
    for (const slot of singleSlots) {
      const uid = copiesOf(slot)[0].uid;
      if (on) {
        if (!o.includes(slot)) o.push(slot);
        own[slot] = uid;
      } else if (own[slot] === uid) {
        o = o.filter((x) => x !== slot); // only the slots this toggle set (same copy): the others stay as the user left them
        delete own[slot];
      }
    }
    patch({ owned: o, own_uid: own });
    solver.result = null;
  };
  /** Opens the popup of a slot to pick the copy (from the warning). */
  const pickSlot = (slot: string) => {
    const el = document.querySelector<HTMLElement>(`.slot[data-slot="${slot}"] .ic`);
    if (el) openPop(slot, el);
  };

  /** ✕: owned (the character's own copy when it has one, else the perfect roll) or bought again. */
  const toggleOwned = (slot: string) => setOwned(slot, owned.has(slot) ? null : firstCopy(slot) ?? "perfect");

  /** Popup of a slot (click on its item): perfect roll, one of the character's copies, or to buy. */
  let pop = $state<PopAt | null>(null);
  function openPop(slot: string, el: HTMLElement) {
    if (pop?.slot === slot) { pop = null; return; }
    const r = el.getBoundingClientRect();
    const left = Math.max(8, Math.min(r.left, window.innerWidth - POP_W - 8));
    const below = window.innerHeight - r.bottom - 12;
    pop = below >= 260 || below >= r.top ? { slot, left, top: r.bottom + 4, maxH: below } : { slot, left, bottom: window.innerHeight - r.top + 4, maxH: r.top - 12 };
  }
  /** PA / PM / PO minimums start at the Dofusbook value of the build (once per build, so clearing them sticks). */
  const AUTOFILL = [1, 23, 19];
  const AUTOFILL_KEY = "karrelage.stuff.autofilled";
  $effect(() => {
    if (!build || !max || !slotIds.every((s) => info.items[build!.slots[s].gid])) return; // data of the items not loaded yet
    const done = readJSON<string[]>(AUTOFILL_KEY, []);
    if (done.includes(build.id)) return;
    const targets = { ...build.targets };
    for (const s of AUTOFILL) if (targets[s] == null && (max.totals.get(s) ?? 0) > 0) targets[s] = max.totals.get(s)!;
    patch({ targets });
    writeJSON(AUTOFILL_KEY, [...done, build.id]);
  });
  const setTarget = (stat: number, v: string) => {
    const t = { ...build!.targets };
    const n = parseInt(v, 10);
    if (Number.isFinite(n) && n > 0) t[stat] = n; else delete t[stat];
    patch({ targets: t });
    solver.result = null;
  };

  const setFm = (code: string, v: string) => {
    const fm = { ...build!.fm };
    const n = parseInt(v, 10);
    if (Number.isFinite(n) && n !== 0) fm[code] = n; else delete fm[code];
    patch({ fm });
    solver.result = null;
  };
  let fmAdd = $state("");
  const fmRows = $derived(Object.entries(build?.fm ?? {}).filter(([c, v]) => v && FM_STATS[c]));
  const fmFree = $derived(Object.entries(FM_STATS).filter(([c]) => !build?.fm?.[c]));

  /** The search runs in a worker (Solver): the combination found is applied right away. */
  function run() {
    const lots = Object.fromEntries(slotIds.map((s) => [s, lotsOf(s).map((l) => ({ uid: l.uid, gid: l.gid, prices: l.prices, effects: l.effects }))]));
    const own = Object.fromEntries(slotIds.flatMap((s) => { const l = ownLot(s); return l ? [[s, { uid: l.uid, gid: l.gid, prices: [], effects: l.effects }]] : []; }));
    solver.run(
      { build: { ...$state.snapshot(build!), ignored: [...ignored] }, info, lots, own, targets: $state.snapshot(build!.targets) },
      (r) => patch({ chosen: { ...build!.chosen, ...r.picks } }),
      (msg) => (err = msg),
    );
  }

  const rowsOf = (map: Record<string, number | null | undefined>): OutRow[] =>
    slotIds.flatMap((s) => {
      if (ignored.has(s)) return []; // owned: own copy or perfect roll, not a lot of the output
      const lot = map[s] != null ? lotsOf(s).find((l) => l.uid === map[s]) : null;
      return lot ? [{ slot: s, name: build!.slots[s].name, picture: build!.slots[s].picture, price: unitPriceOrNull(lot)!, lot }] : [];
    });
  /** The lots of the combination found, else the lots chosen in the build. */
  const tableRows = $derived(build ? rowsOf(solver.result && solver.result !== "none" ? solver.result.picks : build.chosen) : []);

  onMount(() => {
    const tick = setInterval(() => (now = Date.now() / 1000), 15000);
    let alive = true;
    call<Build[]>("stuff", "list").then((l) => { if (!alive) return; builds = l; if (l[0]) select(l[0].id); });
    // a player inspected in game: added to "Builds des joueurs", or replacing that player's build
    const off = listenAll({
      "stuff:player": (b: Build) => {
        const had = builds.some((x) => x.id === b.id);
        builds = had ? builds.map((x) => (x.id === b.id ? b : x)) : [...builds, b];
        if (sel === b.id) void loadInfo(b);
      },
    });
    cached<Recipes>("auction", "recipes").then((r) => (recipes = r)).catch((e) => (err = String(e)));
    void auction.ensure();
    void prices.ensure();
    void inventory.ensure();
    return () => {
      alive = false;
      off();
      clearInterval(tick);
      infoReq++; // a pending item_info / icons answer is dropped
      solver.stop(); // the solver must not keep running (and patch the build) once the tab is gone
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
    <div class="slot" data-slot={id} class:fresh={!owned.has(id) && fresh(id)} class:stale={!owned.has(id) && !fresh(id)} class:own={owned.has(id)} class:mine={!!mine} class:ign={ignored.has(id)}>
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="ic" title="{sl.name} — {SLOT_TITLES[id]}{mine ? ' (mon objet)' : owned.has(id) ? ' (jet parfait)' : lot ? ` (lot à ${kamas(unitPriceOrNull(lot)!)})` : rollOf(id) ? ' (jet du joueur)' : ''}" onclick={(e) => openPop(id, e.currentTarget)}>
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
  <BuildList {cols} bind:builds bind:sel bind:err onselect={select} />
  <Grip {cols} name="left" dir={1} />

  <main class="main">
    {#if !build}
      <div class="mut pad">Aucun stuff sélectionné.</div>
    {:else}
      <div class="bar">
        <div class="ttl"><b>{build.name}</b> <span class="mut">niv. {build.level}</span>
          {#if build.player}
            <span class="mut" title="Équipement lu sur la fiche du joueur en jeu (jets exacts) ; une nouvelle inspection le remplace">{build.class ? `${className(build.class)} · ` : ""}{(build.player.level ?? 0) > 200 ? `niv. ${build.player.level} (Oméga) · ` : ""}inspecté le {new Date(build.player.seen * 1000).toLocaleString("fr-FR", { dateStyle: "short", timeStyle: "short" })}</span>
          {:else}
            <button class="ghost" title="Recharger depuis Dofusbook" onclick={() => refresh(build!)}>↻</button>
            <a class="mut" href={build.url} target="_blank" rel="noreferrer">Dofusbook</a>
          {/if}
          <button class="scan" onclick={() => openScanList(toScan)} disabled={!toScan.length} title="Ouvre une petite fenêtre flottante avec les objets à scanner à l'HDV">À scanner ({toScan.length})</button>
          {#if build.player}<button class="scan" onclick={() => ((exported = playerBuildLink(build!)), (copied = false))} title="Lien d'un stuff Dofusbook avec les objets de ce joueur (exos de ses jets en forgemagie)">Exporter vers Dofusbook</button>{/if}</div>
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
            {#if mineSlots.length}
              <div class="usemine">
                <label title="Compter les objets du stuff présents dans mon inventaire à leur jet réel, hors du prix"><input type="checkbox" checked={useMine} disabled={!singleSlots.length} onchange={(e) => setUseMine(e.currentTarget.checked)} /> Utiliser mes objets <span class="mut">({mineSlots.length} dans mon inventaire)</span></label>
                {#if toPick.length || (!singleSlots.length && multiSlots.some((s) => !usesCopy(s)))}
                  <div class="warn small">
                    ⚠ Plusieurs exemplaires dans mon inventaire, à choisir moi-même :
                    {#each (toPick.length ? toPick : multiSlots.filter((s) => !usesCopy(s))) as s, i (s)}{i ? ", " : " "}<button class="link" onclick={(ev) => (ev.stopPropagation(), pickSlot(s))}>{SLOT_TITLES[s]} ({copiesOf(s).length})</button>{/each}
                  </div>
                {/if}
              </div>
            {/if}
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
          <PricePanel
            {craft}
            {first}
            {picked}
            {estSlots}
            {anyChosen}
            {dofusSlots}
            {allDofusOwned}
            onDofusOwned={setDofusOwned}
            {solver}
            canSearch={Object.keys(build.targets).length > 0}
            {unscanned}
            onrun={run}
            rows={tableRows}
          />
        </div>
      </SmoothScroll>
    {/if}
  </main>
</div>

{#if exported}
  <Modal title="Exporter vers Dofusbook" width="520px" onclose={() => (exported = null)}>
    <div class="mut small">Lien à ouvrir dans le navigateur : Dofusbook y crée un stuff avec les objets du joueur (jets max, exos de ses jets en forgemagie ; ses caractéristiques ne sont pas sur sa fiche : points à 0, parchemins 100 ; niveau {build?.level}).</div>
    <input class="link" readonly value={exported} use:selectAll />
    {#if copied === null}<div class="bad small">Copie refusée : le lien est sélectionné, Ctrl+C pour le copier.</div>{/if}
    <div class="acts">
      <button class="mini" onclick={() => (exported = null)}>Fermer</button>
      <button class="mini" onclick={copyExport}>{copied ? "Copié ✓" : "Copier"}</button>
    </div>
  </Modal>
{/if}

{#if build}<SlotPopup bind:pop {build} {owned} mine={pop ? ownLot(pop.slot) : undefined} onset={setOwned} />{/if}

<style>
  .grid { flex: 1; min-height: 0; display: grid; font-family: var(--font); }
  .main { min-height: 0; display: flex; flex-direction: column; padding: 8px 10px 0; min-width: 0; }
  .main > :global(.wrap) { flex: 1; }
  /* icon buttons without a frame */
  .ghost { border: 0; background: none; padding: 0 4px; color: var(--mut); }
  .ghost:hover { color: var(--fg); }
  .pad { padding: 20px; }
  .bar { display: flex; flex-wrap: wrap; gap: 8px 18px; align-items: center; padding: 6px 10px; margin-bottom: 8px; background: var(--panel); border: 1px solid var(--line); border-radius: 6px; }
  .ttl { flex: 1; min-width: 220px; }
  .scan { margin-left: 10px; padding: 2px 8px; font-size: 12px; }
  .link { width: 100%; box-sizing: border-box; padding: 5px 8px; }
  .acts { display: flex; justify-content: flex-end; gap: 6px; }
  .small { font-size: 12px; }
  .ttl a { margin-left: 8px; font-size: 12px; }
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
  .small { font-size: 12px; margin: 4px 0; }
  .usemine { margin: 8px 0 2px; font-size: 13px; }
  .usemine .link { border: 0; background: none; padding: 0; color: var(--acc); text-decoration: underline; font: inherit; }
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
  .slot img { max-width: 100%; max-height: 100%; }
  .ow { position: absolute; left: -4px; bottom: -4px; width: 18px; height: 18px; padding: 0; border-radius: 9px; font-size: 10px; line-height: 1; color: var(--mut); }
  .ow.on { background: var(--mut); color: #000; border-color: var(--mut); }
  .legend { font-size: 11px; }
  .dot { display: inline-block; width: 9px; height: 9px; border-radius: 50%; margin: 0 3px 0 8px; }
  .dot.fresh { background: var(--ok); }
  .dot.stale { background: var(--bad); }
</style>
