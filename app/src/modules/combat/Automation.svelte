<script lang="ts">
  import { RULE_TITLES, rules, type RuleBase } from "../../lib/stores/automation.svelte";
  import { playSound, SOUNDS } from "../../lib/sounds";
  import Switch from "../../lib/components/Switch.svelte";
  import Segmented from "../../lib/components/Segmented.svelte";
  import ConfirmButton from "../../lib/components/ConfirmButton.svelte";
  import Banner from "../../lib/components/Banner.svelte";
  import { settings, type CombatShow } from "../../lib/stores/settings.svelte";
  import { addCharacterName, fight, openCombatOverlay, removeCharacter, removeCharacterName } from "../../lib/stores/combat.svelte";

  /** Combat tab « Automatisation » (formerly in the Automation tab): the player's characters, the « À votre tour » sound
   *  (lib/automation.ts rule `turn`) and the fight following with its floating window. */

  /** Notes shown for each monster of the fight (floating window). */
  const SHOW: { id: CombatShow; label: string; title?: string }[] = [
    { id: "none", label: "Rien", title: "Aucune remarque : seulement les tags des monstres" },
    { id: "warning", label: "Avertissements" },
    { id: "invulnerable", label: "+ Invulnérabilités" },
    { id: "all", label: "+ Conseils" },
  ];
  const DESC = {
    turn: "Un son au début de chacun de vos tours de jeu, sauf si votre personnage est mort (votre personnage est reconnu à la connexion, application ouverte, ou par son nom ci-dessus).",
    follow: "Détecte les monstres du combat en cours (paquets du placement et des tours) et affiche leurs remarques et les poisons PA / PM dans la fenêtre flottante.",
  };
  const FOLLOW = "Suivre les monstres des combats";
  /** No character of the player known yet: neither the turn sound nor the poisons can tell it apart. */
  const unknownMe = $derived(!fight.mine?.length && (rules.turn.enabled || settings.combat.enabled));
  /** The character name being typed (Vos personnages). */
  let newName = $state("");
  function addName() {
    const n = newName.trim();
    if (n) void addCharacterName(n).then(() => (newName = ""));
  }
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

<div class="head">
  <h2>Automatisation</h2>
</div>
{#if unknownMe}
  <Banner tone="warn" inset>
    Personnage non reconnu : le son « {RULE_TITLES.turn} » ne se déclenchera pas et aucun poison ne sera affiché.
    {#if fight.pending?.length}Il sera reconnu au placement du prochain combat.{:else}Reconnectez-vous avec l'application ouverte, ou ajoutez son nom ci-dessous.{/if}
  </Banner>
{/if}
<div class="me">
  <span title="Reconnus à la connexion (application ouverte) ou par leur nom au placement d'un combat">Vos personnages</span>
  {#each fight.characters ?? [] as c (c.id)}
    <span class="chip">
      <ConfirmButton label="{c.name ?? 'nom au prochain combat'} ✕" confirmLabel="Oublier {c.name ?? 'ce personnage'} ?" mini title="Personnage reconnu (id {c.id}) : cliquer pour l'oublier (une connexion avec lui le rajoute)" onconfirm={() => removeCharacter(c.id)} />
    </span>
  {/each}
  {#each fight.pending ?? [] as n (n)}
    <span class="chip wait">
      <button class="mini" title="Pas encore vu (reconnu au placement d'un combat) : cliquer pour ne plus chercher ce nom" onclick={() => removeCharacterName(n)}>{n} <span class="mut">(en attente)</span> ✕</button>
    </span>
  {/each}
  <form class="add" onsubmit={(e) => (e.preventDefault(), addName())}>
    <input type="text" placeholder="Nom du personnage" bind:value={newName} />
    <button class="mini" type="submit" disabled={!newName.trim()}>Ajouter</button>
  </form>
</div>
<div class="rule" class:off={!rules.turn.enabled}>
  <div class="rh">
    <Switch bind:checked={rules.turn.enabled} label="Activer la règle" />
    <div class="lb">
      <div><b>{RULE_TITLES.turn}</b></div>
      <div class="mut">{DESC.turn}</div>
    </div>
  </div>
  <div class="params">
    <label class="chk" title="Ne pas sonner quand la fenêtre de Dofus est au premier plan">
      <input type="checkbox" bind:checked={rules.turn.unfocusedOnly} /> Seulement si le jeu n'est pas au premier plan
    </label>
    {@render sound(rules.turn)}
  </div>
</div>
<div class="rule" class:off={!settings.combat.enabled}>
  <div class="rh">
    <Switch bind:checked={settings.combat.enabled} label="Suivre les combats" />
    <div class="lb">
      <div><b>{FOLLOW}</b></div>
      <div class="mut">{DESC.follow}</div>
    </div>
  </div>
  <div class="params">
    <Segmented bind:value={settings.combat.show} options={SHOW} label="Informations affichées" />
    <label class="chk" title="Petite fenêtre toujours au premier plan (sauf jeu en plein écran exclusif) : elle s'ouvre au début d'un combat, là où vous l'avez laissée, et se ferme à la fin">
      <input type="checkbox" bind:checked={settings.combat.overlay} /> Fenêtre flottante pendant les combats
    </label>
    <button class="mini" onclick={() => openCombatOverlay()} title="Afficher la fenêtre flottante maintenant (pour la placer sur le jeu : elle rouvrira au même endroit)">⧉ Afficher</button>
  </div>
</div>

<style>
  .head { display: flex; align-items: center; gap: 16px; padding-bottom: 4px; border-bottom: 1px solid var(--line); margin: 24px 0 4px; }
  h2 { font: 600 12px var(--font); letter-spacing: 0.08em; text-transform: uppercase; color: var(--mut); margin: 0; }
  .rule { padding: 8px 0; display: flex; flex-direction: column; gap: 8px; }
  .rule.off .params { opacity: 0.5; }
  .rh { display: flex; gap: 12px; align-items: flex-start; }
  .lb .mut { font-size: 12px; margin-top: 2px; }
  .params { display: flex; flex-wrap: wrap; gap: 8px 18px; align-items: center; padding-left: 56px; }
  .params label { display: inline-flex; align-items: center; gap: 6px; }
  .params .chk { cursor: pointer; }
  .params .chk input { margin: 0; }
  .snd { display: inline-flex; align-items: center; gap: 6px; }
  .snd input[type="range"] { width: 110px; }
  .vol { min-width: 38px; }
  .me { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 8px; padding: 4px 0; }
  .chip { display: inline-flex; }
  .chip :global(button) { border-radius: 999px; padding: 2px 10px; }
  .chip.wait :global(button) { border-style: dashed; }
  .add { display: inline-flex; gap: 6px; }
  .add input { width: 170px; }
</style>
