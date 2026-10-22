<script lang="ts">
  import { fight, groups, openCombatOverlay, poisonText, shown, type Group, type Notes } from "../../lib/stores/combat.svelte";
  import { settings, type CombatShow } from "../../lib/stores/settings.svelte";
  import SmoothScroll from "../../lib/components/SmoothScroll.svelte";
  import { call } from "../../lib/api";
  import { tagsOf } from "../../lib/monsterTags";

  /** Combat: the monsters of the current fight (detected from the fight packets, lib/stores/combat.svelte.ts) with the notes
   *  kept for them: warnings, invulnerabilities, tips (how many kinds are shown = `settings.combat.show`), and their DofusDB
   *  tags (push, pull, AP / MP removal..., lib/monsterTags.ts). */
  const SHOW: { id: CombatShow; label: string }[] = [
    { id: "warning", label: "Avertissements" },
    { id: "invulnerable", label: "+ Invulnérabilités" },
    { id: "all", label: "+ Conseils" },
  ];
  const KIND: Record<keyof Notes, { icon: string; title: string }> = {
    warning: { icon: "⚠", title: "Avertissement" },
    invulnerable: { icon: "🛡", title: "Invulnérabilité" },
    tips: { icon: "💡", title: "Conseil" },
  };

  const all = $derived(groups(fight.fighters, settings.combat.show));
  const notesOf = (g: Group) => shown(g.notes, settings.combat.show);
  // dead last, then summons; warnings first, then monsters with notes, then by name
  const cards = $derived(
    [...all].sort(
      (a, b) =>
        Number(!a.alive) - Number(!b.alive) ||
        Number(a.summoned) - Number(b.summoned) ||
        (b.notes?.warning?.length ?? 0) - (a.notes?.warning?.length ?? 0) ||
        Number(notesOf(b).length > 0) - Number(notesOf(a).length > 0) ||
        a.name.localeCompare(b.name),
    ),
  );

  const time = (t: number) => new Date(t * 1000).toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" });
</script>

