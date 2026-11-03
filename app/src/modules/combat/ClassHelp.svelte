<script lang="ts">
  import { settings } from "../../lib/stores/settings.svelte";
  import { SHORTCUTS, sameBinding } from "../../lib/shortcuts";
  import { PORTAL_BONUS, TURRET_RANGE } from "../../lib/classHelp";
  import Switch from "../../lib/components/Switch.svelte";
  import KeyCapture from "../../lib/components/KeyCapture.svelte";
  import Banner from "../../lib/components/Banner.svelte";

  /** Combat tab « Aide de classe »: what the in-game class help shows, per class (`settings.classHelp`). */
  const help = $derived(settings.classHelp);
  const view = $derived(settings.shortcuts.classHelp.vk ? settings.shortcuts.classHelp.label : null);
  /** a global shortcut on the same keys takes them first: the portal key never reaches the game nor the simulation */
  const clash = $derived(help.eliotrope.portalKey.vk ? SHORTCUTS.find((s) => sameBinding(settings.shortcuts[s.name], help.eliotrope.portalKey)) : undefined);
</script>

<div class="head">
  <h2>Aide de classe</h2>
</div>
<p class="mut help">
  Affichée sur le jeu pendant un combat{#if view}, tant que <b>{view}</b> est enfoncée{/if} (modifiable dans Paramètres › Raccourcis).
</p>

<div class="row">
  <div class="lb">
    <div>Steamer</div>
    <div class="mut">La zone de chaque tourelle ({TURRET_RANGE} PO et en ligne de vue) où votre personnage reçoit son bonus.</div>
  </div>
  <div class="ctl"><Switch bind:checked={help.steamer.enabled} label="Aide du Steamer" /></div>
</div>

<div class="row">
  <div class="lb">
    <div>Éliotrope</div>
    <div class="mut">
      Vos portails numérotés du plus ancien au plus récent ; avec 4 portails, le n° 1 (rouge) part au prochain portail posé et
      le n° 2 (orange) ensuite. Un sort passant par les portails gagne {PORTAL_BONUS} % de dommages par case parcourue entre eux.
    </div>
  </div>
  <div class="ctl"><Switch bind:checked={help.eliotrope.enabled} label="Aide de l'Éliotrope" /></div>
</div>
<div class="row sub" class:off={!help.eliotrope.enabled}>
  <div class="lb">
    <div>Touche du sort Portail</div>
    <div class="mut">
      Votre raccourci du sort Portail dans le jeu : le jeu la reçoit toujours, et un portail est simulé sur la case sous la
      souris (sur un de vos portails, il le remplace ; sinon le plus ancien est barré s'il y en a 4), avec la redirection jusqu'à la sortie et son bonus de dommages. La touche à
      nouveau, un clic (le sort est lancé ou annulé) ou Échap l'effacent.
    </div>
    {#if clash}<Banner tone="warn" inset>Même touche que « {clash.title} » : ce raccourci la prend, le jeu ne la reçoit pas.</Banner>{/if}
  </div>
  <div class="ctl">
    <KeyCapture label="Touche du sort Portail" value={help.eliotrope.portalKey} onchange={(b) => (help.eliotrope.portalKey = b)} />
  </div>
</div>

<style>
  .head { display: flex; align-items: center; gap: 16px; padding-bottom: 4px; border-bottom: 1px solid var(--line); margin-top: 24px; }
  h2 { font: 600 12px var(--font); letter-spacing: 0.08em; text-transform: uppercase; color: var(--mut); margin: 0; }
  .help { font-size: 12px; margin: 8px 0 4px; }
  .row { display: flex; align-items: center; gap: 16px; padding: 10px 0; border-bottom: 1px solid color-mix(in srgb, var(--line) 50%, transparent); }
  .row.sub { padding-left: 16px; }
  .row.off { opacity: 0.5; }
  .lb { flex: 1; min-width: 0; }
  .lb .mut { font-size: 12px; margin-top: 2px; }
  .ctl { flex-shrink: 0; }
</style>
