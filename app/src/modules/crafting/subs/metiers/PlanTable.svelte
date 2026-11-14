<script lang="ts">
  import { fmtInt, kamas, kamasOr } from "../../../../lib/fmt";
  import type { PlanRow } from "../../../../lib/jobXp";

  /** The plan's crafts in the order they are made (lowest job level first): ✕ leaves a recipe out (or a wished one off the
   *  wishes), the quantity of a wished craft can be typed. A click on a name copies it (auction house search). */
  let {
    rows,
    name,
    cost,
    onexclude,
    onqty,
  }: {
    rows: PlanRow[];
    name: (gid: number) => string;
    cost: (gid: number) => number | null;
    onexclude: (gid: number) => void;
    onqty: (gid: number, qty: number) => void;
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
    {#each rows as r (r.gid)}
      <tr class:forced={r.forced}>
        <td>
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <span class="nm" onclick={() => copy(r.gid)} title="Cliquer pour copier le nom"><b>{name(r.gid)}</b></span>
          {#if r.forced}<span class="tag" title="Ajouté à la main">souhaité</span>{/if}
          {#if copied === r.gid}<span class="copied">copié</span>{/if}
        </td>
        <td class="n">{r.level}</td>
        <td class="n">
          {#if r.forced}
            <input type="number" min="1" value={r.qty} onchange={(e) => onqty(r.gid, Number(e.currentTarget.value))} />
          {:else}{fmtInt(r.qty)}{/if}
        </td>
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
  .copied { margin-left: 6px; color: var(--ok); font-size: 10px; }
  input { width: 64px; padding: 1px 4px; text-align: right; }
  .x { border: 0; background: none; color: var(--mut); padding: 0 4px; cursor: pointer; }
  .x:hover { color: var(--bad); }
</style>
