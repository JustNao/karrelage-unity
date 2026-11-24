<script lang="ts">
  import { onMount } from "svelte";
  import { listenAll } from "../../lib/api";
  import { persisted } from "../../lib/persist.svelte";
  import type { CraftResult } from "../../lib/craft";
  import { batchFactor, meanRoll, outcome, type BrisEffect, type BrisItem, type BrisPack, type Outcome } from "../../lib/brisage";
  import { fmtNum, fmtTime, kamas, pct, runeShort, signedKamas } from "../../lib/fmt";
  import Segmented from "../../lib/components/Segmented.svelte";
  import ConfirmButton from "../../lib/components/ConfirmButton.svelte";

  /**
   * « Crafts »: the equipment pieces crafted or bought while the Brisage tab is open (`brisage:acquired`, heard only
   * while this panel is mounted), with their coefficient and the runes they should give: their own rolls, at the item's
   * known coefficient. Same items (gid) on one row with a count, or one row per piece. Its cost: the price paid for a
   * purchase, else the craft's estimated at today's ingredient prices (`craftCost`: each bought or crafted, the cheaper).
   * Planned crafts (a right click on an item of the list, `plan`) come first, greyed, at the mean roll; each craft of that
   * item seen takes one off; their runes count the coefficient falling at each piece (`decay`). An item crafted on the way
   * to a planned craft (an ingredient of it, `subCrafts`) is not listed. Once nothing is planned, rows are in craft order
   * (first craft first), else the latest first. « Ressources »: what they take, in the floating shopping list (`onshop`).
   */
  /** `qty`: identical pieces stacked (a batch craft), 1 when absent (older entries) */
  interface Acquired { t: number; gid: number; uid: number; qty?: number; how: "craft" | "achat"; price: number | null; effects: [number, number][] }
  /** A craft to come: the item and how many. */
  interface Planned { gid: number; qty: number; t: number }

  let { pack, coefOf, price, craft, jobOf, decay, subCrafts, selected, onpick, onshop }: {
    /** coefficient decay per piece broken (lib/brisage.ts `latestDecay`) */
    decay: number;
    /** the intermediate items the planned crafts take (crafted on the way: `Costs.shop`) */
    subCrafts: (planned: Map<number, number>) => Set<number>;
    /** the job crafting an item (null: no recipe) */
    jobOf: (gid: number) => string | null;
    pack: BrisPack;
    coefOf: (gid: number) => number | null;
    price: (gid: number) => number | null;
    /** the item's craft cost at the stored prices (null: no recipe) */
    craft: (gid: number) => Pick<CraftResult, "cost" | "missing"> | null;
    selected: number | null;
    onpick: (gid: number) => void;
    /** the planned crafts' resources in the shopping list: gid -> how many */
    onshop: (planned: Map<number, number>) => void;
  } = $props();

  /** Pieces kept, latest last. */
  const MAX = 300;
  const store = persisted("karrelage.brisage.recent", { list: [] as Acquired[], grouped: true, planned: [] as Planned[] });

  /** One more planned craft of this item (a right click in the Brisage list). */
  export function plan(gid: number) {
    const p = store.planned.find((x) => x.gid === gid);
    store.planned = p ? store.planned.map((x) => (x === p ? { ...x, qty: x.qty + 1, t: Date.now() / 1000 } : x)) : [...store.planned, { gid, qty: 1, t: Date.now() / 1000 }];
  }
  /** `n` planned crafts less (a right click on a planned row: one; a craft seen: its pieces). */
  function unplan(gid: number, n = 1) {
    store.planned = store.planned.map((x) => (x.gid === gid ? { ...x, qty: x.qty - n } : x)).filter((x) => x.qty > 0);
  }
  const plannedMap = $derived(new Map(store.planned.map((p) => [p.gid, p.qty])));
  const subSet = $derived(store.planned.length ? subCrafts(plannedMap) : new Set<number>());
  type Mode = "grouped" | "each";
  let mode = $state<Mode>(store.grouped ? "grouped" : "each");
  $effect(() => void (store.grouped = mode === "grouped"));

  onMount(() =>
    listenAll({
      "brisage:acquired": (a: Acquired) => {
        if (store.list.some((x) => x.uid === a.uid && x.how === a.how)) return;
        // an ingredient of a planned craft, made on the way (listed in « Ressources »)
        if (a.how === "craft" && !plannedMap.has(a.gid) && subSet.has(a.gid)) return;
        store.list = [...store.list, a].slice(-MAX);
        // a planned craft done
        if (a.how === "craft" && plannedMap.has(a.gid)) unplan(a.gid, a.qty ?? 1);
      },
    }),
  );

  const items = $derived(new Map(pack.items.map((i) => [i.gid, i])));

  /** `cost`: paid (purchase) or estimated (craft: `estimated`, `missing` = ingredients without a price) */
  interface Piece { a: Acquired; item: BrisItem | undefined; out: Outcome | null; coef: number | null; value: number | null; cost: number | null; estimated: boolean; missing: string[] }
  /** Each piece broken at its item's coefficient: its own rolls (an effect the item lacks by default = exo: no focus). */
  const pieces = $derived(
    store.list.map((a): Piece => {
      const item = items.get(a.gid);
      const coef = coefOf(a.gid);
      const c = a.how === "achat" && a.price != null ? null : craft(a.gid);
      const n = a.qty ?? 1;
      // a purchase's price is what the whole stack cost; a craft's estimate is per piece
      const paid = { cost: c ? (c.cost == null ? null : c.cost * n) : a.price, estimated: !!c, missing: c?.missing ?? [] };
      if (!item) return { a, item, out: null, coef, value: null, ...paid };
      const defaults = new Set(item.effects.map((e) => e.a));
      const fx: BrisEffect[] = a.effects.map(([e, v]) => ({ a: e, value: v, exo: !defaults.has(e) }));
      const out = outcome(fx, item.level, pack.runes, price);
      return { a, item, out, coef, value: coef == null ? null : (out.best.value * coef * n) / 100, ...paid };
    }),
  );

  interface Row { key: string; gid: number; name: string; count: number; coef: number | null; value: number | null; cost: number | null; estimated: boolean; how: Set<string>; t: number; pieces: Piece[] }
  const rows = $derived.by<Row[]>(() => {
    const one = (p: Piece, key: string): Row => ({
      key, gid: p.a.gid, name: p.item?.name ?? `#${p.a.gid}`, count: p.a.qty ?? 1, coef: p.coef, value: p.value, cost: p.cost, estimated: p.estimated, how: new Set([p.a.how]), t: p.a.t, pieces: [p],
    });
    // all planned crafts done: in craft order (first craft first), else the latest first
    const inOrder = store.planned.length === 0;
    if (mode === "each") {
      const l = pieces.map((p) => one(p, `${p.a.uid}:${p.a.t}`));
      return inOrder ? l : l.reverse();
    }
    const by = new Map<number, Row>();
    for (const p of pieces) {
      const r = by.get(p.a.gid);
      if (!r) {
        by.set(p.a.gid, one(p, String(p.a.gid)));
        continue;
      }
      r.count += p.a.qty ?? 1;
      r.value = r.value == null || p.value == null ? null : r.value + p.value;
      r.cost = r.cost == null || p.cost == null ? null : r.cost + p.cost;
      r.estimated ||= p.estimated;
      r.how.add(p.a.how);
      r.t = Math.max(r.t, p.a.t);
      r.pieces.push(p);
    }
    // grouped: a row's first craft gives its place in craft order, its latest the place among the latest
    const firstT = (r: Row) => Math.min(...r.pieces.map((p) => p.a.t));
    return [...by.values()].sort((a, b) => (inOrder ? firstT(a) - firstT(b) : b.t - a.t));
  });
  /** Planned crafts, at the item's mean roll: runes at the known coefficient, the craft estimated. Grouped by job (`first`:
   *  the group's first row, which shows the job). */
  const plannedRows = $derived.by(() => {
    const list = store.planned.map((p) => {
      const item = items.get(p.gid);
      const coef = coefOf(p.gid);
      const c = craft(p.gid);
      const out = item ? outcome(meanRoll(item), item.level, pack.runes, price) : null;
      const job = jobOf(p.gid) ?? "—";
      // the pieces broken one after the other: the coefficient falls at each
      const value = out && coef != null && item ? out.best.value * batchFactor(coef, item.level, p.qty, decay) : null;
      return { p, job, first: false, name: item?.name ?? `#${p.gid}`, coef, out, value, cost: c?.cost == null ? null : c.cost * p.qty, missing: c?.missing ?? [] };
    });
    list.sort((a, b) => a.job.localeCompare(b.job, "fr") || a.name.localeCompare(b.name, "fr"));
    list.forEach((r, i) => (r.first = i === 0 || list[i - 1].job !== r.job));
    return list;
  });
  const plannedCount = $derived(store.planned.reduce((s, p) => s + p.qty, 0));

  /** pieces, stacks counted whole */
  const count = $derived(store.list.reduce((s, a) => s + (a.qty ?? 1), 0));
  const total = $derived(rows.reduce((s, r) => s + (r.value ?? 0), 0));

  /** The session = since the app started (the page's load: the main window lives as long as the app). */
  const SESSION_START = performance.timeOrigin / 1000;
  /** The session's recap: pieces with both a cost and a rune value are summed, the others counted apart. */
  const session = $derived.by(() => {
    const mine = pieces.filter((p) => p.a.t >= SESSION_START);
    const known = mine.filter((p) => p.cost != null && p.value != null);
    const cost = known.reduce((s, p) => s + p.cost!, 0);
    const value = known.reduce((s, p) => s + p.value!, 0);
    const n = (l: Piece[]) => l.reduce((s, p) => s + (p.a.qty ?? 1), 0);
    return { count: n(mine), known: n(known), cost, value, gain: value - cost };
  });

  const coefText = (c: number | null) => (c == null ? "—" : `${fmtNum(c, c < 10 ? 1 : 0)} %`);
  const howText = (r: Row) => [...r.how].join(" + ");
  /** The runes of the row (without focus, at the coefficient), and the best result's focus. */
  function runesTitle(r: Row): string {
    if (r.coef == null) return "Coefficient inconnu : brisez-en un ou saisissez-le dans le détail de l'objet";
    const k = r.coef / 100;
    const counts = new Map<string, number>();
    for (const p of r.pieces) for (const l of p.out?.lines ?? []) counts.set(runeShort(l.rune.name) ?? "?", (counts.get(runeShort(l.rune.name) ?? "?") ?? 0) + l.count * k * (p.a.qty ?? 1));
    const plain = [...counts].map(([n, q]) => `${fmtNum(q, 1)} ${n}`).join(", ");
    const focus = r.pieces[0]?.out?.best.focus;
    const best = focus == null ? "sans focus" : `focus ${runeShort(r.pieces[0].out!.lines.find((l) => l.a === focus)?.rune.name)}`;
    return `Sans focus : ${plain}\nMeilleur résultat : ${best} (taxe de 2 % déduite)`;
  }
  function costTitle(r: Row): string {
    const bought = r.pieces.filter((p) => !p.estimated && p.cost != null).length;
    const crafted = r.pieces.filter((p) => p.estimated).length;
    const missing = [...new Set(r.pieces.flatMap((p) => p.missing))];
    const parts = [
      bought ? `${bought > 1 ? `${bought} achetés` : "Acheté"} : prix payé à l'hôtel de vente` : "",
      crafted ? `${crafted > 1 ? `${crafted} craftés` : "Crafté"} : coût du craft estimé aux prix enregistrés des ingrédients (achetés ou craftés, le moins cher)` : "",
      missing.length ? `Prix manquant : ${missing.join(", ")}` : "",
    ];
    return parts.filter(Boolean).join("\n") || "Ni prix d'achat ni recette connus";
  }
