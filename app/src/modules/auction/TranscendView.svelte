<script lang="ts">
  import { ago, kamasFull, runeShort } from "../../lib/fmt";
  import { readJSON, writeJSON } from "../../lib/persist.svelte";
  import { auction } from "../../lib/stores/auction.svelte";
  import { useSort } from "../../lib/sort.svelte";
  import { targetKey, type Target, type transcendance } from "../../lib/transcend";
  import LotTable from "../../lib/components/LotTable.svelte";
  import SortTable from "../../lib/components/SortTable.svelte";

  /**
   * HDV > Transcendance: the transcended lots seen (rune targets; a click limits the candidates to that rune, `focus`), the
   * profitable candidates (unfolding one marks it and the lots with the same rune on the graph, `cand`) and the candidates
   * saved for later (a snapshot kept in localStorage across scans).
   */
  let {
    trans,
    item,
    keyOf,
    effLabel,
    open,
    ontoggle,
    focus = $bindable(),
    cand = $bindable(),
    fade,
    onfade,
    onselect,
  }: {
    trans: ReturnType<typeof transcendance>;
    item: any;
    keyOf: (row: any) => string;
    effLabel: (a: number) => string;
    open: (key: string) => boolean;
    ontoggle: (key: string) => void;
    focus: string | null;
    cand: { id: string | number; a: number; value: number } | null;
    fade: number | null;
    onfade: (a: number) => void;
    /** open another item (a saved candidate's) */
    onselect: (gid: number) => void;
  } = $props();

  // ---- saved candidates: a snapshot kept across scans (the lot may be sold by the time you come back)
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
    const l = readJSON<Saved[]>(SAVED_KEY, []).map((s) => ({ ...s, row: lotSummary(s.row) })); // older entries kept the full row
    if (l.length) writeJSON(SAVED_KEY, l);
    return l;
  };
  let saved = $state.raw<Saved[]>(loadSaved());
  const persistSaved = (l: Saved[]) => {
    saved = l;
    writeJSON(SAVED_KEY, l);
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
    const it = auction.items.get(sv.gid);
    return it && sv.row.uid != null && it.updated > sv.savedAt ? it.rows.some((r: any) => r.uid === sv.row.uid) : null;
  };

  /** Unfolding a candidate also puts it (and the lots with the same rune) forward on the graph; folding it clears that. */
  function toggleCand(key: string, o: any) {
    ontoggle(key);
    const cid = keyOf(o.row);
    const t = o.best!.target;
    cand = open(key) ? { id: cid, a: t.a, value: t.value } : cand?.id === cid ? null : cand;
  }
  type TargetKey = "effect" | "lots" | "sale" | "rune" | "cost";
  const targetSort = useSort<TargetKey>(null, false, (k) => k === "lots");
  const TARGET_COLUMNS: { key: TargetKey; label: string; num?: boolean; title?: string }[] = [
    { key: "effect", label: "Effet ajouté" },
    { key: "lots", label: "Lots", num: true },
    { key: "sale", label: "À partir de", num: true, title: "Lot transcendé le moins cher avec cet effet" },
    { key: "rune", label: "Rune" },
    { key: "cost", label: "Prix de la rune", num: true },
  ];
  const targetValue = (t: Target, k: TargetKey) => (k === "effect" ? effLabel(t.a) : k === "lots" ? t.lots : k === "sale" ? t.sale : k === "rune" ? (t.rune?.name ?? null) : t.runeCost);
</script>

