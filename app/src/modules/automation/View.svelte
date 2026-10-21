<script lang="ts">
  import { RULE_DEFAULTS, RULE_TITLES, rules, type RuleBase } from "../../lib/stores/automation.svelte";
  import { playSound, SOUNDS } from "../../lib/sounds";
  import SmoothScroll from "../../lib/components/SmoothScroll.svelte";

  /** Automation: rules checked on every scan (lib/automation.ts), one section per tab, like Paramètres. A triggered rule
   *  plays its sound and shows up in the header « Notifications ». */
  const clampCoef = (v: number) => (Number.isFinite(v) ? Math.max(0, Math.round(v)) : RULE_DEFAULTS.exoDo.maxEven);
</script>

{#snippet sound(r: RuleBase)}
  <div class="snd">
    <select value={r.sound} onchange={(e) => (r.sound = e.currentTarget.value)} title="Son joué quand la règle se déclenche">
      {#each SOUNDS as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
    </select>
    <button class="mini" title="Écouter" onclick={() => playSound(r.sound, r.volume)}>▶</button>
    <input type="range" min="0" max="1" step="0.05" value={r.volume} oninput={(e) => (r.volume = e.currentTarget.valueAsNumber)} title="Volume" />
    <span class="mut vol">{Math.round(r.volume * 100)} %</span>
  </div>
{/snippet}

<div class="view">
  <SmoothScroll>
    <div class="auto">
      <section>
        <h2>HDV</h2>
        <div class="rule" class:off={!rules.exoDo.enabled}>
          <div class="rh">
            <button class="sw" class:on={rules.exoDo.enabled} role="switch" aria-checked={rules.exoDo.enabled} aria-label="Activer la règle" onclick={() => (rules.exoDo.enabled = !rules.exoDo.enabled)}></button>
            <div class="lb">
              <div><b>{RULE_TITLES.exoDo}</b></div>
              <div class="mut">
                À chaque objet scanné à l'hôtel de vente : un lot avec un exo « % dommages » (sorts, armes, distance, mêlée) moins cher
                que la rune de transcendance de cette stat (prix enregistré), et dont le brisage rembourse le prix à partir d'un
                coefficient au plus égal au seuil (« Coef. rentable » de l'onglet Brisage). Un même lot ne sonne qu'une fois par session.
              </div>
            </div>
          </div>
          <div class="params">
            <label title="Coefficient de brisage à partir duquel le lot est rentable (meilleur résultat, avec ou sans focus)">
              Coef. rentable max
              <input type="number" min="0" step="10" value={rules.exoDo.maxEven} onchange={(e) => (rules.exoDo.maxEven = clampCoef(e.currentTarget.valueAsNumber))} />
              <span class="mut">%</span>
            </label>
            {@render sound(rules.exoDo)}
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
  .snd { display: inline-flex; align-items: center; gap: 6px; }
  .snd input[type="range"] { width: 110px; }
  .vol { min-width: 38px; }
  .sw { position: relative; width: 44px; height: 24px; padding: 0; background: var(--line); border-radius: 12px; transition: 0.15s; flex: none; border: 0; }
  .sw::after { content: ""; position: absolute; top: 3px; left: 3px; width: 18px; height: 18px; border-radius: 50%; background: #fff; transition: 0.15s; }
  .sw.on { background: var(--ok); }
  .sw.on::after { left: 23px; }
</style>
