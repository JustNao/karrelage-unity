<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteMap } from "svelte/reactivity";
  import { call, kamas, on } from "../../../../lib/api";
  import { storedPrice } from "../../../../lib/craft";
  import { xpOfLevel } from "../../../../lib/companion";
  import SmoothScroll from "../../../../lib/components/SmoothScroll.svelte";
  import type { StoredPrice } from "../../../../lib/transcend";
  import table from "./familiar_xp.json";

  /** Croquette enrichie: a flat 500 xp (not in the site's table). */
  const CROQUETTE = { name: "Croquette enrichie", xp: 500, qty: 1 };
  /** Food table from dofustool.com: [name, xp given per meal, quantity of the resource per meal]. */
  const FOODS: { name: string; xp: number; qty: number }[] = (() => {
    // the site lists some resources twice: keep the entry with the best xp per unit
    const by = new Map<string, { name: string; xp: number; qty: number }>();
    for (const [name, xp, qty] of [[CROQUETTE.name, CROQUETTE.xp, CROQUETTE.qty], ...(table.foods as [string, number, number][])] as [string, number, number][]) {
      const old = by.get(name);
      if (!old || xp / qty > old.xp / old.qty) by.set(name, { name, xp, qty });
    }
    return [...by.values()];
  })();

  type SortKey = "name" | "xp" | "qty" | "unit" | "meal" | "perXp" | "meals" | "total";
  interface Row { gid: number; name: string; xp: number; qty: number; unit: number | null; meal: number | null; perXp: number | null; meals: number; total: number | null }

  let gids = $state.raw<Record<string, number>>({});
  let stored = new SvelteMap<number, StoredPrice>();
  let level = $state(1);
  let xp = $state(0);
  let target = $state(100);
  let q = $state("");
  let err = $state("");
  let onlyPriced = $state(false);
  let sort = $state<SortKey>("total");
  let desc = $state(false);

  /** Total xp of a level; level 1 starts at 0. */
  const base = (l: number) => (l <= 1 ? 0 : xpOfLevel(l));
  const need = $derived(Math.max(0, base(Math.min(Math.max(target, 1), 100)) - base(Math.min(Math.max(level, 1), 100)) - Math.max(0, xp || 0)));

  const rows = $derived.by<Row[]>(() =>
    FOODS.flatMap((f) => {
      const gid = gids[f.name];
      if (gid == null) return [];
      const unit = storedPrice(stored, gid);
      const meal = unit != null ? unit * f.qty : null;
      const perXp = meal != null ? meal / f.xp : null;
      const meals = Math.ceil(need / f.xp);
      return [{ gid, name: f.name, xp: f.xp, qty: f.qty, unit, meal, perXp, meals, total: meal != null ? meal * meals : null }];
    }),
  );

  const view = $derived.by(() => {
    const n = q.trim().toLowerCase();
    const k = sort;
    const out = rows.filter((r) => (!n || r.name.toLowerCase().includes(n)) && (!onlyPriced || r.unit != null));
    return out.sort((a, b) => {
      const x = a[k], y = b[k];
      if (x == null && y == null) return 0;
      if (x == null) return 1; // unknown values always last
      if (y == null) return -1;
      const d = typeof x === "string" ? x.localeCompare(y as string, "fr") : (x as number) - (y as number);
      return desc ? -d : d;
    });
  });
  const best = $derived(rows.filter((r) => r.total != null).sort((a, b) => a.total! - b.total!)[0] ?? null);

  const setSort = (k: SortKey) => {
    if (sort === k) desc = !desc;
    else { sort = k; desc = false; }
  };
  const arrow = (k: SortKey) => (sort === k ? (desc ? " ▾" : " ▴") : "");
  const num = (n: number) => n.toLocaleString("fr-FR", { maximumFractionDigits: 2 });
  const int = (e: Event) => Number((e.currentTarget as HTMLInputElement).value);

  onMount(() => {
    call<Record<string, number>>("prices", "gids_by_name", { names: FOODS.map((f) => f.name) }).then((m) => (gids = m)).catch((e) => (err = String(e)));
    call<StoredPrice[]>("prices", "list").then((l) => l.forEach((p) => stored.set(p.gid, p)));
    const offs = [on<StoredPrice>("prices:price", (p) => stored.set(p.gid, p)), on<StoredPrice[]>("prices:bulk", (l) => l.forEach((p) => stored.set(p.gid, p))), on("prices:cleared", () => stored.clear())];
    return () => void offs.forEach((o) => o.then((f) => f()));
  });
</script>

