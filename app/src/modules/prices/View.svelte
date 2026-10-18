<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteMap, SvelteSet } from "svelte/reactivity";
  import { ago, call, kamas, on } from "../../lib/api";
  import SmoothScroll from "../../lib/components/SmoothScroll.svelte";

  interface Price { gid: number; name: string; type_id: number | null; type: string; category: string; unit: number; updated: number; override: number | null }

  const CATEGORIES = ["Équipement", "Ressource", "Consommable", "Autre"];
  let prices = new SvelteMap<number, Price>();
  let shut = new SvelteSet<string>(); // collapsed "category" / "category/type" keys
  let q = $state("");

  const tree = $derived.by(() => {
    const n = q.trim().toLowerCase();
    const cats = new Map<string, Map<string, Price[]>>();
    for (const p of prices.values()) {
      if (n && !p.name.toLowerCase().includes(n) && !p.type.toLowerCase().includes(n)) continue;
      if (!cats.has(p.category)) cats.set(p.category, new Map());
      const types = cats.get(p.category)!;
      if (!types.has(p.type)) types.set(p.type, []);
      types.get(p.type)!.push(p);
    }
    return CATEGORIES.filter((c) => cats.has(c)).map((c) => ({
      name: c,
      count: [...cats.get(c)!.values()].reduce((a, l) => a + l.length, 0),
      types: [...cats.get(c)!.entries()]
        .sort((a, b) => a[0].localeCompare(b[0], "fr"))
        .map(([type, items]) => ({ type, items: items.sort((a, b) => a.name.localeCompare(b.name, "fr")) })),
    }));
  });

  const toggle = (k: string) => (shut.has(k) ? shut.delete(k) : shut.add(k));
  const latest = (items: Price[]) => Math.max(...items.map((p) => p.updated));

  async function setOverride(p: Price, raw: string) {
    const v = raw.trim() === "" ? null : Number(raw);
    if (v !== null && !(v >= 0)) return;
    if (v === p.override) return;
    await call("prices", "set_override", { gid: p.gid, value: v });
  }
  async function forget() {
    if (confirm("Forget all stored prices?")) await call("prices", "forget");
  }

  onMount(() => {
    call<Price[]>("prices", "list").then((l) => l.forEach((p) => prices.set(p.gid, p)));
    const offs = [on<Price>("prices:price", (p) => prices.set(p.gid, p)), on("prices:cleared", () => prices.clear())];
    return () => void offs.forEach((o) => o.then((f) => f()));
  });
</script>

<div class="view">
  <SmoothScroll>
    <div class="head">
      <div class="fh">Stored prices <span class="mut">· per unit, from the highest stack on sale, refreshed each time you open an item</span></div>
      <input type="search" placeholder="Search item or type…" bind:value={q} />
      <button onclick={forget}>Forget all prices</button>
    </div>
    {#each tree as c (c.name)}
      <button class="cat" aria-expanded={!shut.has(c.name)} onclick={() => toggle(c.name)}>
        <span class="chev" class:shut={shut.has(c.name)}>▾</span> {c.name} <span class="mut">{c.count}</span>
      </button>
      {#if !shut.has(c.name)}
        {#each c.types as t (t.type)}
          {@const k = `${c.name}/${t.type}`}
          <button class="type" aria-expanded={!shut.has(k)} onclick={() => toggle(k)}>
            <span class="chev" class:shut={shut.has(k)}>▾</span> {t.type}
            <span class="mut">{t.items.length} · updated {ago(latest(t.items))} ago</span>
          </button>
          {#if !shut.has(k)}
            <table>
              <thead><tr><th>Item</th><th class="n">Captured / unit</th><th>Last updated</th><th class="n">Manual override</th></tr></thead>
              <tbody>
                {#each t.items as p (p.gid)}
                  <tr>
                    <td>{p.name}</td>
                    <td class="n" class:struck={p.override !== null}>{kamas(p.unit)}</td>
                    <td class="mut">{ago(p.updated)} ago</td>
                    <td class="n">
                      <input type="number" min="0" placeholder="—" class:set={p.override !== null} value={p.override ?? ""} onchange={(e) => setOverride(p, e.currentTarget.value)} />
                      {#if p.override !== null}<button class="mini" title="Back to the captured price" onclick={() => setOverride(p, "")}>✕</button>{/if}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}
        {/each}
      {/if}
    {:else}
      <div class="mut">No price yet: open items in the auction house and their unit price is stored here.</div>
    {/each}
  </SmoothScroll>
</div>

<style>
  .view { flex: 1; min-height: 0; display: flex; flex-direction: column; font-family: system-ui, sans-serif; }
  .view > :global(.wrap) { flex: 1; }
  .head { display: flex; align-items: center; gap: 10px; margin-bottom: 10px; flex-wrap: wrap; }
  .head .fh { margin: 0; flex: 1; }
  .head input { width: 200px; padding: 3px 6px; }
  .cat, .type { display: flex; width: 100%; align-items: center; gap: 6px; border: 0; border-radius: 0; text-align: left; background: none; color: inherit; }
  .cat { font: 700 14px system-ui; padding: 8px 4px; border-bottom: 1px solid var(--line); margin-top: 8px; }
  .type { font: 600 12px system-ui; padding: 5px 4px 5px 18px; }
  .chev { display: inline-block; transition: transform 0.2s; }
  .chev.shut { transform: rotate(-90deg); }
  table { width: calc(100% - 22px); margin: 0 0 6px 22px; border-collapse: collapse; font-size: 12px; }
  th { text-align: left; font-weight: 400; color: var(--mut); padding: 2px 6px; }
  td { padding: 3px 6px; border-top: 1px solid var(--line); }
  .n { text-align: right; }
  .struck { text-decoration: line-through; opacity: 0.5; }
  td input { width: 100px; padding: 2px 4px; text-align: right; }
  td input.set { border-color: var(--acc); color: var(--acc); }
  .mini { padding: 1px 6px; font-size: 11px; }
</style>