</script>

<div class="recent">
  <div class="head">
    <b>Crafts</b>
    {#if rows.length}<span class="mut">{count} objet{count > 1 ? "s" : ""}{" · "}{kamas(Math.round(total))}</span>{/if}
    {#if plannedCount}<span class="mut" title="Crafts prévus (clic droit sur un objet de la liste)">{plannedCount} prévu{plannedCount > 1 ? "s" : ""}</span>{/if}
    <span class="sp"></span>
    <Segmented
      bind:value={mode}
      label="Affichage des crafts récents"
      options={[
        { id: "grouped", label: "Regrouper", title: "Les mêmes objets sur une ligne, avec leur nombre" },
        { id: "each", label: "Par objet", title: "Une ligne par objet crafté ou acheté" },
      ]}
    />
    <button
      class="mini"
      disabled={!plannedCount}
      onclick={() => onshop(plannedMap)}
      title="Les ressources des crafts prévus, dans une fenêtre au-dessus du jeu (inventaire et banque déduits, cochées une fois achetées)"
    >Ressources</button>
    <ConfirmButton mini label="Vider" disabled={!store.list.length} onconfirm={() => void (store.list = [])} />
  </div>
  <div class="session" title="Objets craftés ou achetés depuis le lancement de l'application : coût (prix payé ou craft estimé), runes estimées au coefficient connu, bénéfice">
    <b>Session</b>
    {#if session.known}
      <span class="mut">{session.known} objet{session.known > 1 ? "s" : ""}</span>
      <span>Coût <b>{kamas(Math.round(session.cost))}</b></span>
      <span>Runes <b>{kamas(Math.round(session.value))}</b></span>
      <b class:good={session.gain > 0} class:bad={session.gain < 0}>{signedKamas(Math.round(session.gain))}{#if session.cost > 0}{" · "}{pct((session.gain / session.cost) * 100, 0)}{/if}</b>
    {:else}
      <span class="mut">{session.count ? "pas encore de coût ou de coefficient connu" : "rien crafté ni acheté depuis le lancement"}</span>
    {/if}
    {#if session.count > session.known && session.known}
      <span class="warn" title="Coût (prix d'un ingrédient) ou coefficient inconnu : non comptés">{session.count - session.known} sans estimation</span>
    {/if}
  </div>
  <div class="body">
    {#if rows.length || plannedRows.length}
      <table class="tbl">
        <thead>
          <tr>
            <th title="Métier qui crafte l'objet">Métier</th>
            <th>Objet</th>
            {#if mode === "grouped"}<th class="n">Qté</th>{:else}<th>Heure</th>{/if}
            <th class="n" title="Coefficient connu de l'objet (relevé au brisage ou saisi)">Coef</th>
            <th class="n" title="Valeur des runes au coefficient connu, avec les jets réels de chaque objet (meilleur résultat, taxe de 2 % déduite)">Runes estimées</th>
            <th class="n" title="Prix payé à l'hôtel de vente pour un achat, coût du craft estimé aux prix des ingrédients sinon">Prix</th>
          </tr>
        </thead>
        <tbody>
          {#each plannedRows as r (r.p.gid)}
            <tr
              class="planned"
              class:group={r.first}
              class:sel={r.p.gid === selected}
              onclick={() => onpick(r.p.gid)}
              oncontextmenu={(e) => (e.preventDefault(), unplan(r.p.gid))}
              title="Craft prévu, au jet moyen (clic droit : un de moins ; un craft de cet objet l'enlève)"
            >
              <td>{#if r.first}<b>{r.job}</b>{/if}</td>
              <td><b>{r.name}</b> <span class="mut">prévu</span></td>
              {#if mode === "grouped"}<td class="n">{r.p.qty}</td>{:else}<td class="mut">× {r.p.qty}</td>{/if}
              <td class="n">{coefText(r.coef)}</td>
              <td class="n">{#if r.value != null}<b>{kamas(Math.round(r.value))}</b>{#if r.cost != null}<span class="gain" class:good={r.value > r.cost} class:bad={r.value < r.cost}>({signedKamas(Math.round(r.value - r.cost))}{#if r.cost > 0}{" · "}{pct(((r.value - r.cost) / r.cost) * 100, 0)}{/if})</span>{/if}{:else}<span class="mut">—</span>{/if}</td>
              <td class="n" title={r.missing.length ? `Prix manquant : ${r.missing.join(", ")}` : "Coût du craft estimé aux prix enregistrés des ingrédients"}>{#if r.cost != null}{kamas(Math.round(r.cost))} <span class="mut">craft</span>{:else}—{#if r.missing.length} <span class="warn">⚠</span>{/if}{/if}</td>
            </tr>
          {/each}
          {#each rows as r (r.key)}
            <tr class:sel={r.gid === selected} onclick={() => onpick(r.gid)} title="Afficher l'objet">
              <td class="mut">{jobOf(r.gid) ?? "—"}</td>
              <td><b>{r.name}</b> <span class="mut">{howText(r)}</span></td>
              {#if mode === "grouped"}<td class="n">{r.count}</td>{:else}<td class="mut">{fmtTime(r.t)}</td>{/if}
              <td class="n">{coefText(r.coef)}</td>
              <td class="n" title={runesTitle(r)}>{#if r.value != null}<b>{kamas(Math.round(r.value))}</b>{#if r.pieces.some((p) => p.out?.unpriced)}<span class="warn" title="Une rune n'a pas de prix (comptée 0) : ouvrez-la à l'hôtel de vente">⚠</span>{/if}{#if r.cost != null}<span class="gain" class:good={r.value > r.cost} class:bad={r.value < r.cost}>({signedKamas(Math.round(r.value - r.cost))}{#if r.cost > 0}{" · "}{pct(((r.value - r.cost) / r.cost) * 100, 0)}{/if})</span>{/if}{:else}<span class="mut">—</span>{/if}</td>
              <td class="n" title={costTitle(r)}>{#if r.cost != null}{kamas(Math.round(r.cost))}{#if r.estimated} <span class="mut">craft</span>{/if}{:else}—{#if r.pieces.some((p) => p.missing.length)} <span class="warn">⚠</span>{/if}{/if}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <div class="mut empty">Les équipements craftés ou achetés pendant que cet onglet est ouvert s'affichent ici. Clic droit sur un objet de la liste : un craft prévu.</div>
    {/if}
  </div>
</div>

<style>
  .recent { flex: none; max-height: 40%; min-height: 90px; display: flex; flex-direction: column; border-top: 1px solid var(--line); padding-top: 6px; }
  .head { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; margin-bottom: 6px; }
  .sp { flex: 1; }
  .session { display: flex; align-items: baseline; gap: 12px; flex-wrap: wrap; padding: 5px 8px; margin-bottom: 6px; background: var(--bg); border: 1px solid var(--line); }
  .body { overflow: auto; min-height: 0; }
  .body :global(.tbl th) { position: sticky; top: 0; background: var(--panel); }
  tbody tr { cursor: pointer; }
  tr.planned td { opacity: 0.5; }
  tr.planned:hover td { opacity: 0.75; }
  tr.planned.group:not(:first-child) td { border-top: 1px solid var(--line); }
  tr.sel td { box-shadow: inset 0 1px 0 var(--acc), inset 0 -1px 0 var(--acc); }
  td { white-space: nowrap; }
  .empty { padding: 6px 0; }
  .gain { margin-left: 4px; }
  .warn { margin-left: 4px; }
</style>
