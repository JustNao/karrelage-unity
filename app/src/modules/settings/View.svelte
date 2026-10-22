<script lang="ts">
  import { SETTING_DEFAULTS, settings, type CombatShow } from "../../lib/stores/settings.svelte";
  import SmoothScroll from "../../lib/components/SmoothScroll.svelte";

  /** Paramètres: one section per tab (general UI settings will come first). Values live in lib/stores/settings.svelte.ts. */
  const clampZ = (v: number) => (Number.isFinite(v) ? Math.min(5, Math.max(0.5, Math.round(v * 10) / 10)) : SETTING_DEFAULTS.hdv.goodZ);
</script>

<div class="view">
  <SmoothScroll>
    <div class="settings">
      <section>
        <h2>HDV</h2>
        <div class="row">
          <div class="lb">
            <div>Seuil de bonne affaire</div>
            <div class="mut">
              Un lot est une bonne affaire (point vert, ligne verte, « Bonnes affaires ») quand il est au moins à ce nombre
              d'écarts-types (σ) au-dessus de la tendance prix / densité. Plus bas = plus de lots signalés. La bande du graphique suit ce seuil.
            </div>
          </div>
          <div class="ctl">
            <input
              type="number"
              min="0.5"
              max="5"
              step="0.1"
              value={settings.hdv.goodZ}
              onchange={(e) => (settings.hdv.goodZ = clampZ(e.currentTarget.valueAsNumber))}
            />
            <span class="mut">σ</span>
            <button class="mini" disabled={settings.hdv.goodZ === SETTING_DEFAULTS.hdv.goodZ} onclick={() => (settings.hdv.goodZ = SETTING_DEFAULTS.hdv.goodZ)} title="Revenir à la valeur par défaut ({SETTING_DEFAULTS.hdv.goodZ} σ)">Par défaut</button>
          </div>
        </div>
      </section>
      <section>
        <h2>Combat</h2>
        <div class="row">
          <div class="lb">
            <div>Suivre les combats</div>
            <div class="mut">Détecte les monstres du combat en cours (paquets du placement et des tours) et affiche leurs remarques dans l'onglet Combat.</div>
          </div>
          <div class="ctl">
            <button class="sw" class:on={settings.combat.enabled} role="switch" aria-checked={settings.combat.enabled} aria-label="Suivre les combats" onclick={() => (settings.combat.enabled = !settings.combat.enabled)}></button>
          </div>
        </div>
        <div class="row">
          <div class="lb">
            <div>Informations affichées</div>
            <div class="mut">Avertissements seuls, avec les invulnérabilités, ou avec aussi les conseils.</div>
          </div>
          <div class="ctl">
            <select value={settings.combat.show} onchange={(e) => (settings.combat.show = e.currentTarget.value as CombatShow)}>
              <option value="warning">Avertissements</option>
              <option value="invulnerable">Avertissements + invulnérabilités</option>
              <option value="all">Avertissements + invulnérabilités + conseils</option>
            </select>
          </div>
        </div>
        <div class="row">
          <div class="lb">
            <div>Ouvrir l'onglet au début d'un combat</div>
            <div class="mut">Affiche l'onglet Combat dès qu'un combat commence.</div>
          </div>
          <div class="ctl"><input type="checkbox" bind:checked={settings.combat.autoOpen} /></div>
        </div>
        <div class="row">
          <div class="lb">
            <div>Fenêtre flottante au début d'un combat</div>
            <div class="mut">Ouvre la petite fenêtre Combat, toujours au premier plan (sauf jeu en plein écran exclusif), dès qu'un combat commence.</div>
          </div>
          <div class="ctl"><input type="checkbox" bind:checked={settings.combat.overlayAuto} /></div>
        </div>
      </section>
    </div>
  </SmoothScroll>
</div>

<style>
  .view { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .view > :global(.wrap) { flex: 1; }
  .settings { max-width: 760px; font-family: var(--font); font-size: 13px; }
  section { margin-bottom: 20px; }
  h2 { font: 600 12px var(--font); letter-spacing: 0.08em; text-transform: uppercase; color: var(--mut); margin: 4px 0 8px; padding-bottom: 4px; border-bottom: 1px solid var(--line); }
  .row { display: flex; gap: 20px; align-items: flex-start; justify-content: space-between; padding: 8px 0; }
  .lb .mut { font-size: 12px; margin-top: 2px; }
  .ctl { display: flex; gap: 6px; align-items: center; flex: none; }
  .ctl input[type="number"] { width: 64px; }
  .sw { position: relative; width: 44px; height: 24px; padding: 0; background: var(--line); border-radius: 12px; transition: 0.15s; flex: none; border: 0; }
  .sw::after { content: ""; position: absolute; top: 3px; left: 3px; width: 18px; height: 18px; border-radius: 50%; background: #fff; transition: 0.15s; }
  .sw.on { background: var(--ok); }
  .sw.on::after { left: 23px; }
</style>
