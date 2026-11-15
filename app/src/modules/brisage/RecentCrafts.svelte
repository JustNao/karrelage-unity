<script lang="ts">
  import { onMount } from "svelte";
  import { listenAll } from "../../lib/api";
  import { persisted } from "../../lib/persist.svelte";
  import { outcome, type BrisEffect, type BrisItem, type BrisPack, type Outcome } from "../../lib/brisage";
  import { fmtNum, fmtTime, kamas, runeShort, signedKamas } from "../../lib/fmt";
  import Segmented from "../../lib/components/Segmented.svelte";
  import ConfirmButton from "../../lib/components/ConfirmButton.svelte";

  /**
   * « Crafts récents »: the equipment pieces crafted or bought while the Brisage tab is open (`brisage:acquired`, heard only
   * while this panel is mounted), with their coefficient and the runes they should give: their own rolls, at the item's
   * known coefficient. Same items (gid) on one row with a count, or one row per piece.
   */
  interface Acquired { t: number; gid: number; uid: number; how: "craft" | "achat"; price: number | null; effects: [number, number][] }

  let { pack, coefOf, price, selected, onpick }: { pack: BrisPack; coefOf: (gid: number) => number | null; price: (gid: number) => number | null; selected: number | null; onpick: (gid: number) => void } = $props();

  /** Pieces kept, latest last. */
  const MAX = 300;
  const store = persisted("karrelage.brisage.recent", { list: [] as Acquired[], grouped: true });
  type Mode = "grouped" | "each";
  let mode = $state<Mode>(store.grouped ? "grouped" : "each");
  $effect(() => void (store.grouped = mode === "grouped"));

  onMount(() =>
    listenAll({
      "brisage:acquired": (a: Acquired) => {
        if (store.list.some((x) => x.uid === a.uid && x.how === a.how)) return;
        store.list = [...store.list, a].slice(-MAX);
      },
    }),
  );

  const items = $derived(new Map(pack.items.map((i) => [i.gid, i])));

  interface Piece { a: Acquired; item: BrisItem | undefined; out: Outcome | null; coef: number | null; value: number | null }
  /** Each piece broken at its item's coefficient: its own rolls (an effect the item lacks by default = exo: no focus). */
  const pieces = $derived(
    store.list.map((a): Piece => {
      const item = items.get(a.gid);
      const coef = coefOf(a.gid);
      if (!item) return { a, item, out: null, coef, value: null };
      const defaults = new Set(item.effects.map((e) => e.a));
      const fx: BrisEffect[] = a.effects.map(([e, v]) => ({ a: e, value: v, exo: !defaults.has(e) }));
      const out = outcome(fx, item.level, pack.runes, price);
      return { a, item, out, coef, value: coef == null ? null : (out.best.value * coef) / 100 };
    }),
  );

  interface Row { key: string; gid: number; name: string; count: number; coef: number | null; value: number | null; cost: number | null; how: Set<string>; t: number; pieces: Piece[] }
  const rows = $derived.by<Row[]>(() => {
    const one = (p: Piece, key: string): Row => ({
      key, gid: p.a.gid, name: p.item?.name ?? `#${p.a.gid}`, count: 1, coef: p.coef, value: p.value, cost: p.a.price, how: new Set([p.a.how]), t: p.a.t, pieces: [p],
    });
    if (mode === "each") return pieces.map((p) => one(p, `${p.a.uid}:${p.a.t}`)).reverse();
    const by = new Map<number, Row>();
    for (const p of pieces) {
      const r = by.get(p.a.gid);
      if (!r) {
        by.set(p.a.gid, one(p, String(p.a.gid)));
        continue;
      }
      r.count++;
      r.value = r.value == null || p.value == null ? null : r.value + p.value;
      r.cost = r.cost == null || p.a.price == null ? null : r.cost + p.a.price;
      r.how.add(p.a.how);
      r.t = Math.max(r.t, p.a.t);
      r.pieces.push(p);
    }
    return [...by.values()].sort((a, b) => b.t - a.t);
  });
  const total = $derived(rows.reduce((s, r) => s + (r.value ?? 0), 0));

  const coefText = (c: number | null) => (c == null ? "—" : `${fmtNum(c, c < 10 ? 1 : 0)} %`);
  const howText = (r: Row) => [...r.how].join(" + ");
  /** The runes of the row (without focus, at the coefficient), and the best result's focus. */
  function runesTitle(r: Row): string {
    if (r.coef == null) return "Coefficient inconnu : brisez-en un ou saisissez-le dans le détail de l'objet";
    const k = r.coef / 100;
    const counts = new Map<string, number>();
    for (const p of r.pieces) for (const l of p.out?.lines ?? []) counts.set(runeShort(l.rune.name) ?? "?", (counts.get(runeShort(l.rune.name) ?? "?") ?? 0) + l.count * k);
    const plain = [...counts].map(([n, q]) => `${fmtNum(q, 1)} ${n}`).join(", ");
    const focus = r.pieces[0]?.out?.best.focus;
    const best = focus == null ? "sans focus" : `focus ${runeShort(r.pieces[0].out!.lines.find((l) => l.a === focus)?.rune.name)}`;
    return `Sans focus : ${plain}\nMeilleur résultat : ${best} (taxe de 2 % déduite)`;
  }