<div class="trans">
  <!-- saved candidates: a snapshot kept across scans -->
  {#snippet svRemove(sv: Saved)}<button class="mini" title="Retirer ce candidat" onclick={(ev) => (ev.stopPropagation(), persistSaved(saved.filter((x) => x.key !== sv.key)))}>✕</button>{/snippet}
  {#snippet svProfit(sv: Saved)}<b class="ok">+{kamasFull(sv.profit)}</b>{/snippet}
  {#snippet svInvest(sv: Saved)}<span title="Prix du lot + prix de la rune">{kamasFull(sv.buy + sv.runeCost)}</span>{/snippet}
  {#snippet svItem(sv: Saved)}<button class="link" onclick={(ev) => (ev.stopPropagation(), onselect(sv.gid))}>{sv.itemName}</button>{/snippet}
  {#snippet svRune(sv: Saved)}+{sv.value} {sv.label} <span class="mut">{runeShort(sv.rune)}</span>{/snippet}
  {#snippet svState(sv: Saved)}
    {@const live = stillListed(sv)}
    {#if live === true}<span class="ok">en vente</span>{:else if live === false}<span class="warn">absent du dernier scan</span>{:else}<span class="mut">pas rescanné</span>{/if}
    <span class="mut"> · {ago(sv.savedAt)}</span>
  {/snippet}
  {#if saved.length}
    <div class="ghead">Candidats enregistrés</div>
    <LotTable
      lots={saved.map((sv) => ({ row: sv.row, key: "s" + sv.key, meta: sv }))}
      deals={false}
      transcend={false}
      alignPrice={false}
      leads={[
        { key: "rm", label: "", cell: svRemove },
        { key: "profit", label: "Bénéfice", cell: svProfit, sort: (sv) => sv.profit, num: true, desc: true },
        { key: "invest", label: "Investissement", cell: svInvest, sort: (sv) => sv.buy + sv.runeCost, num: true, title: "Prix du lot + prix de la rune" },
        { key: "item", label: "Objet", cell: svItem, sort: (sv) => sv.itemName },
        { key: "rune", label: "Rune", cell: svRune, sort: (sv) => sv.label },
        { key: "state", label: "Statut", cell: svState, sort: (sv) => sv.savedAt, desc: true },
      ]}
      {open}
      {ontoggle}
    />
  {/if}
  {#if trans.targets.length === 0}
    <div class="mut">Aucun lot transcendé de cet objet dans la liste : rien à comparer.</div>
  {:else}
    <div class="ghead">Lots transcendés vus <span class="hint">· cliquer sur une ligne pour ne voir que les candidats de cette rune</span></div>
    <div class="ttable">
      <SortTable
        columns={TARGET_COLUMNS}
        rows={targetSort.apply(trans.targets, targetValue)}
        sort={targetSort}
        key={(t) => targetKey(t)}
        rowProps={(t) => ({ class: focus === targetKey(t) ? "picked" : "", onclick: () => (focus = focus === targetKey(t) ? null : targetKey(t)) })}
      >
        {#snippet row(t)}
          <td><b>+{t.value} {effLabel(t.a)}</b></td>
          <td class="n">{t.lots}</td>
          <td class="n">{kamasFull(t.sale)}</td>
          <td>{#if t.rune}{t.rune.name}{:else}<span class="warn">aucune rune correspondante</span>{/if}</td>
          <td class="n">
            {#if t.rune && t.runeCost == null}<span class="warn" title="Ouvrez la rune à l'hôtel de vente">prix inconnu</span>
            {:else if t.runeCost != null}{kamasFull(t.runeCost)}
              {#if t.avgOnly}<span class="warn" title="Prix moyen sur 7 jours seulement : ouvrez la rune à l'hôtel de vente pour relever son prix"> ⚠</span>{:else if t.stale}<span class="warn" title="Prix de la rune vieux de plus d'un jour : ouvrez-la à l'hôtel de vente pour l'actualiser"> ⚠</span>{/if}
            {/if}
          </td>
        {/snippet}
      </SortTable>
    </div>
    {#if trans.unpricedCount}<div class="mut note">{trans.unpricedCount} lots candidats ne peuvent pas être classés tant que leur rune n'a pas de prix enregistré.</div>{/if}
    <div class="ghead">Candidats rentables{#if focus} <button class="mini" onclick={() => (focus = null)}>Toutes les runes ✕</button>{/if}</div>
    {#snippet oSave(o: any)}<button class="mini star" class:on={isSaved(o)} title={isSaved(o) ? "Enregistré : cliquer pour l'oublier" : "Garder ce candidat pour y revenir plus tard"} onclick={(ev) => (ev.stopPropagation(), toggleSaved(o))}>{isSaved(o) ? "★" : "☆"}</button>{/snippet}
    {#snippet oProfit(o: any)}<b class="ok">+{kamasFull(o.best.profit)}</b>{/snippet}
    {#snippet oInvest(o: any)}<span title="Prix du lot + prix de la rune">{kamasFull(o.buy + (o.best.target.runeCost ?? 0))}</span>{/snippet}
    {#snippet oRune(o: any)}
      {@const t = o.best.target}
      +{t.value} {effLabel(t.a)} <span class="mut">{runeShort(t.rune?.name) ?? ""}</span>
      {#if t.avgOnly}<span class="warn" title="Prix moyen sur 7 jours, jamais relevé à l'hôtel de vente"> ⚠</span>{:else if t.stale}<span class="warn" title="Prix de la rune vieux de plus d'un jour"> ⚠</span>{/if}
    {/snippet}
    {#snippet oSale(o: any)}~{kamasFull(o.best.target.sale)}{/snippet}
    {#snippet oDensity(o: any)}<span class="mut">{o.z >= 0 ? "+" : ""}{o.z.toFixed(1)}σ</span>{/snippet}
    {#if trans.opportunities.length}
      <LotTable
        lots={trans.opportunities.map((o) => ({ row: o.row, key: "t" + keyOf(o.row), meta: o }))}
        deals={false}
        transcend={false}
        alignPrice={false}
        picked={cand ? "t" + cand.id : null}
        leads={[
          { key: "save", label: "", cell: oSave },
          { key: "profit", label: "Bénéfice", cell: oProfit, sort: (o) => o.best.profit, num: true, desc: true },
          { key: "invest", label: "Investissement", cell: oInvest, sort: (o) => o.buy + (o.best.target.runeCost ?? 0), num: true, title: "Prix du lot + prix de la rune" },
          { key: "rune", label: "Rune à ajouter", cell: oRune, sort: (o) => effLabel(o.best.target.a) },
          { key: "sale", label: "Revente", cell: oSale, sort: (o) => o.best.target.sale, num: true, title: "Lot transcendé le moins cher avec le même effet (avant taxe)" },
          { key: "z", label: "Densité", cell: oDensity, sort: (o) => o.z, num: true, desc: true, title: "Écart du lot à la tendance prix / densité (> 0 = plus de stats que son prix ne le laisse penser)" },
        ]}
        {open}
        ontoggle={(k) => toggleCand(k, trans.opportunities.find((o) => "t" + keyOf(o.row) === k))}
        {fade}
        {onfade}
      />
    {:else}
      <div class="mut">Aucun candidat rentable pour le moment.</div>
    {/if}
  {/if}
</div>

<style>
  .ghead { margin: 12px 0 2px; font-size: 10px; letter-spacing: 0.08em; text-transform: uppercase; color: var(--mut); }
  .ghead .hint { text-transform: none; letter-spacing: 0; }
  .trans .note { margin: 4px 0 8px; font-size: 12px; }
  .ttable :global(tbody tr) { cursor: pointer; }
  .ttable :global(tbody tr:hover) { background: var(--panel); }
  .ttable :global(tr.picked) { outline: 1px solid var(--acc); outline-offset: -1px; }
  .trans :global(.mini.star) { padding: 0 5px; font-size: 13px; line-height: 1.2; }
  .trans :global(.mini.star.on) { color: var(--acc); border-color: var(--acc); }
  .link { border: 0; background: none; color: var(--acc); padding: 0; text-decoration: underline; }
  .trans .ok { color: var(--ok); }
</style>
