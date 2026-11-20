<script lang="ts">
  import { settings } from "../../lib/stores/settings.svelte";
  import { NO_BINDING, SHORTCUTS, sameBinding } from "../../lib/shortcuts";
  import Switch from "../../lib/components/Switch.svelte";
  import Segmented from "../../lib/components/Segmented.svelte";
  import KeyCapture from "../../lib/components/KeyCapture.svelte";
  import Banner from "../../lib/components/Banner.svelte";

  /** Combat tab « Général »: the options that are about no class. */
  const help = $derived(settings.shortcuts.classHelp);
  const key = $derived(settings.shortcuts.bossHelp);
  const clash = $derived(key.vk ? SHORTCUTS.find((s) => s.name !== "bossHelp" && sameBinding(settings.shortcuts[s.name], key)) : undefined);
</script>

<div class="head">
  <h2>Général</h2>
</div>

<div class="row">
  <div class="lb">
    <div>Élément le plus faible des ennemis</div>
    <div class="mut">
      Avec la ligne de vue en jeu{#if settings.shortcuts.los.vk}{" "}(<b>{settings.shortcuts.los.label}</b>){/if} : sur chaque ennemi, l'élément
      où sa résistance en % est la plus basse (dans sa couleur), la valeur en dessous. Suit les buffs et débuffs du combat.
    </div>
  </div>
  <div class="ctl"><Switch bind:checked={settings.combat.weakElement} label="Élément le plus faible des ennemis" /></div>
</div>
<div class="row sub" class:off={!settings.combat.weakElement}>
  <div class="lb">
    <div>Élément affiché</div>
    <div class="mut">
      « Mon build » : l'élément qui fait le plus de dégâts à chaque ennemi avec vos caractéristiques, puissance et dommages
      (fixes et élémentaires), contre ses résistances en % et fixes, pour un sort de 30 de base. Suit les
      changements d'équipement et les buffs du combat (personnage dont c'est le tour). Tant que vos caractéristiques ne sont pas
      connues : la résistance la plus basse.
    </div>
  </div>
  <div class="ctl">
    <Segmented
      bind:value={settings.combat.weakElementMode}
      label="Élément affiché"
      options={[
        { id: "resist", label: "Résistance la plus basse" },
        { id: "damage", label: "Mon build" },
      ]}
    />
  </div>
</div>
<div class="row sub" class:off={!settings.combat.weakElement}>
  <div class="lb">
    <div>Toujours affiché</div>
    <div class="mut">Pendant tout le combat, sans appuyer sur la touche (tant que le jeu est au premier plan).</div>
  </div>
  <div class="ctl"><Switch bind:checked={settings.combat.weakElementAlways} label="Élément le plus faible toujours affiché" /></div>
</div>
<div class="row">
  <div class="lb">
    <div>Aide contre les boss</div>
    <div class="mut">
      Sur le jeu, pendant le combat d'un de ces boss.
      {#if key.vk}Tant que <b>{key.label}</b> est enfoncée (plus avec l'aide de classe).
      {:else}Avec l'aide de classe{#if help.vk}{" "}(<b>{help.label}</b>){/if}, ou une touche dédiée.{/if}
    </div>
    {#if clash}<Banner tone="warn" inset>Même touche que « {clash.title} » : un seul des deux raccourcis fonctionnera.</Banner>{/if}
  </div>
  <div class="ctl">
    {#if key.vk}<button class="mini" title="Retirer cette touche : l'aide s'affiche à nouveau avec l'aide de classe" onclick={() => (settings.shortcuts.bossHelp = { ...NO_BINDING })}>Retirer</button>{/if}
    <KeyCapture label="Touche de l'aide contre les boss" value={key} onchange={(b) => (settings.shortcuts.bossHelp = b)} />
  </div>
</div>
<div class="row sub" class:off={!settings.combat.harebourgCell}>
  <div class="lb">
    <div>Comte Harebourg : position des sorts</div>
    <div class="mut">Ses sorts font tourner les vôtres autour de vous : en bleu, la case où arrive un sort visant la case sous la souris.</div>
  </div>
  <div class="ctl"><Switch bind:checked={settings.combat.harebourgCell} label="Position des sorts contre le Comte Harebourg" /></div>
</div>
<div class="row sub" class:off={!settings.combat.guerreWeapon}>
  <div class="lb">
    <div>Guerre : prochaine arme géante</div>
    <div class="mut">
      Épée, Marteau, Hache puis Morgenstern, invoqués à chaque palier de 20 % de sa vie (à 2 PO en ligne en haut à droite d'elle, sinon la case libre
      suivante à 2 PO dans le sens des aiguilles d'une montre) : en jaune la case de la prochaine, en rouge la zone qu'elle frappera.
    </div>
  </div>
  <div class="ctl"><Switch bind:checked={settings.combat.guerreWeapon} label="Prochaine arme de Guerre" /></div>
</div>

<style>
  .head { display: flex; align-items: center; gap: 16px; padding-bottom: 4px; border-bottom: 1px solid var(--line); margin-top: 24px; }
  h2 { font: 600 12px var(--font); letter-spacing: 0.08em; text-transform: uppercase; color: var(--mut); margin: 0; }
  .row { display: flex; align-items: center; gap: 16px; padding: 10px 0; border-bottom: 1px solid color-mix(in srgb, var(--line) 50%, transparent); }
  .row.sub { padding-left: 16px; }
  .row.off { opacity: 0.5; }
  .lb { flex: 1; min-width: 0; }
  .lb .mut { font-size: 12px; margin-top: 2px; }
  .ctl { flex-shrink: 0; display: flex; align-items: center; gap: 8px; }
</style>
