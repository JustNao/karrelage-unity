<script lang="ts">
  import { onMount } from "svelte";
  import SubView from "../../../../lib/components/SubView.svelte";
  import Switch from "../../../../lib/components/Switch.svelte";
  import { cached, call, listenAll } from "../../../../lib/api";
  import { persisted } from "../../../../lib/persist.svelte";
  import type { Recipes } from "../../../../lib/craft";
  import { prices } from "../../../../lib/stores/prices.svelte";
  import { fight } from "../../../../lib/stores/combat.svelte";
  import { fmtInt, kamas } from "../../../../lib/fmt";
  import { openShopList, shopRows } from "../../../../lib/shoplist";
  import {
    BASE_JOB,
    Costs,
    MAX_JOB_LEVEL,
    XP_COEFS,
    levelFloor,
    levelOf,
    plan,
    reprice,
    type JobCatalog,
    type JobLevels,
    type JobRecipe,
  } from "../../../../lib/jobXp";
  import PlanTable from "./PlanTable.svelte";
  import Choices from "./Choices.svelte";

  /**
   * Crafting « XP métiers »: a job's level and xp (from the game, jobs.rs; can be typed over), a target level, and the cheapest
   * crafts to get there (lib/jobXp.ts `plan`). Recipes can be left out or wished (with a quantity); ingredients are bought, or
   * with « Craftable par moi » crafted when cheaper by one of the character's jobs at its level (rows needing such a sub-craft
   * are flagged). The resources to buy open in a floating list.
   */
  const ui = persisted("karrelage.crafting.jobxp", {
    job: null as number | null,
    character: null as number | null,
    target: MAX_JOB_LEVEL,
    coef: 100,
    epic: false,
    /** typed over the game's values, per job: level + xp in that level */
    overrides: {} as Record<string, { level: number; inLevel: number }>,
    /** recipes left out, per job */
    excluded: {} as Record<string, number[]>,
    /** wished crafts, per job: gid -> quantity */
    forced: {} as Record<string, Record<string, number>>,
    /** ingredients crafted (when cheaper) with the character's jobs at their levels, else all bought */
    craftMine: true,
    /** most crafts of one recipe in the plan (wished crafts aside), null = no limit */
    maxCrafts: null as number | null,
    /** the resources already in the inventory and the bank are used first (a craft costs only what is missing) */
    useOwned: true,
    /** frozen plans, per job: their crafts kept as they were, priced again (prices, owned resources) */
    frozen: {} as Record<string, { t: number; rows: { gid: number; level: number; qty: number; forced: boolean }[] }>,
  });
  /** « Crafts max par recette » choices: past a few thousand, the ingredients' prices would not hold */
  const MAX_CRAFTS = [100, 250, 500, 1000, 2000, 5000];

  let catalog = $state.raw<JobCatalog>({ jobs: {}, crafts: {} });
  let recipes = $state.raw<Recipes>({ recipes: {}, names: {} });
  let levels = $state.raw<JobLevels>({ current: 0, characters: {} });
  let err = $state("");
  /** inventory + bank (inventory.rs `owned`): gid -> quantity; `bank.t` = when the bank was last seen merged at an auction house */
  interface Owned { items: Record<string, number>; inventory: boolean; bank: { t: number | null; count: number } }
  let owned = $state.raw<Owned>({ items: {}, inventory: false, bank: { t: null, count: 0 } });

  onMount(() => {
    cached<JobCatalog>("jobs", "catalog").then((c) => (catalog = c)).catch((e) => (err = String(e)));
    cached<Recipes>("auction", "recipes").then((r) => (recipes = r)).catch((e) => (err = String(e)));
    // owned resources change with every loot or purchase: asked again once they settle
    let timer: ReturnType<typeof setTimeout> | undefined;
    const fetchOwned = () => call<Owned>("inventory", "owned", {}, true).then((o) => (owned = o)).catch(() => {});
    const off = listenAll({
      "jobs:levels": (l: JobLevels) => (levels = l),
      "inventory:owned": () => (clearTimeout(timer), (timer = setTimeout(fetchOwned, 500))),
    });
    off.ready.then(() => call<JobLevels>("jobs", "levels")).then((l) => (levels = l)).catch((e) => (err = String(e)));
    void fetchOwned();
    void prices.ensure();
    return () => (off(), clearTimeout(timer));
  });

  const name = (gid: number) => recipes.names[gid] ?? `#${gid}`;
  const jobs = $derived(Object.entries(catalog.jobs).map(([id, n]) => ({ id: Number(id), name: n })).filter((j) => j.id !== BASE_JOB).sort((a, b) => a.name.localeCompare(b.name)));

  // the character whose jobs are read: the one chosen, else the last one logged in
  const characters = $derived(Object.keys(levels.characters).map(Number).filter((c) => Object.keys(levels.characters[c] ?? {}).length));
  const charName = (id: number) => fight.characters?.find((c) => c.id === id)?.name ?? (id ? `Personnage ${id}` : "Personnage");
  const character = $derived(ui.character !== null && characters.includes(ui.character) ? ui.character : characters.includes(levels.current) ? levels.current : (characters[0] ?? null));
  const mine = $derived(character !== null ? (levels.characters[character] ?? {}) : {});

  const job = $derived(ui.job ?? jobs.find((j) => mine[j.id])?.id ?? jobs[0]?.id ?? null);
  const key = $derived(String(job));
  const game = $derived(job !== null ? mine[job] : undefined);
  const override = $derived(ui.overrides[key]);
  const level = $derived(override?.level ?? game?.level ?? 1);
  const inLevel = $derived(override?.inLevel ?? (game ? game.xp - levelFloor(game.level) : 0));
  const startXp = $derived(levelFloor(level) + Math.max(0, inLevel));

  function setLevel(v: number) {
    const l = Math.max(1, Math.min(MAX_JOB_LEVEL, Math.round(v) || 1));
    ui.overrides[key] = { level: l, inLevel: l === level ? inLevel : 0 };
  }
  function setInLevel(v: number) {
    const cap = level < MAX_JOB_LEVEL ? levelFloor(level + 1) - levelFloor(level) - 1 : 0;
    ui.overrides[key] = { level, inLevel: Math.max(0, Math.min(cap, Math.round(v) || 0)) };
  }
  const resetLevel = () => delete ui.overrides[key];

  // « Craftable par moi »: an ingredient of one of the character's jobs (the planned one at the level typed), at its level
  const levelOfJob = (j: number) => (j === job ? level : j === BASE_JOB ? MAX_JOB_LEVEL : (mine[j]?.level ?? 0));
  const canCraft = (gid: number) => {
    const c = catalog.crafts[gid];
    return ui.craftMine && !!c && levelOfJob(c[0]) >= c[1];
  };

  const jobRecipes = $derived<JobRecipe[]>(
    job === null ? [] : Object.entries(catalog.crafts).filter(([, c]) => c[0] === job).map(([g, c]) => ({ gid: Number(g), level: c[1], ratio: c[2] })).filter((r) => recipes.recipes[r.gid]),
  );
  const excluded = $derived(new Set(ui.excluded[key] ?? []));
  const forced = $derived(new Map(Object.entries(ui.forced[key] ?? {}).map(([g, q]) => [Number(g), q])));
  const target = $derived(Math.max(1, Math.min(MAX_JOB_LEVEL, ui.target || MAX_JOB_LEVEL)));

  const ownedMap = $derived(new Map(Object.entries(owned.items).map(([g, q]) => [Number(g), q])));
  const frozen = $derived(ui.frozen[key]);
  /** Freeze the plan shown (its crafts stay; prices and owned resources still count), or let it be computed again. */
  function toggleFreeze() {
    if (frozen) {
      const f = { ...ui.frozen };
      delete f[key];
      ui.frozen = f;
    } else ui.frozen = { ...ui.frozen, [key]: { t: Date.now(), rows: p.rows.map((r) => ({ gid: r.gid, level: r.level, qty: r.qty, forced: r.forced })) } };
  }
  const bankDate = $derived(owned.bank.t ? new Date(owned.bank.t * 1000).toLocaleString("fr-FR", { dateStyle: "short", timeStyle: "short" }) : null);

  // rebuilt whenever a price changes (each price read inside is tracked)
  const result = $derived.by(() => {
    const costs = new Costs(recipes, prices.map, canCraft);
    const cost = (gid: number) => costs.crafted(gid).unit;
    const input = {
      recipes: jobRecipes, cost, startXp, target, forced, excluded, coef: ui.coef, epic: ui.epic, maxPerRecipe: ui.maxCrafts,
      owned: ui.useOwned ? ownedMap : undefined,
      ingredients: (gid: number) => recipes.recipes[gid] ?? [],
      unit: (gid: number) => costs.best(gid).unit,
    };
    const p = frozen ? reprice(frozen.rows, input) : plan(input);
    return { costs, cost, p };
  });
  const p = $derived(result.p);
  const crafts = $derived(p.rows.reduce((s, r) => s + r.qty, 0));
  /** recipes without a price, which the search can't use */
  const unpriced = $derived(jobRecipes.filter((r) => r.level <= Math.max(level, target) && result.cost(r.gid) === null).length);

  function exclude(gid: number) {
    if (forced.has(gid)) return unwish(gid);
    ui.excluded[key] = [...excluded, gid];
  }
  const restore = (gid: number) => (ui.excluded[key] = [...excluded].filter((g) => g !== gid));
  function wish(gid: number, qty: number) {
    ui.forced[key] = { ...(ui.forced[key] ?? {}), [gid]: Math.max(1, Math.round(qty) || 1) };
    if (excluded.has(gid)) restore(gid);
  }
  function unwish(gid: number) {
    const f = { ...(ui.forced[key] ?? {}) };
    delete f[gid];
    ui.forced[key] = f;
  }

  /** The resources of the plan's crafts in the floating list: in full, the window counting what the inventory and the bank
   *  hold (with « Inventaire + banque »; owned intermediates are used first). */
  function exportList() {
    const needs = new Map<number, number>();
    for (const r of p.rows) for (const [g, q] of recipes.recipes[r.gid] ?? []) needs.set(g, (needs.get(g) ?? 0) + q * r.qty);
    const title = `${catalog.jobs[key] ?? "Métier"} ${level} → ${levelOf(p.endXp)}`;
    const stock = ui.useOwned;
    void openShopList({ title, ...shopRows(result.costs, needs, name, stock ? ownedMap : undefined), stock, had: stock ? owned.items : undefined });
  }
  const num = (e: Event) => Number((e.currentTarget as HTMLInputElement).value);