<div class="rv">
  <div class="head">
    <div class="fh">Familiers</div>
  </div>
  <div class="form">
    <label>Niveau actuel <input type="number" min="1" max="100" value={level} oninput={(e) => (level = int(e))} /></label>
    <label>XP dans ce niveau <input type="number" min="0" value={xp} oninput={(e) => (xp = int(e))} /></label>
    <label>Niveau désiré <input type="number" min="1" max="100" value={target} oninput={(e) => (target = int(e))} /></label>
    <div class="res">
      <b>{num(need)}</b> XP à gagner
      {#if best && need > 0}· avec <b>{best.name}</b> : <b>{best.meals}</b> repas ({num(best.meals * best.qty)} ressource{best.meals * best.qty > 1 ? "s" : ""}) pour <b class="good">{kamas(Math.round(best.total!))}</b>
      {:else if need > 0}<span class="mut">· prix inconnus</span>{/if}
    </div>
  </div>
  <div class="head">
    <input type="search" placeholder="Rechercher une ressource…" bind:value={q} />
    <label><input type="checkbox" bind:checked={onlyPriced} /> Prix connus seulement</label>
  </div>
  {#if err}<div class="bad">Erreur : {err} (relancez l'application pour charger la dernière version)</div>{/if}
  <SmoothScroll>
    <table>
      <thead>
        <tr>
          <th><button onclick={() => setSort("name")}>Ressource{arrow("name")}</button></th>
          <th class="n"><button onclick={() => setSort("xp")}>XP / repas{arrow("xp")}</button></th>
          <th class="n"><button onclick={() => setSort("qty")}>Quantité / repas{arrow("qty")}</button></th>
          <th class="n"><button onclick={() => setSort("unit")}>Prix unitaire{arrow("unit")}</button></th>
          <th class="n"><button onclick={() => setSort("meal")}>Coût / repas{arrow("meal")}</button></th>
          <th class="n"><button onclick={() => setSort("perXp")}>Coût / XP{arrow("perXp")}</button></th>
          <th class="n"><button onclick={() => setSort("meals")}>Repas nécessaires{arrow("meals")}</button></th>
          <th class="n"><button onclick={() => setSort("total")}>Prix estimé{arrow("total")}</button></th>
        </tr>
      </thead>
      <tbody>
        {#each view as r (r.name)}
          <tr>
            <td><b>{r.name}</b></td>
            <td class="n">{num(r.xp)}</td>
            <td class="n">{r.qty}</td>
            <td class="n">{r.unit != null ? kamas(r.unit) : "—"}</td>
            <td class="n">{r.meal != null ? kamas(Math.round(r.meal)) : "—"}</td>
            <td class="n">{r.perXp != null ? num(r.perXp) + " K" : "—"}</td>
            <td class="n">{need > 0 ? r.meals : "—"}</td>
            <td class="n"><b>{r.total != null && need > 0 ? kamas(Math.round(r.total)) : "—"}</b></td>
          </tr>
        {:else}
          <tr><td colspan="8" class="mut">Aucune ressource à afficher : ouvrez-les à l'hôtel de vente pour enregistrer leur prix (ou désactivez « Prix connus seulement »).</td></tr>
        {/each}
      </tbody>
    </table>
  </SmoothScroll>
</div>

<style>
  .rv { flex: 1; min-height: 0; display: flex; flex-direction: column; font-family: var(--font); padding: 8px 10px 0; }
  .rv > :global(.wrap) { flex: 1; }
  .head { display: flex; align-items: center; gap: 10px; margin-bottom: 10px; flex-wrap: wrap; }
  .head .fh { margin: 0; flex: 1; }
  .head input[type="search"] { width: 200px; padding: 3px 6px; }
  .form { display: flex; align-items: center; gap: 14px; flex-wrap: wrap; margin-bottom: 10px; padding: 8px 10px; background: var(--panel); border: 1px solid var(--line); }
  .form label { display: flex; align-items: center; gap: 6px; }
  .form input { width: 90px; padding: 3px 6px; text-align: right; }
  .res { flex: 1; min-width: 260px; }
  table { width: 100%; border-collapse: collapse; font-size: 12px; }
  th { text-align: left; font-weight: 400; color: var(--mut); padding: 2px 6px; position: sticky; top: 0; background: var(--bg); }
  th button { border: 0; background: none; color: inherit; font: inherit; padding: 0; cursor: pointer; }
  td { padding: 4px 6px; border-top: 1px solid var(--line); }
  .n { text-align: right; white-space: nowrap; }
  .bad { color: var(--bad, #e05555); margin-bottom: 6px; }
  .good { color: var(--good, #4caf50); }
</style>
