<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { call } from "../../lib/api";
  import { ago, kamas } from "../../lib/fmt";
  import { effectivePrice, prices, type PriceRow as Price } from "../../lib/stores/prices.svelte";
  import Collapsible from "../../lib/components/Collapsible.svelte";
  import ConfirmButton from "../../lib/components/ConfirmButton.svelte";
  import SearchBar, { matcher } from "../../lib/components/SearchBar.svelte";
  import SmoothScroll from "../../lib/components/SmoothScroll.svelte";

  const CATEGORIES = ["Équipement", "Ressource", "Consommable", "Autre"];
  const collator = new Intl.Collator("fr");
  let opened = new SvelteSet<string>(); // expanded "category" / "category/type" keys: everything starts collapsed
  let q = $state("");

  // Prices grouped by category / type as they arrive; only the groups a price lands in (or leaves) are sorted again.
  const groups = new Map<string, Map<string, Map<number, Price>>>(); // category -> type -> gid -> price
  const where = new Map<number, [string, string]>(); // gid -> [category, type]
  const sortedCache = new Map<Map<number, Price>, Price[]>();
  let version = $state(0);
  function put(p: Price) {
    const old = where.get(p.gid);
    if (old && (old[0] !== p.category || old[1] !== p.type)) {
      const g = groups.get(old[0])?.get(old[1]);
      g?.delete(p.gid);
      if (g) sortedCache.delete(g);
    }
    let types = groups.get(p.category);
    if (!types) groups.set(p.category, (types = new Map()));
    let g = types.get(p.type);
    if (!g) types.set(p.type, (g = new Map()));
    g.set(p.gid, p);
    sortedCache.delete(g);
    where.set(p.gid, [p.category, p.type]);
  }
  const putAll = (l: Iterable<Price>) => {
    for (const p of l) put(p);
    version++;
  };
  const clearAll = () => (groups.clear(), where.clear(), sortedCache.clear(), version++);
  const sorted = (g: Map<number, Price>) => {
    let l = sortedCache.get(g);
    if (!l) sortedCache.set(g, (l = [...g.values()].sort((a, b) => collator.compare(a.name, b.name))));
    return l;
  };

  const tree = $derived.by(() => {
    version;
    const m = matcher(q);
    return CATEGORIES.filter((c) => groups.has(c))
      .map((c) => {
        const types = [...groups.get(c)!.entries()]
          .map(([type, g]) => ({ type, items: m(type) ? sorted(g) : sorted(g).filter((p) => m(p.name)) }))
          .filter((t) => t.items.length)
          .sort((a, b) => collator.compare(a.type, b.type));
        return { name: c, count: types.reduce((a, t) => a + t.items.length, 0), types };
      })
      .filter((c) => c.types.length);
  });

  /** Collapsed unless opened; while searching, matches are shown expanded. */
  const isShut = (k: string) => !q.trim() && !opened.has(k);
  const toggle = (k: string) => (opened.has(k) ? opened.delete(k) : opened.add(k));
  const stamp = (p: Price) => p.updated ?? p.avg_updated ?? 0;
  const latest = (items: Price[]) => Math.max(...items.map(stamp));

  async function setOverride(p: Price, raw: string) {
    const v = raw.trim() === "" ? null : Number(raw);
    if (v !== null && !(v >= 0)) return;
    if (v === p.override) return;
    await call("prices", "set_override", { gid: p.gid, value: v });
  }

  onMount(() => {
    // the shared store has the prices; this tab keeps its own grouping, updated with each change
    const off = prices.subscribe((c) => (c.kind === "clear" ? clearAll() : putAll(c.prices)));
    void prices.ensure().then(() => {
      clearAll();
      putAll(prices.map.values());
    });
    return off;
  });
</script>

