<script lang="ts">
  import { settings } from "../../lib/stores/settings.svelte";
  import { SHORTCUTS, sameBinding } from "../../lib/shortcuts";
  import Switch from "../../lib/components/Switch.svelte";
  import KeyCapture from "../../lib/components/KeyCapture.svelte";
  import Banner from "../../lib/components/Banner.svelte";

  /** Combat tab « Général »: the options that are about no class. */
  const help = $derived(settings.shortcuts.classHelp);
  const key = $derived(settings.shortcuts.spellCell);
  const clash = $derived(key.vk ? SHORTCUTS.find((s) => s.name !== "spellCell" && sameBinding(settings.shortcuts[s.name], key)) : undefined);
</script>

<div class="head">
  <h2>Général</h2>
</div>

<div class="row">
  <div class="lb">
    <div>Position des sorts contre le Comte Harebourg</div>
    <div class="mut">
      Ses sorts font tourner les vôtres autour de vous : en bleu, sur le jeu, la case où arrive un sort visant la case sous la souris.
    </div>
  </div>
  <div class="ctl"><Switch bind:checked={settings.combat.harebourgCell} label="Position des sorts contre le Comte Harebourg" /></div>
</div>
<div class="row sub" class:off={!settings.combat.harebourgCell}>
  <div class="lb">
    <div>Touche dédiée</div>
    <div class="mut">
      {#if key.vk}La case s'affiche tant que <b>{key.label}</b> est enfoncée, et plus avec l'aide de classe.
      {:else}Aucune : la case s'affiche avec l'aide de classe{#if help.vk} (<b>{help.label}</b>){/if}.{/if}
    </div>
    {#if clash}<Banner tone="warn" inset>Même touche que « {clash.title} » : un seul des deux raccourcis fonctionnera.</Banner>{/if}
  </div>
  <div class="ctl">
    <KeyCapture label="Touche de la position des sorts" value={key} onchange={(b) => (settings.shortcuts.spellCell = b)} />
  </div>
</div>

<style>
  .head { display: flex; align-items: center; gap: 16px; padding-bottom: 4px; border-bottom: 1px solid var(--line); margin-top: 24px; }
  h2 { font: 600 12px var(--font); letter-spacing: 0.08em; text-transform: uppercase; color: var(--mut); margin: 0; }
  .row { display: flex; align-items: center; gap: 16px; padding: 10px 0; border-bottom: 1px solid color-mix(in srgb, var(--line) 50%, transparent); }
  .row.sub { padding-left: 16px; }
  .row.off { opacity: 0.5; }
  .lb { flex: 1; min-width: 0; }
  .lb .mut { font-size: 12px; margin-top: 2px; }
  .ctl { flex-shrink: 0; }
</style>
