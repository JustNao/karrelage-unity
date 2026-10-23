<script lang="ts">
  import type { Columns } from "../../lib/columns.svelte";
  import PanelHead from "../../lib/components/PanelHead.svelte";
  import EffectPicker from "../../lib/components/EffectPicker.svelte";
  import ItemCard from "../../lib/components/ItemCard.svelte";
  import { LEVEL_EFFECT } from "../../lib/companion";
  import { FLAG_EFFECTS, rangeLabel, sections, type EffectMeta } from "../../lib/effects";
  import { iconOfStat } from "../../lib/stores/statIcons.svelte";
  import { effectLabel, type EffectRow, type Filters } from "./filters.svelte";

  /** HDV right panel: the pinned lot, then the open item's effect filters (companions: level and legendary status). */
  let {
    cols,
    item,
    comp,
    filt,
    meta,
    picker,
    pin,
    onunpin,
    legendOnly = $bindable(),
  }: {
    cols: Columns<"list" | "filter">;
    item: any;
    comp: boolean;
    filt: Filters;
    /** every labelled effect */
    meta: Record<number, EffectMeta>;
    /** effects the picker offers (found on level 150-200 equipment) */
    picker: EffectMeta[];
    pin: { item: any; row: any } | null;
    onunpin: () => void;
    legendOnly: boolean;
  } = $props();

  const iconOf = (a: number) => iconOfStat(meta[a]?.stat); // Dofusbook stat icons (shared store)
  const effLabel = (a: number) => effectLabel(meta, a);

  /** Weapon hits ("dommages X", "vol X") are not filtered on. */
  const filterable = (a: number) => !!meta[a]?.filterable && meta[a]?.group !== "weapon";

  /** Effects the item has by default (filterable ones only), then the ones the user added himself. */
  const effectRows = $derived.by<EffectRow[]>(() => {
    if (!item) return [];
    const seen = new Map<number, EffectRow>();
    for (const d of item.defaults ?? []) if (filterable(d.a)) seen.set(d.a, { a: d.a, def: [d.min, d.max] });
    for (const x of filt.list(item.gid)) if (!seen.has(x.a)) seen.set(x.a, { a: x.a, def: null });
    return [...seen.values()];
  });

  const pickable = $derived(picker.filter((e) => e.group !== "weapon"));

  /** Filter rows grouped like the in-game characteristics panel. */
  const groups = $derived([
    // the effects the user added come first, in their own section
    ...(effectRows.some((x) => x.def == null) ? [{ key: "added", label: "Effets ajoutés", items: effectRows.filter((x) => x.def == null).map((x) => ({ ...x, label: effLabel(x.a) })) }] : []),
    ...sections(effectRows.filter((x) => x.def != null).map((x) => ({ ...x, group: meta[x.a]?.group ?? "other", order: meta[x.a]?.order ?? 9999, label: effLabel(x.a) }))),
  ]);
  const preset = (p: string) => filt.preset(p, effectRows, meta);
</script>

