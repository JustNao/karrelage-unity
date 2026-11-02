<script lang="ts">
  import { SETTING_DEFAULTS, settings } from "../../lib/stores/settings.svelte";
  import SmoothScroll from "../../lib/components/SmoothScroll.svelte";
  import Switch from "../../lib/components/Switch.svelte";
  import KeyCapture from "../../lib/components/KeyCapture.svelte";
  import Banner from "../../lib/components/Banner.svelte";
  import { SHORTCUTS, sameBinding, type ShortcutName } from "../../lib/shortcuts";
  import { taken } from "../../lib/stores/shortcuts.svelte";

  /** Paramètres: general settings first, then one section per tab. Values live in lib/stores/settings.svelte.ts. */
  const isDefault = (n: ShortcutName) => sameBinding(settings.shortcuts[n], SETTING_DEFAULTS.shortcuts[n]);
  const resetKey = (n: ShortcutName) => (settings.shortcuts[n] = { ...SETTING_DEFAULTS.shortcuts[n] });
  const resetKeys = () => SHORTCUTS.forEach((s) => resetKey(s.name));
  /** Another shortcut with the same combination (only one of them can work). */
  const clash = (n: ShortcutName) => {
    const b = settings.shortcuts[n];
    return b.vk ? SHORTCUTS.find((s) => s.name !== n && sameBinding(settings.shortcuts[s.name], b)) : undefined;
  };
  const clampZ =(v: number) => (Number.isFinite(v) ? Math.min(5, Math.max(0.5, Math.round(v * 10) / 10)) : SETTING_DEFAULTS.hdv.goodZ);
</script>

<div class="view">
  <SmoothScroll>
    <div class="settings">
      <section>
        <h2>Général</h2>
        <div class="row">
          <div class="lb">
            <div>Garder en arrière-plan à la fermeture</div>
            <div class="mut">
              Fermer la fenêtre laisse Karrelage tourner dans la zone de notification (la capture et les fenêtres flottantes continuent).
              Clic sur son icône pour la rouvrir, clic droit pour « Ouvrir » ou « Quitter ».
            </div>
          </div>
          <div class="ctl">
            <Switch bind:checked={settings.general.closeToTray} label="Garder en arrière-plan à la fermeture" />
          </div>
        </div>
      </section>
      <section>
        <h2 class="with-action">
          Raccourcis
          <button class="mini" disabled={SHORTCUTS.every((s) => isDefault(s.name))} onclick={resetKeys} title="Remettre tous les raccourcis par défaut">Tout réinitialiser</button>
        </h2>
        <p class="mut intro">Raccourcis globaux : ils marchent quand le jeu est au premier plan (le jeu ne reçoit alors pas ces touches).</p>
        {#each SHORTCUTS as s (s.name)}
          {@const other = clash(s.name)}
          <div class="row">
            <div class="lb">
              <div>{s.title}</div>
              <div class="mut">{s.desc}</div>
              {#if taken.get(s.name) && settings.shortcuts[s.name].vk}
                <Banner tone="warn" inset>{settings.shortcuts[s.name].label} est déjà pris par un autre programme : choisissez une autre combinaison.</Banner>
              {:else if other}
                <Banner tone="warn" inset>Même combinaison que « {other.title} ».</Banner>
              {/if}
            </div>
            <div class="ctl">
              <KeyCapture label={s.title} value={settings.shortcuts[s.name]} onchange={(b) => (settings.shortcuts[s.name] = b)} />
              <button class="mini" disabled={isDefault(s.name)} onclick={() => resetKey(s.name)} title="Revenir au raccourci par défaut ({SETTING_DEFAULTS.shortcuts[s.name].label})">Par défaut</button>
            </div>
          </div>
        {/each}
      </section>
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
    </div>
  </SmoothScroll>
</div>

<style>
  .view { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .view > :global(.wrap) { flex: 1; }
  .settings { max-width: 760px; font-family: var(--font); font-size: 13px; }
  section { margin-bottom: 20px; }
  h2 { font: 600 12px var(--font); letter-spacing: 0.08em; text-transform: uppercase; color: var(--mut); margin: 4px 0 8px; padding-bottom: 4px; border-bottom: 1px solid var(--line); }
  h2.with-action { display: flex; align-items: center; justify-content: space-between; }
  h2 .mini { text-transform: none; letter-spacing: normal; font-weight: 400; }
  .intro { font-size: 12px; margin: 0 0 4px; }
  .lb :global(.banner) { margin-top: 6px; }
  .row { display: flex; gap: 20px; align-items: flex-start; justify-content: space-between; padding: 8px 0; }
  .lb .mut { font-size: 12px; margin-top: 2px; }
  .ctl { display: flex; gap: 6px; align-items: center; flex: none; }
  .ctl input[type="number"] { width: 64px; }
</style>
