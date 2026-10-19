<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { ago, call, kamas, on } from "../../lib/api";
  import SmoothScroll from "../../lib/components/SmoothScroll.svelte";

  interface Price { gid: number; name: string; type_id: number | null; type: string; category: string; unit: number | null; updated?: number | null; avg?: number | null; avg_updated?: number | null; override: number | null }

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
  const putAll = (l: Price[]) => (l.forEach(put), version++);
  const clearAll = () => (groups.clear(), where.clear(), sortedCache.clear(), version++);
  const sorted = (g: Map<number, Price>) => {
    let l = sortedCache.get(g);
    if (!l) sortedCache.set(g, (l = [...g.values()].sort((a, b) => collator.compare(a.name, b.name))));
    return l;
  };

  const tree = $derived.by(() => {
    version;
    const n = q.trim().toLowerCase();
    return CATEGORIES.filter((c) => groups.has(c))
      .map((c) => {
        const types = [...groups.get(c)!.entries()]
          .map(([type, g]) => ({ type, items: n && !type.toLowerCase().includes(n) ? sorted(g).filter((p) => p.name.toLowerCase().includes(n)) : sorted(g) }))
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
  /** The price used elsewhere: manual, else read in the auction house, else the weekly average. */
  const used = (p: Price): { v: number; src: string } | null =>
    p.override != null ? { v: p.override, src: "manuel" } : p.unit != null ? { v: p.unit, src: "relevé" } : p.avg != null ? { v: p.avg, src: "moyenne" } : null;

  async function setOverride(p: Price, raw: string) {
    const v = raw.trim() === "" ? null : Number(raw);
    if (v !== null && !(v >= 0)) return;
    if (v === p.override) return;
    await call("prices", "set_override", { gid: p.gid, value: v });
  }
  async function forget() {
    if (confirm("Oublier tous les prix enregistrés ?")) await call("prices", "forget");
  }

  onMount(() => {
    call<Price[]>("prices", "list").then(putAll);
    const offs = [on<Price>("prices:price", (p) => putAll([p])), on<Price[]>("prices:bulk", putAll), on("prices:cleared", clearAll)];
    return () => void offs.forEach((o) => o.then((f) => f()));
  });
</script>

<div class="view">
  <SmoothScroll>
    <div class="head">
      <div class="fh">Prix enregistrés <span class="mut">· à l'unité, d'après le plus gros lot en vente, mis à jour à chaque ouverture d'un objet</span></div>
      <input type="search" placeholder="Rechercher un objet ou un type…" bind:value={q} />
      <button onclick={forget}>Oublier tous les prix</button>
    </div>
    {#each tree as c (c.name)}
      <button class="cat" aria-expanded={!isShut(c.name)} onclick={() => toggle(c.name)}>
        <span class="chev" class:shut={isShut(c.name)}>▾</span> {c.name} <span class="mut">{c.count}</span>
      </button>
      {#if !isShut(c.name)}
        {#each c.types as t (t.type)}
          {@const k = `${c.name}/${t.type}`}
          <button class="type" aria-expanded={!isShut(k)} onclick={() => toggle(k)}>
            <span class="chev" class:shut={isShut(k)}>▾</span> {t.type}
            <span class="mut">{t.items.length} · mis à jour {ago(latest(t.items))}</span>
          </button>
          {#if !isShut(k)}
            <table>
              <colgroup><col /><col class="c1" /><col class="c2" /><col class="c3" /><col class="c4" /><col class="c5" /></colgroup>
              <thead><tr><th>Objet</th><th class="n">Prix utilisé</th><th class="n">Relevé HDV / unité</th><th class="n">Moyenne 7 j</th><th>Dernier relevé</th><th class="n">Prix manuel (kamas)</th></tr></thead>
              <tbody>
                {#each t.items as p (p.gid)}
                  {@const u = used(p)}
                  <tr>
                    <td>{p.name}</td>
                                        <td class="n"><b>{u ? kamas(u.v) : "—"}</b> {#if u}<small class="src {u.src}">{u.src}</small>{/if}</td>
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
          {/if}
        {/each}
      {/if}
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
  .head input { width: 200px; padding: 3px 6px; }
  .cat, .type { display: flex; width: 100%; align-items: center; gap: 6px; border: 0; border-radius: 0; text-align: left; background: none; color: inherit; }
  .cat { font: 700 14px var(--font); padding: 8px 4px; border-bottom: 1px solid var(--line); margin-top: 8px; }
  .type { font: 600 12px var(--font); padding: 5px 4px 5px 18px; }
  .chev { display: inline-block; transition: transform 0.2s; }
  .chev.shut { transform: rotate(-90deg); }
  /* fixed layout + the same widths in every table: the price columns line up across all item types */
  table { width: calc(100% - 22px); margin: 0 0 6px 22px; border-collapse: collapse; font-size: 12px; table-layout: fixed; }
  col.c1 { width: 190px; } col.c2 { width: 150px; } col.c3 { width: 150px; } col.c4 { width: 120px; } col.c5 { width: 190px; }
  td { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  th { text-align: left; font-weight: 400; color: var(--mut); padding: 2px 6px; }
  td { padding: 3px 6px; border-top: 1px solid var(--line); }
  .n { text-align: right; }
  .dim { opacity: 0.45; }
  .src { color: var(--mut); margin-left: 4px; }
  .src.manuel { color: var(--acc); }
  .src.moyenne { color: var(--warn, orange); }
  .struck { text-decoration: line-through; opacity: 0.5; }
  td input { width: 100px; padding: 2px 4px; text-align: right; }
  td input.set { border-color: var(--acc); color: var(--acc); }
  .mini { padding: 1px 6px; font-size: 11px; }
</style>
