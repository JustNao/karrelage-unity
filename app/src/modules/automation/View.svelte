<script lang="ts">
  import { RULE_DEFAULTS, RULE_TITLES, rules, type RuleBase } from "../../lib/stores/automation.svelte";
  import { playSound, SOUNDS } from "../../lib/sounds";
  import SmoothScroll from "../../lib/components/SmoothScroll.svelte";
  import Switch from "../../lib/components/Switch.svelte";
  import { kamasFull } from "../../lib/fmt";
  import { settings } from "../../lib/stores/settings.svelte";
  import SearchBar, { matcher } from "../../lib/components/SearchBar.svelte";
  import MonsterSearch from "./MonsterSearch.svelte";
  import { onStored, readJSON, removeKey } from "../../lib/persist.svelte";
  import { CALIBRATION_KEY, type FmCalibration } from "../../lib/forgemagie";

  /** Automation: rules checked on every scan, fight end or map change (lib/automation.ts), one section per tab, like Paramètres
   *  (the fight ones are in the Combat tab: combat/Automation.svelte). A triggered rule plays its sound and shows up in the
   *  header « Notifications ». */

  /** "2,500,000", "2.5M", "500k" -> kamas; empty or unreadable -> null (no limit). */
  function parsePrice(s: string): number | null {
    const m = s.replace(/[\s,]/g, "").match(/^(\d+(?:\.\d+)?)([kKmM]?)$/);
    if (!m) return null;
    const v = parseFloat(m[1]) * (m[2] === "" ? 1 : m[2].toLowerCase() === "k" ? 1e3 : 1e6);
    return v > 0 ? Math.round(v) : null;
  }
  const priceText = (v: number | null) => (v != null ? kamasFull(v).replace(/\s*K$/, "") : "");
  /** Each rule's description, also searched. */
  const DESC = {
    songes: "À la fin d'un combat, si quelqu'un du groupe a obtenu une « Légende … » (Ressource des Songes) : un son selon le prix enregistré de la meilleure, au-dessus ou en dessous du seuil (prix inconnu = en dessous), et une notification par Légende.",
    goblins: "En entrant dans une salle, si un gobelin des Songes s'y trouve (King Gob, Gobséric, Gobledore ou Bog) : un son par gobelin, l'un après l'autre, et une notification par gobelin.",
    monsters: "À chaque changement de carte (et quand un groupe apparaît sur la carte), si un des monstres recherchés est dans un groupe : un seul son, même si plusieurs correspondent, et une notification par monstre.",
    forgemagie: "Quand un équipement est posé dans un atelier de forgemagie : un bouton « Avancé » au bout de la ligne « Fusionner tout / Fusionner » affiche sous chaque rune ses chances, au choix au-dessus du bouton : succès critique, neutre ou échec critique (modèle établi par les joueurs de dofocus.fr, pas la formule officielle). Le premier clic lance une calibration : deux clics sur les coins de la fenêtre de forgemagie (clic droit sur le bouton pour la refaire si vous la déplacez).",
    exoDo: "À chaque objet scanné à l'hôtel de vente : un lot avec un exo « % dommages » (sorts, armes, distance, mêlée) moins cher que la rune de transcendance de cette stat (prix enregistré), et dont le brisage rembourse le prix à partir d'un coefficient au plus égal au seuil (« Coef. rentable » de l'onglet Brisage), sans dépasser le prix max s'il est fixé. Un même lot ne sonne qu'une fois par session.",
  };
  const FORGE = "Chances des runes en forgemagie";
  /** Searched text (session only): a rule shows when its section, title or description matches. */
  let q = $state("");
  const SECTION = { songes: "Songes", goblins: "Songes", monsters: "Monstres", exoDo: "HDV", forgemagie: "Forgemagie" } as const;
  const shown = $derived.by(() => {
    const m = matcher(q);
    const title = (k: keyof typeof DESC) => (k === "forgemagie" ? FORGE : RULE_TITLES[k]);
    return Object.fromEntries((Object.keys(DESC) as (keyof typeof DESC)[]).map((k) => [k, m(SECTION[k], title(k), DESC[k])])) as Record<keyof typeof DESC, boolean>;
  });
  /** The forgemagie overlay's calibration (taken in its window): reset here. */
  let fmCalibration = $state(readJSON<FmCalibration | null>(CALIBRATION_KEY, null));
  $effect(() => onStored<FmCalibration>(CALIBRATION_KEY, (c) => (fmCalibration = c)));
  function resetCalibration() {
    removeKey(CALIBRATION_KEY);
    fmCalibration = null;
  }
  const clampCoef = (v: number) => (Number.isFinite(v) ? Math.max(0, Math.round(v)) : RULE_DEFAULTS.exoDo.maxEven);
