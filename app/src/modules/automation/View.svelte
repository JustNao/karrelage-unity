<script lang="ts">
  import { RULE_DEFAULTS, RULE_TITLES, rules, type RuleBase } from "../../lib/stores/automation.svelte";
  import { playSound, SOUNDS } from "../../lib/sounds";
  import SmoothScroll from "../../lib/components/SmoothScroll.svelte";
  import Switch from "../../lib/components/Switch.svelte";
  import { kamasFull } from "../../lib/fmt";
  import MonsterSearch from "./MonsterSearch.svelte";

  /** Automation: rules checked on every scan, fight end or map change (lib/automation.ts), one section per tab, like Paramètres. A triggered rule
   *  plays its sound and shows up in the header « Notifications ». */

  /** "2,500,000", "2.5M", "500k" -> kamas; empty or unreadable -> null (no limit). */
  function parsePrice(s: string): number | null {
    const m = s.replace(/[\s,]/g, "").match(/^(\d+(?:\.\d+)?)([kKmM]?)$/);
    if (!m) return null;
    const v = parseFloat(m[1]) * (m[2] === "" ? 1 : m[2].toLowerCase() === "k" ? 1e3 : 1e6);
    return v > 0 ? Math.round(v) : null;
  }
  const priceText = (v: number | null) => (v != null ? kamasFull(v).replace(/\s*K$/, "") : "");
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
      <section>
        <h2>HDV</h2>
        <div class="rule" class:off={!rules.exoDo.enabled}>
          <div class="rh">
            <Switch bind:checked={rules.exoDo.enabled} label="Activer la règle" />
            <div class="lb">
              <div><b>{RULE_TITLES.exoDo}</b></div>
              <div class="mut">
                À chaque objet scanné à l'hôtel de vente : un lot avec un exo « % dommages » (sorts, armes, distance, mêlée) moins cher
                que la rune de transcendance de cette stat (prix enregistré), et dont le brisage rembourse le prix à partir d'un
                coefficient au plus égal au seuil (« Coef. rentable » de l'onglet Brisage), sans dépasser le prix max s'il est fixé. Un même lot ne sonne qu'une fois par session.
              </div>
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
      </section>
      <section>
        <h2>Songes</h2>
        <div class="rule" class:off={!rules.songes.enabled}>
          <div class="rh">
            <Switch bind:checked={rules.songes.enabled} label="Activer la règle" />
            <div class="lb">
              <div><b>{RULE_TITLES.songes}</b></div>
              <div class="mut">
                À la fin d'un combat, si quelqu'un du groupe a obtenu une « Légende … » (Ressource des Songes) : un son selon le prix
                enregistré de la meilleure, au-dessus ou en dessous du seuil (prix inconnu = en dessous), et une notification par Légende.
              </div>
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
        <div class="rule" class:off={!rules.goblins.enabled}>
          <div class="rh">
            <Switch bind:checked={rules.goblins.enabled} label="Activer la règle" />
            <div class="lb">
              <div><b>{RULE_TITLES.goblins}</b></div>
              <div class="mut">
                En entrant dans une salle, si un gobelin des Songes s'y trouve (King Gob, Gobséric, Gobledore ou Bog) : un son par
                salle et une notification par gobelin.
              </div>
            </div>
          </div>
          <div class="params">
            {@render sound(rules.goblins)}
          </div>
        </div>
      </section>
      <section>
        <h2>Monstres</h2>
        <div class="rule" class:off={!rules.monsters.enabled}>
          <div class="rh">
            <Switch bind:checked={rules.monsters.enabled} label="Activer la règle" />
            <div class="lb">
              <div><b>{RULE_TITLES.monsters}</b></div>
              <div class="mut">
                À chaque changement de carte (et quand un groupe apparaît sur la carte), si un des monstres recherchés est dans un
                groupe : un seul son, même si plusieurs correspondent, et une notification par monstre.
              </div>
            </div>
          </div>
          <div class="params">
            <MonsterSearch bind:gids={rules.monsters.gids} />
            {@render sound(rules.monsters)}
          </div>
        </div>
      </section>
    </div>
  </SmoothScroll>
</div>

<style>
  .view { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .view > :global(.wrap) { flex: 1; }
  .auto { max-width: 760px; font-family: var(--font); font-size: 13px; }
  section { margin-bottom: 20px; }
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