<div class="filter" class:cz={cols.collapsed.filter}>
  <PanelHead {cols} name="filter" title="Filtres" dir={-1} />
  {#if pin}
    <div class="pinned">
      <div class="fh">Objet épinglé <span class="mut">· {pin.item.name}{pin.item.level != null ? " niv. " + pin.item.level : ""}</span></div>
      <ItemCard row={pin.row} companion={!!pin.item.companion} perfectFlags={false} pinned onpin={onunpin} />
    </div>
  {/if}
  {#if item}
    {#if comp}
      <div class="fh">Filtrer les {item.type?.toLowerCase() ?? "compagnon"}s</div>
      {@const lv = filt.list(item.gid).find((z) => z.a === LEVEL_EFFECT)}
      <div class="frow lv" class:on={filt.active(LEVEL_EFFECT)}>
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <div class="lb" onclick={() => filt.clear(LEVEL_EFFECT)} title="Cliquer pour effacer">Niveau</div>
        <input type="number" placeholder="min" value={lv?.min ?? ""} oninput={(e) => (filt.of(item.gid, LEVEL_EFFECT).min = e.currentTarget.value)} />
        <input type="number" placeholder="max" value={lv?.max ?? ""} oninput={(e) => (filt.of(item.gid, LEVEL_EFFECT).max = e.currentTarget.value)} />
      </div>
      <label class="leg"><input type="checkbox" bind:checked={legendOnly} /> Statut légendaire uniquement</label>
    {:else}
    <div class="fh">Filtrer les effets</div>
    <div class="btns">
      <button onclick={() => preset("min")} title="min = valeur la plus basse de la plage de base de chaque effet">Valeurs min</button>
      <button onclick={() => preset("damage")} title="parfait (max de base) pour les effets de dommages">Dommages</button>
      <button onclick={() => preset("resist")} title="parfait (max de base) pour les % de résistance">Résistances</button>
      <button onclick={() => preset("clear")}>Effacer</button>
    </div>
    <div class="btns"><EffectPicker effects={pickable} onpick={(id) => filt.of(item.gid, id)} /></div>
    {#each groups as g (g.key)}
      <div class="ghead">
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <span class="glb" class:on={g.items.some((x) => filt.active(x.a))} onclick={() => filt.toggleGroup(g)} title="Cliquer pour remplir le min de chaque effet avec le bas de sa plage de base (encore une fois pour effacer)">{g.label}</span>
        {#if g.key !== "added"}
          <span class="gbtns">
            <button class="mini" onclick={() => g.items.forEach((x) => filt.setPerf(x.a, x.def))} title="Remplir le min de tous les effets de cette section avec le max de leur plage de base (jet parfait)">Perf</button>
          </span>
        {/if}
      </div>
      {#each g.items as x (x.a)}
        {@const y = filt.list(item.gid).find((z) => z.a === x.a)}
        <div class="frow" class:on={filt.active(x.a)}>
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="lb" onclick={() => filt.toggle(x.a, x.def)} title="Cliquer pour remplir le min avec le bas de la plage de base (encore une fois pour effacer)">{#if iconOf(x.a)}<img class="sic" src={iconOf(x.a)} alt="" />{/if}{x.label}</div>
          {#if FLAG_EFFECTS.has(x.a)}
            <label class="tog"><input type="checkbox" checked={filt.active(x.a)} onchange={(e) => (filt.of(item.gid, x.a).min = e.currentTarget.checked ? "1" : "")} /> Uniquement les lots qui l'ont</label>
          {:else}
          <small class="rg">{rangeLabel(x.def)}</small>
          <input type="number" placeholder="min" value={y?.min ?? ""} oninput={(e) => (filt.of(item.gid, x.a).min = e.currentTarget.value)} />
          <input type="number" placeholder="max" value={y?.max ?? ""} oninput={(e) => (filt.of(item.gid, x.a).max = e.currentTarget.value)} />
          {#if x.def == null}
            <button class="mini" onclick={() => (filt.of(item.gid, x.a).min = "1")} title="Remplir le min avec 1 (lots qui ont cet effet)">+1</button>
          {:else}
            <button class="mini" onclick={() => filt.setPerf(x.a, x.def)} title="Remplir le min avec le max de la plage de base (jet parfait)">Perf</button>
          {/if}
          {/if}
        </div>
      {/each}
    {:else}
      <div class="mut">Aucun effet</div>
    {/each}
    {/if}
  {/if}
</div>

<style>
  .filter :global(.phead) { padding-left: 0; padding-right: 0; }
  .filter { overflow: auto; background: var(--panel); border-left: 1px solid var(--line); padding: 0 10px 10px; font-family: var(--font); font-size: 12px; }
  .filter .pinned { margin-bottom: 12px; }
  .filter .ghead { display: flex; align-items: center; justify-content: flex-start; gap: 10px; }
  .ghead { margin: 12px 0 2px; font-size: 10px; letter-spacing: 0.08em; text-transform: uppercase; color: var(--mut); }
  .gbtns { display: flex; gap: 4px; text-transform: none; letter-spacing: 0; }
  .btns { display: flex; gap: 6px; flex-wrap: wrap; margin-bottom: 8px; }
  .frow.lv { grid-template-columns: 1fr 52px 52px; }
  .leg { display: block; margin: 8px 0; }
  .frow { display: grid; grid-template-columns: 1fr auto 52px 52px auto; gap: 4px; align-items: center; margin: 3px 0; }
  .frow input { width: 100%; padding: 2px 4px; }
  .frow .tog { grid-column: 2 / -1; display: flex; gap: 6px; align-items: center; }
  .frow .tog input { width: auto; }
  .frow .sic { width: 16px; height: 16px; vertical-align: -3px; margin-right: 5px; }
  .frow .lb { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; cursor: pointer; }
  .frow .lb:hover, .glb:hover { color: var(--acc); }
  .glb { cursor: pointer; }
  .glb.on { color: var(--acc); }
  .frow .rg { color: var(--mut); text-align: right; white-space: nowrap; padding-right: 2px; }
  .frow.on .lb { font-weight: 700; color: var(--acc); }
</style>