<div class="view">
  <SmoothScroll>
    <div class="head">
      <div class="fh">Prix enregistrés <span class="mut">· à l'unité, d'après le plus gros lot en vente, mis à jour à chaque ouverture d'un objet</span></div>
      <SearchBar bind:value={q} placeholder="Rechercher un objet ou un type…" compact />
      <ConfirmButton label="Oublier tous les prix" title="Oublier tous les prix enregistrés" onconfirm={() => call("prices", "forget")} />
    </div>
    {#each tree as c (c.name)}
      <Collapsible head="cat" title={c.name} count={c.count} open={!isShut(c.name)} ontoggle={() => toggle(c.name)}>
        {#each c.types as t (t.type)}
          {@const k = `${c.name}/${t.type}`}
          <Collapsible head="type" title={t.type} count="{t.items.length} · mis à jour {ago(latest(t.items))}" open={!isShut(k)} ontoggle={() => toggle(k)}>
            <table class="tbl">
              <colgroup><col /><col class="c1" /><col class="c2" /><col class="c3" /><col class="c4" /><col class="c5" /></colgroup>
              <thead><tr><th>Objet</th><th class="n">Prix utilisé</th><th class="n">Relevé HDV / unité</th><th class="n">Moyenne 7 j</th><th>Dernier relevé</th><th class="n">Prix manuel (kamas)</th></tr></thead>
              <tbody>
                {#each t.items as p (p.gid)}
                  {@const u = effectivePrice(p)}
                  <tr>
                    <td>{p.name}</td>
                    <td class="n"><b>{u ? kamas(u.value) : "—"}</b> {#if u}<small class="src {u.source}">{u.source}</small>{/if}</td>
                    <td class="n" class:dim={p.override != null}>{p.unit != null ? kamas(p.unit) : "—"}</td>
                    <td class="n" class:dim={p.override != null || p.unit != null}>{p.avg != null ? kamas(p.avg) : "—"}</td>
                    <td class="mut">{p.updated != null ? ago(p.updated) : "—"}</td>
                    <td class="n">
                      <input type="number" min="0" placeholder="—" class:set={p.override !== null} value={p.override ?? ""} onchange={(e) => setOverride(p, e.currentTarget.value)} />
                      {#if p.override !== null}<button class="mini" title="Revenir au prix relevé" onclick={() => setOverride(p, "")}>✕</button>{/if}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </Collapsible>
        {/each}
      </Collapsible>
    {:else}
      <div class="mut">Aucun prix pour l'instant : ouvrez des objets à l'hôtel de vente, leur prix unitaire sera enregistré ici.</div>
    {/each}
  </SmoothScroll>
</div>

<style>
  .view { flex: 1; min-height: 0; display: flex; flex-direction: column; font-family: var(--font); }
  .view > :global(.wrap) { flex: 1; }
  .head { display: flex; align-items: center; gap: 10px; margin-bottom: 10px; flex-wrap: wrap; }
  .head .fh { margin: 0; flex: 1; }
  .view :global(.chd.cat) { font: 700 14px var(--font); padding: 8px 4px; border-bottom: 1px solid var(--line); margin-top: 8px; }
  .view :global(.chd.type) { font: 600 12px var(--font); padding: 5px 4px 5px 18px; }
  /* fixed layout + the same widths in every table: the price columns line up across all item types */
  .tbl { width: calc(100% - 22px); margin: 0 0 6px 22px; table-layout: fixed; }
  .tbl th { position: static; }
  col.c1 { width: 190px; } col.c2 { width: 150px; } col.c3 { width: 150px; } col.c4 { width: 120px; } col.c5 { width: 190px; }
  .tbl td { padding: 3px 6px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dim { opacity: 0.45; }
  .src { color: var(--mut); margin-left: 4px; }
  .src.manuel { color: var(--acc); }
  .src.moyenne { color: var(--warn); }
  td input { width: 100px; padding: 2px 4px; text-align: right; }
  td input.set { border-color: var(--acc); color: var(--acc); }
</style>