{#snippet title(g: Group)}
  <span class="nm">{g.name}</span>
  {#if g.alive > 1}<span class="cnt">×{g.alive}</span>{/if}
  {#if !g.alive}<span class="dead">mort</span>{/if}
  {#if g.summoned}<span class="tag" title="Apparu en cours de combat (invocation)">invoc.</span>{/if}
{/snippet}

<div class="view">
  <div class="bar">
    <button class="sw" class:on={settings.combat.enabled} role="switch" aria-checked={settings.combat.enabled} aria-label="Suivre les combats" title="Suivre les monstres des combats" onclick={() => (settings.combat.enabled = !settings.combat.enabled)}></button>
    <div class="seg" role="radiogroup" aria-label="Informations affichées" class:off={!settings.combat.enabled}>
      {#each SHOW as s (s.id)}
        <button role="radio" aria-checked={settings.combat.show === s.id} class:on={settings.combat.show === s.id} onclick={() => (settings.combat.show = s.id)}>{s.label}</button>
      {/each}
    </div>
    <label class="mut chk" title="Afficher cet onglet dès qu'un combat commence">
      <input type="checkbox" bind:checked={settings.combat.autoOpen} /> Ouvrir au début d'un combat
    </label>
    <label class="mut chk" title="Fenêtre toujours au premier plan, ouverte au début d'un combat et fermée à sa fin">
      <input type="checkbox" bind:checked={settings.combat.overlay} /> Fenêtre flottante pendant les combats
    </label>
    <button class="mini pop" onclick={() => openCombatOverlay()} title="Afficher la fenêtre flottante maintenant (pour la placer sur le jeu : elle rouvrira au même endroit)">⧉ Afficher</button>
    <span class="st">
      {#if !settings.combat.enabled}
        <span class="mut">Désactivé</span>
      {:else if fight.active}
        <span class="dot"></span> Combat en cours <span class="mut">depuis {time(fight.started)}</span>
      {:else if fight.seen}
        <span class="mut">Dernier combat, terminé à {time(fight.ended ?? fight.started)}</span>
        <button class="mini" onclick={() => call("combat", "clear")} title="Effacer le dernier combat">Effacer</button>
      {:else}
        <span class="mut">En attente d'un combat</span>
      {/if}
    </span>
  </div>

  <SmoothScroll>
    <div class="body" class:off={!settings.combat.enabled}>
      {#if settings.combat.enabled}
        {#each fight.poisons ?? [] as p (p.action + ":" + p.target)}
          <div class="poison">
            ☠ {poisonText(p)}{fight.me == null ? " (un joueur)" : ""}
            {#if fight.me == null}<span class="mut">— personnage pas encore identifié : reconnectez-vous pour ne voir que vos poisons</span>{/if}
          </div>
        {/each}
      {/if}
      {#if !settings.combat.enabled}
        <p class="mut empty">Le suivi des combats est désactivé : activez-le avec l'interrupteur ci-dessus (ou dans Paramètres).</p>
      {:else if !fight.fighters.length}
        <p class="mut empty">
          Les monstres du combat apparaissent ici dès la phase de placement (capture active), avec les avertissements, invulnérabilités et
          conseils connus pour chacun.
        </p>
      {:else}
        <div class="cards">
          {#each cards as g (g.key)}
            {@const notes = notesOf(g)}
            <article class="card" class:sum={g.summoned} class:gone={!g.alive} class:hasw={!!g.notes?.warning?.length} class:bare={!notes.length}>
              <header>{@render title(g)}</header>
              {#if g.tags.length}
                <div class="tags">
                  {#each tagsOf(g.tags) as t (t.id)}<span class="tg {t.group}">{t.label}</span>{/each}
                </div>
              {/if}
              {#each notes as [kind, lines] (kind)}
                <div class="note {kind}">
                  <span class="ic" title={KIND[kind].title}>{KIND[kind].icon}</span>
                  <ul>
                    {#each lines as l, i (i)}<li>{l}</li>{/each}
                  </ul>
                </div>
              {/each}
            </article>
          {/each}
        </div>
      {/if}
    </div>
  </SmoothScroll>
</div>

<style>
  .view { flex: 1; min-height: 0; display: flex; flex-direction: column; font-family: var(--font); }
  .view > :global(.wrap) { flex: 1; }
  .bar { display: flex; flex-wrap: wrap; align-items: center; gap: 10px 16px; padding: 12px 16px 10px; border-bottom: 1px solid var(--line); margin-bottom: 10px; }
  .st { margin-left: auto; display: inline-flex; align-items: center; gap: 8px; }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--ok); box-shadow: 0 0 0 0 color-mix(in srgb, var(--ok) 60%, transparent); animation: pulse 1.6s infinite; }
  @keyframes pulse { 70% { box-shadow: 0 0 0 6px transparent; } }
  .chk { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; cursor: pointer; }
  .chk input { margin: 0; }
  .pop { font-size: 12px; padding: 3px 9px; }
  .seg { display: inline-flex; border: 1px solid var(--line); border-radius: 6px; overflow: hidden; }
  .seg button { border: 0; border-radius: 0; background: none; color: var(--mut); padding: 4px 10px; font-size: 12px; }
  .seg button + button { border-left: 1px solid var(--line); }
  .seg button.on { background: var(--panel); color: var(--fg); font-weight: 600; }
  .seg.off, .body.off { opacity: 0.5; }
  .body { padding: 0 16px 16px; }
  .sw { position: relative; width: 44px; height: 24px; padding: 0; background: var(--line); border-radius: 12px; transition: 0.15s; flex: none; border: 0; }
  .sw::after { content: ""; position: absolute; top: 3px; left: 3px; width: 18px; height: 18px; border-radius: 50%; background: #fff; transition: 0.15s; }
  .sw.on { background: var(--ok); }
  .sw.on::after { left: 23px; }
  .empty { max-width: 640px; }
  .cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 10px; align-items: start; }
  .card { background: var(--panel); border: 1px solid var(--line); border-radius: 8px; padding: 10px 12px; display: flex; flex-direction: column; gap: 8px; }
  .card.hasw { border-left: 3px solid var(--warn); }
  .card.sum { opacity: 0.85; }
  header { display: flex; align-items: baseline; gap: 6px; flex-wrap: wrap; }
  .nm { font-weight: 600; font-size: 14px; }
  .cnt { font-weight: 600; color: var(--acc); }
  .tag { font-size: 10px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--mut); border: 1px solid var(--line); border-radius: 4px; padding: 0 4px; }
  .note { display: flex; gap: 8px; align-items: flex-start; padding: 6px 8px; border-radius: 6px; font-size: 12.5px; }
  .note ul { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 3px; }
  .note .ic { flex: none; width: 16px; text-align: center; }
  .note.warning { background: color-mix(in srgb, var(--warn) 13%, transparent); }
  .note.warning .ic { color: var(--warn); }
  .note.invulnerable { background: color-mix(in srgb, var(--acc) 12%, transparent); }
  .note.tips { background: color-mix(in srgb, var(--mut) 10%, transparent); }
  .card.bare { gap: 6px; }
  .tags { display: flex; flex-wrap: wrap; gap: 4px; }
  .tg { font-size: 11px; padding: 0 6px; border-radius: 4px; border: 1px solid var(--line); line-height: 18px; white-space: nowrap; }
  .tg.flag { color: var(--bad); border-color: color-mix(in srgb, var(--bad) 45%, transparent); }
  .tg.control { color: var(--warn); border-color: color-mix(in srgb, var(--warn) 45%, transparent); }
  .card.gone { opacity: 0.45; }
  .dead { font-size: 10px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--bad); }
  .poison { margin-bottom: 10px; padding: 8px 12px; border-radius: 8px; background: color-mix(in srgb, var(--bad) 16%, transparent); border: 1px solid var(--bad); font-weight: 600; }
  .poison .mut { font-weight: 400; margin-left: 6px; }
</style>
