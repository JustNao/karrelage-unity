<script lang="ts">
  import { fmtInt, kamas, kamasOr } from "../../../../lib/fmt";
  import type { PlanRow } from "../../../../lib/jobXp";

  /** The plan's crafts in the order they are made (a recipe crafted again later gets another row): ✕ leaves a recipe out (or a
   *  wished one off the wishes; their quantity is typed under the plan). A click on a name copies it (auction house search). */
  let {
    rows,
    name,
    cost,
    subs,
    onexclude,
  }: {
    rows: PlanRow[];
    name: (gid: number) => string;
    cost: (gid: number) => number | null;
    /** items to craft first for one craft of it (its ingredients' sub-crafts) */
    subs: (gid: number) => number[];
    onexclude: (gid: number) => void;
  } = $props();

  let copied = $state<number | null>(null);
  function copy(gid: number) {
    navigator.clipboard?.writeText(name(gid)).then(() => {
      copied = gid;
      setTimeout(() => copied === gid && (copied = null), 900);
    }, () => {});
  }
</script>

<table class="tbl">
  <thead>
    <tr>
      <th>Objet</th>
      <th class="n">Niv.</th>
      <th class="n">Quantité</th>
      <th class="n" title="XP du premier craft (elle baisse quand le métier monte)">XP / craft</th>
      <th class="n">XP totale</th>
      <th class="n" title="Niveaux du métier pendant ces crafts">Niveaux</th>
      <th class="n">Coût / craft</th>
      <th class="n">Coût total</th>
      <th></th>
    </tr>
  </thead>
  <tbody>
    {#each rows as r, i (i)}
      <tr class:forced={r.forced}>
        <td>
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <span class="nm" onclick={() => copy(r.gid)} title="Cliquer pour copier le nom"><b>{name(r.gid)}</b></span>
          {#if r.forced}<span class="tag" title="Ajouté à la main">souhaité</span>{/if}
          {#if subs(r.gid).length}<span class="tag sub" title={`À crafter d'abord : ${subs(r.gid).map(name).join(", ")}`}>sous-craft</span>{/if}
          {#if copied === r.gid}<span class="copied">copié</span>{/if}
        </td>
        <td class="n">{r.level}</td>
        <td class="n">{fmtInt(r.qty)}</td>
        <td class="n">{fmtInt(Math.round(r.xp / Math.max(1, r.qty)))}</td>
        <td class="n">{fmtInt(r.xp)}</td>
        <td class="n">{r.from === r.to ? r.from : `${r.from} → ${r.to}`}</td>
        <td class="n">{kamasOr(cost(r.gid) === null ? null : Math.round(cost(r.gid)!))}</td>
        <td class="n"><b>{r.cost === null ? "—" : kamas(Math.round(r.cost))}</b></td>
        <td><button class="x" onclick={() => onexclude(r.gid)} title={r.forced ? "Retirer des crafts souhaités" : "Ne pas crafter cet objet (le calcul cherche sans lui)"}>✕</button></td>
      </tr>
    {:else}
      <tr><td colspan="9" class="mut">Aucun craft : niveau visé atteint, ou aucune recette de ce métier n'a de prix connu (ouvrez ses ingrédients à l'hôtel de vente).</td></tr>
    {/each}
  </tbody>
</table>

<style>
  .nm { cursor: pointer; }
  .nm:hover { text-decoration: underline; }
  .tag { margin-left: 6px; padding: 0 5px; border-radius: 4px; font-size: 10px; background: color-mix(in srgb, var(--acc) 25%, transparent); color: var(--fg); }
  .tag.sub { background: color-mix(in srgb, var(--gold) 30%, transparent); }
  .copied { margin-left: 6px; color: var(--ok); font-size: 10px; }
  .x { border: 0; background: none; color: var(--mut); padding: 0 4px; cursor: pointer; }
  .x:hover { color: var(--bad); }
</style>