</script>

{#snippet pick(r: Record<string, any>, key: string, title: string)}
  <select value={r[key]} onchange={(e) => (r[key] = e.currentTarget.value)} {title}>
    {#each SOUNDS as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
  </select>
  <button class="mini" title="Écouter" onclick={() => playSound(r[key], r.volume)}>▶</button>
{/snippet}

{#snippet volume(r: RuleBase)}
  <input type="range" min="0" max="1" step="0.05" value={r.volume} oninput={(e) => (r.volume = e.currentTarget.valueAsNumber)} title="Volume" />
  <span class="mut vol">{Math.round(r.volume * 100)} %</span>
{/snippet}

{#snippet sound(r: RuleBase)}
  <div class="snd">
    {@render pick(r, "sound", "Son joué quand la règle se déclenche")}
    {@render volume(r)}
  </div>
{/snippet}

<div class="view">
  <SmoothScroll>
    <div class="auto">
      <div class="search"><SearchBar bind:value={q} placeholder="Rechercher une règle…" grow /></div>
      {#if shown.songes || shown.goblins}
        <section>
          <h2>Songes</h2>
        {#if shown.songes}
        <div class="rule" class:off={!rules.songes.enabled}>
          <div class="rh">
            <Switch bind:checked={rules.songes.enabled} label="Activer la règle" />
            <div class="lb">
              <div><b>{RULE_TITLES.songes}</b></div>
              <div class="mut">{DESC.songes}</div>
            </div>
          </div>
          <div class="params">
            <label title="Prix enregistré (onglet Prix enregistrés) à partir duquel la Légende est une bonne nouvelle">
              Seuil
              <input class="price" type="text" inputmode="numeric" value={priceText(rules.songes.threshold)} onchange={(e) => (e.currentTarget.value = priceText((rules.songes.threshold = parsePrice(e.currentTarget.value) ?? RULE_DEFAULTS.songes.threshold)))} />
              <span class="mut">K</span>
            </label>
            <div class="snd">
              <span class="mut">Au-dessus</span>
              {@render pick(rules.songes, "sound", "Son joué quand la Légende vaut au moins le seuil")}
            </div>
            <div class="snd">
              <span class="mut">En dessous</span>
              {@render pick(rules.songes, "soundLow", "Son joué quand la Légende vaut moins que le seuil")}
            </div>
            <div class="snd">{@render volume(rules.songes)}</div>
          </div>
        </div>
        {/if}
        {#if shown.goblins}
        <div class="rule" class:off={!rules.goblins.enabled}>
          <div class="rh">
            <Switch bind:checked={rules.goblins.enabled} label="Activer la règle" />
            <div class="lb">
              <div><b>{RULE_TITLES.goblins}</b></div>
              <div class="mut">{DESC.goblins}</div>
            </div>
          </div>
          <div class="params">
            {@render sound(rules.goblins)}
          </div>
        </div>
        {/if}
        </section>
      {/if}
      {#if shown.monsters}
        <section>
          <h2>Monstres</h2>
        {#if shown.monsters}
        <div class="rule" class:off={!rules.monsters.enabled}>
          <div class="rh">
            <Switch bind:checked={rules.monsters.enabled} label="Activer la règle" />
            <div class="lb">
              <div><b>{RULE_TITLES.monsters}</b></div>
              <div class="mut">{DESC.monsters}</div>
            </div>
          </div>
          <div class="params">
            <MonsterSearch bind:gids={rules.monsters.gids} />
            {@render sound(rules.monsters)}
          </div>
        </div>
        {/if}
        </section>
      {/if}
      {#if shown.exoDo}
        <section>
          <h2>HDV</h2>
        {#if shown.exoDo}
        <div class="rule" class:off={!rules.exoDo.enabled}>
          <div class="rh">
            <Switch bind:checked={rules.exoDo.enabled} label="Activer la règle" />
            <div class="lb">
              <div><b>{RULE_TITLES.exoDo}</b></div>
              <div class="mut">{DESC.exoDo}</div>
            </div>
          </div>
          <div class="params">
            <label title="Coefficient de brisage à partir duquel le lot est rentable (meilleur résultat, avec ou sans focus)">
              Coef. rentable max
              <input type="number" min="0" step="10" value={rules.exoDo.maxEven} onchange={(e) => (rules.exoDo.maxEven = clampCoef(e.currentTarget.valueAsNumber))} />
              <span class="mut">%</span>
            </label>
            <label title="Prix du lot au-delà duquel la règle ne se déclenche pas (vide = sans limite)">
              Prix max
              <input class="price" type="text" inputmode="numeric" placeholder="sans limite" value={priceText(rules.exoDo.maxPrice)} onchange={(e) => (e.currentTarget.value = priceText((rules.exoDo.maxPrice = parsePrice(e.currentTarget.value))))} />
              <span class="mut">K</span>
            </label>
            {@render sound(rules.exoDo)}
          </div>
        </div>
        {/if}
        </section>
      {/if}
      {#if shown.forgemagie}
        <section>
          <h2>Forgemagie</h2>
          <div class="rule" class:off={!settings.forgemagie.enabled}>
            <div class="rh">
              <Switch bind:checked={settings.forgemagie.enabled} label={FORGE} />
              <div class="lb">
                <div><b>{FORGE}</b></div>
                <div class="mut">{DESC.forgemagie}</div>
              </div>
            </div>
            <div class="params">
              <button class="mini" disabled={!fmCalibration} onclick={resetCalibration} title="La prochaine fois, le bouton se place en haut de l'écran du jeu et relance la calibration">Réinitialiser la calibration</button>
              <span class="mut">{fmCalibration ? `Calibrée pour une fenêtre de jeu de ${fmCalibration.width} × ${fmCalibration.height}` : "Pas encore calibrée"}</span>
            </div>
          </div>
        </section>
      {/if}
      {#if !Object.values(shown).some(Boolean)}<p class="mut">Aucune règle ne correspond à « {q} ».</p>{/if}
    </div>
  </SmoothScroll>
</div>

<style>
  .view { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .view > :global(.wrap) { flex: 1; }
  .auto { max-width: 760px; font-family: var(--font); font-size: 13px; }
  section { margin-bottom: 20px; }
  .search { margin-bottom: 14px; display: flex; }
  h2 { font: 600 12px var(--font); letter-spacing: 0.08em; text-transform: uppercase; color: var(--mut); margin: 4px 0 8px; padding-bottom: 4px; border-bottom: 1px solid var(--line); }
  .rule { padding: 8px 0; display: flex; flex-direction: column; gap: 8px; }
  .rule.off .params { opacity: 0.5; }
  .rh { display: flex; gap: 12px; align-items: flex-start; }
  .lb .mut { font-size: 12px; margin-top: 2px; }
  .params { display: flex; flex-wrap: wrap; gap: 8px 18px; align-items: center; padding-left: 56px; }
  .params label { display: inline-flex; align-items: center; gap: 6px; }
  .params input[type="number"] { width: 70px; }
  .params input.price { width: 110px; text-align: right; }
  .snd { display: inline-flex; align-items: center; gap: 6px; }
  .snd input[type="range"] { width: 110px; }
  .vol { min-width: 38px; }
</style>
