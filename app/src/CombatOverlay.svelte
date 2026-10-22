<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { fight, followFight, groups, shown, type Group, type Notes } from "./lib/stores/combat.svelte";
  import { settings, type CombatShow } from "./lib/stores/settings.svelte";

  /** Floating fight window (combat.html): the Combat tab's monsters and notes in a small always-on-top window over the game.
   *  The display level is the shared setting (changing it here changes it in the app too). */
  const SHOW: { id: CombatShow; label: string; title: string }[] = [
    { id: "warning", label: "⚠", title: "Avertissements" },
    { id: "invulnerable", label: "⚠ 🛡", title: "Avertissements + invulnérabilités" },
    { id: "all", label: "⚠ 🛡 💡", title: "Avertissements + invulnérabilités + conseils" },
  ];
  const ICON: Record<keyof Notes, string> = { warning: "⚠", invulnerable: "🛡", tips: "💡" };

  const all = $derived(groups(fight.fighters));
  const noted = $derived(
    all
      .filter((g) => shown(g.notes, settings.combat.show).length > 0)
      .sort((a, b) => Number(a.summoned) - Number(b.summoned) || (b.notes?.warning?.length ?? 0) - (a.notes?.warning?.length ?? 0) || a.name.localeCompare(b.name)),
  );
  const quiet = $derived(all.filter((g) => !noted.includes(g)));
  const lv = (g: Group) => {
    const l = [...new Set(g.levels.filter((x) => x > 0))].sort((a, b) => a - b);
    return l.length ? `niv. ${l.join(", ")}` : "";
  };
  const close = () => getCurrentWindow().close();

  onMount(() => void followFight());
</script>

<div class="box">
  <div class="top" data-tauri-drag-region>
    <span class="t" data-tauri-drag-region>
      {#if fight.active}<span class="dot" title="Combat en cours"></span>{/if}Combat
      {#if fight.seen && !fight.active}<span class="mut" data-tauri-drag-region>terminé</span>{/if}
    </span>
    <span class="seg">
      {#each SHOW as s (s.id)}
        <button class:on={settings.combat.show === s.id} title={s.title} onclick={() => (settings.combat.show = s.id)}>{s.label}</button>
      {/each}
    </span>
    <button class="x" onclick={close} title="Fermer">✕</button>
  </div>
  <div class="list">
    {#if !settings.combat.enabled}
      <div class="mut msg">Suivi des combats désactivé (onglet Combat).</div>
    {:else if !fight.fighters.length}
      <div class="mut msg">En attente d'un combat…</div>
    {/if}
    {#each noted as g (g.key)}
      <div class="mon" class:sum={g.summoned}>
        <div class="hd">
          <span class="nm">{g.name}</span>
          {#if g.levels.length > 1}<span class="cnt">×{g.levels.length}</span>{/if}
          <span class="mut">{lv(g)}</span>
          {#if g.summoned}<span class="mut tag">invoc.</span>{/if}
        </div>
        {#each shown(g.notes, settings.combat.show) as [kind, lines] (kind)}
          {#each lines as l, i (i)}
            <div class="ln {kind}"><span class="ic">{ICON[kind]}</span><span>{l}</span></div>
          {/each}
        {/each}
      </div>
    {/each}
    {#if quiet.length}
      <div class="quiet mut">
        {#each quiet as g, i (g.key)}{i ? " · " : ""}{g.name}{g.levels.length > 1 ? ` ×${g.levels.length}` : ""}{/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .box { height: 100%; box-sizing: border-box; display: flex; flex-direction: column; background: rgba(28, 31, 38, 0.86); border: 1px solid rgba(90, 169, 255, 0.45); border-radius: 10px; overflow: hidden; font-family: var(--font); font-size: 12px; }
  .top { display: flex; align-items: center; gap: 6px; padding: 5px 6px 5px 8px; background: rgba(20, 22, 26, 0.7); cursor: move; user-select: none; }
  .t { flex: 1; font-weight: 600; display: inline-flex; align-items: center; gap: 6px; }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); }
  .seg { display: inline-flex; border: 1px solid rgba(255, 255, 255, 0.12); border-radius: 5px; overflow: hidden; }
  .seg button { border: 0; border-radius: 0; padding: 1px 6px; font-size: 10px; background: none; color: var(--mut); }
  .seg button + button { border-left: 1px solid rgba(255, 255, 255, 0.12); }
  .seg button.on { background: rgba(90, 169, 255, 0.22); color: var(--fg); }
  .x { border: 0; background: none; color: var(--mut); padding: 0 4px; font-size: 13px; }
  .x:hover { color: var(--bad); }
  .list { flex: 1; overflow: auto; }
  .msg { padding: 12px; text-align: center; }
  .mon { padding: 6px 8px; border-top: 1px solid rgba(255, 255, 255, 0.07); display: flex; flex-direction: column; gap: 3px; }
  .mon:first-child { border-top: 0; }
  .mon.sum { opacity: 0.8; }
  .hd { display: flex; align-items: baseline; gap: 5px; flex-wrap: wrap; }
  .nm { font-weight: 600; font-size: 12.5px; }
  .cnt { font-weight: 600; color: var(--acc); }
  .tag { font-size: 10px; text-transform: uppercase; }
  .ln { display: flex; gap: 6px; align-items: flex-start; padding: 2px 6px; border-radius: 4px; line-height: 1.35; }
  .ln .ic { flex: none; width: 14px; text-align: center; }
  .ln.warning { background: rgba(224, 176, 74, 0.16); }
  .ln.warning .ic { color: var(--warn); }
  .ln.invulnerable { background: rgba(90, 169, 255, 0.14); }
  .ln.tips { background: rgba(138, 145, 160, 0.12); }
  .quiet { padding: 6px 8px; border-top: 1px solid rgba(255, 255, 255, 0.07); font-size: 11px; }
</style>