</script>

<SubView title="XP métiers" {err}>
  {#snippet top()}
    <div class="form">
      {#if characters.length > 1}
        <label>Personnage
          <select value={character} onchange={(e) => (ui.character = Number(e.currentTarget.value))}>
            {#each characters as c (c)}<option value={c}>{charName(c)}</option>{/each}
          </select>
        </label>
      {/if}
      <label>Métier
        <select value={job} onchange={(e) => (ui.job = Number(e.currentTarget.value))}>
          {#each jobs as j (j.id)}<option value={j.id}>{j.name}{mine[j.id] ? ` (${mine[j.id].level})` : ""}</option>{/each}
        </select>
      </label>
      <label>Niveau <input type="number" min="1" max={MAX_JOB_LEVEL} value={level} oninput={(e) => setLevel(num(e))} /></label>
      <label>XP dans ce niveau <input type="number" min="0" value={inLevel} oninput={(e) => setInLevel(num(e))} /></label>
      {#if override}
        <button class="mini" onclick={resetLevel} title="Reprendre le niveau et l'XP lus dans le jeu">{game ? `↺ jeu : ${game.level}` : "↺ effacer"}</button>
      {:else}
        <span class="mut">{game ? "lu dans le jeu" : "connectez-vous, application ouverte, pour le lire"}</span>
      {/if}
      <label>Niveau visé <input type="number" min="1" max={MAX_JOB_LEVEL} value={ui.target} oninput={(e) => (ui.target = num(e))} /></label>
      <label title="Bonus d'XP (Almanax, événements…)">Bonus
        <select bind:value={ui.coef}>{#each XP_COEFS as c (c)}<option value={c}>{c} %</option>{/each}</select>
      </label>
      <label title="Au plus ce nombre de crafts d'une même recette dans le plan (crafts souhaités à part) : au-delà, la recette suivante la moins chère prend le relais. Acheter les ingrédients de 10 000 crafts ferait grimper leur prix.">Crafts max par recette
        <select value={ui.maxCrafts ?? ""} onchange={(e) => (ui.maxCrafts = e.currentTarget.value ? Number(e.currentTarget.value) : null)}>
          <option value="">Sans limite</option>
          {#each MAX_CRAFTS as n (n)}<option value={n}>{fmtInt(n)}</option>{/each}
        </select>
      </label>
      <label class="sw"><Switch bind:checked={ui.epic} label="Serveur épique (×3)" /> Serveur épique (×3)</label>
      <label class="sw" title={`Les ressources déjà dans votre inventaire${bankDate ? ` et votre banque (lue le ${bankDate})` : ""} sont utilisées d'abord : un craft ne coûte que ce qui manque. ${bankDate ? "La banque est relue à chaque fois que vous l'affichez à l'hôtel de vente." : "Banque jamais lue : à l'hôtel de vente, affichez votre banque (bouton de la fenêtre) pour qu'elle compte."}${owned.inventory ? "" : " Inventaire pas encore lu : ouvrez un hôtel de vente ou reconnectez-vous."}`}>
        <Switch bind:checked={ui.useOwned} label="Utiliser mon inventaire et ma banque" /> Inventaire + banque
        {#if ui.useOwned}<span class="mut small">{bankDate ? `banque du ${bankDate}` : "banque jamais lue"}</span>{/if}
      </label>
      <label class="sw" title={Object.keys(mine).length ? "Un ingrédient que vos métiers savent faire (à leur niveau) est crafté s'il revient moins cher, sinon tout est acheté" : "Vos métiers ne sont pas encore lus : connectez-vous, application ouverte"}>
        <Switch bind:checked={ui.craftMine} label="Craftable par moi" /> Craftable par moi
      </label>
    </div>
    <div class="sum">
      {#if target <= level}
        <span class="mut">Le niveau visé est déjà atteint.</span>
      {:else}
        <b>{fmtInt(Math.max(0, levelFloor(target) - startXp))}</b> XP à gagner ·
        <b>{fmtInt(crafts)}</b> craft{crafts > 1 ? "s" : ""} pour <b class="good">{kamas(Math.round(p.cost))}</b>
        · niveau atteint <b>{levelOf(p.endXp)}</b>
        {#if p.unpriced}<span class="bad">· un craft souhaité sans prix n'est pas compté</span>{/if}
        {#if frozen}<span class="mut">· liste figée (niveau, bonus et choix ne la changent plus)</span>{/if}
        {#if p.capped}<span class="mut">· {p.capped} recette{p.capped > 1 ? "s" : ""} à la limite de {fmtInt(ui.maxCrafts ?? 0)} crafts</span>{/if}
        {#if p.stuckAt !== null}<span class="bad">· {p.capped ? "plus de recette avec un prix connu sous la limite de crafts" : "aucune recette avec un prix connu"} au niveau {p.stuckAt}</span>{/if}
      {/if}
      <button class="mini right" class:on={!!frozen} disabled={!frozen && !p.rows.length} onclick={toggleFreeze}
        title={frozen ? "Liste figée : les crafts ne changent plus (les prix et les ressources possédées sont toujours pris en compte). Cliquer pour la recalculer." : "Garder ces crafts tels quels pendant les achats : seuls les prix et ce qui reste à acheter changent"}>
        {frozen ? `❄ Figée le ${new Date(frozen.t).toLocaleString("fr-FR", { dateStyle: "short", timeStyle: "short" })} · libérer` : "Figer la liste"}
      </button>
      <button class="mini" disabled={!p.rows.length} onclick={exportList} title="Toutes les ressources à acheter, en quantité exacte, dans une fenêtre au-dessus du jeu">Liste des ingrédients</button>
    </div>
  {/snippet}
  <PlanTable rows={p.rows} {name} cost={result.cost} subs={(gid) => [...result.costs.crafted(gid).craft.keys()]} onexclude={exclude} />
  {#if unpriced}
    <p class="mut small">{unpriced} recette{unpriced > 1 ? "s" : ""} de ce métier sans prix (un ingrédient jamais vu à l'hôtel de vente) : laissée{unpriced > 1 ? "s" : ""} de côté.</p>
  {/if}
  <Choices
    recipes={jobRecipes}
    {name}
    {excluded}
    {forced}
    onrestore={restore}
    onwish={wish}
    onunwish={unwish}
  />
</SubView>

<style>
  .form { display: flex; align-items: center; gap: 10px 14px; flex-wrap: wrap; margin-bottom: 8px; padding: 8px 10px; background: var(--panel); border: 1px solid var(--line); }
  .form label { display: flex; align-items: center; gap: 6px; }
  .form input { width: 80px; padding: 3px 6px; text-align: right; }
  .sum { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; margin-bottom: 10px; }
  .right { margin-left: auto; }
  .form .sw { display: inline-flex; cursor: pointer; align-items: center; gap: 6px; }
  .small { font-size: 12px; }
  .mini.on { background: color-mix(in srgb, var(--acc) 30%, transparent); }
</style>