</script>

<div class="recent">
  <div class="head">
    <b>Crafts récents</b>
    {#if rows.length}<span class="mut">{store.list.length} objet{store.list.length > 1 ? "s" : ""} · {kamas(Math.round(total))}</span>{/if}
    <span class="sp"></span>
    <Segmented
      bind:value={mode}
      label="Affichage des crafts récents"
      options={[
        { id: "grouped", label: "Regrouper", title: "Les mêmes objets sur une ligne, avec leur nombre" },
        { id: "each", label: "Par objet", title: "Une ligne par objet crafté ou acheté" },
      ]}
    />
    <ConfirmButton mini label="Vider" disabled={!store.list.length} onconfirm={() => void (store.list = [])} />
  </div>
  <div class="body">
    {#if rows.length}
      <table class="tbl">
        <thead>
          <tr>
            <th>Objet</th>
            {#if mode === "grouped"}<th class="n">Qté</th>{:else}<th>Heure</th>{/if}
            <th class="n" title="Coefficient connu de l'objet (relevé au brisage ou saisi)">Coef</th>
            <th class="n" title="Valeur des runes au coefficient connu, avec les jets réels de chaque objet (meilleur résultat, taxe de 2 % déduite)">Runes estimées</th>
            <th class="n" title="Prix payé à l'hôtel de vente (achats)">Prix</th>
          </tr>
        </thead>
        <tbody>
          {#each rows as r (r.key)}
            <tr class:sel={r.gid === selected} onclick={() => onpick(r.gid)} title="Afficher l'objet">
              <td><b>{r.name}</b> <span class="mut">{howText(r)}</span></td>
              {#if mode === "grouped"}<td class="n">{r.count}</td>{:else}<td class="mut">{fmtTime(r.t)}</td>{/if}
              <td class="n">{coefText(r.coef)}</td>
              <td class="n" title={runesTitle(r)}>{#if r.value != null}<b>{kamas(Math.round(r.value))}</b>{#if r.pieces.some((p) => p.out?.unpriced)}<span class="warn" title="Une rune n'a pas de prix (comptée 0) : ouvrez-la à l'hôtel de vente">⚠</span>{/if}{#if r.cost != null}<span class="gain" class:good={r.value > r.cost} class:bad={r.value < r.cost}>({signedKamas(Math.round(r.value - r.cost))})</span>{/if}{:else}<span class="mut">—</span>{/if}</td>
              <td class="n">{r.cost != null ? kamas(r.cost) : "—"}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <div class="mut empty">Les équipements craftés ou achetés pendant que cet onglet est ouvert s'affichent ici.</div>
    {/if}
  </div>
</div>

<style>
  .recent { flex: none; max-height: 40%; min-height: 90px; display: flex; flex-direction: column; border-top: 1px solid var(--line); padding-top: 6px; }
  .head { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; margin-bottom: 6px; }
  .sp { flex: 1; }
  .body { overflow: auto; min-height: 0; }
  .body :global(.tbl th) { position: sticky; top: 0; background: var(--panel); }
  tbody tr { cursor: pointer; }
  tr.sel td { box-shadow: inset 0 1px 0 var(--acc), inset 0 -1px 0 var(--acc); }
  td { white-space: nowrap; }
  .empty { padding: 6px 0; }
  .gain { margin-left: 4px; }
  .warn { margin-left: 4px; }
</style>
