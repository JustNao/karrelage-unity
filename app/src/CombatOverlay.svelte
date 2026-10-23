<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow, LogicalPosition, LogicalSize } from "@tauri-apps/api/window";
  import { fight, followFight, NOTE_KINDS, OVERLAY_POS_KEY, OVERLAY_WIDTH, poisonText, shown, sortedGroups, type Group } from "./lib/stores/combat.svelte";
  import { settings } from "./lib/stores/settings.svelte";
  import { tagsOf } from "./lib/monsterTags";
  import { writeJSON } from "./lib/persist.svelte";
  import FloatingShell from "./lib/components/FloatingShell.svelte";

  /** Floating fight window (combat.html): the Combat tab's monsters, their notes (display level = the app's setting) and the
   *  tags worth showing, in a small always-on-top window over the game. Its height follows its content (up to 80 % of the
   *  screen, then it scrolls); where the user drops it is remembered for the next fight. */
  const list = $derived(sortedGroups(fight.fighters, settings.combat.show));
  const notesOf = (g: Group) => shown(g.notes, settings.combat.show);

  let top = $state<HTMLElement>();
  let body = $state<HTMLElement>();

  onMount(() => {
    const following = followFight();
    const win = getCurrentWindow();
    // natural height = title bar + the list's content (+ the box's 2px border); growing up keeps the bottom edge in place
    let last = 0;
    const fit = async () => {
      if (!top || !body) return;
      const h = Math.ceil(Math.min(top.offsetHeight + body.offsetHeight + 2, screen.availHeight * 0.8));
      if (h === last) return;
      const before = last || window.innerHeight; // the height asked last (innerHeight can lag behind a resize)
      last = h;
      try {
        if (settings.combat.overlayGrow === "up") {
          const p = (await win.outerPosition()).toLogical(await win.scaleFactor());
          await win.setPosition(new LogicalPosition(p.x, p.y + before - h));
        }
        await win.setSize(new LogicalSize(OVERLAY_WIDTH, h));
      } catch {}
    };
    const ro = new ResizeObserver(() => void fit());
    if (top) ro.observe(top);
    if (body) ro.observe(body);
    void fit();
    const moved = win.onMoved(async ({ payload }) => {
      const p = payload.toLogical(await win.scaleFactor());
      writeJSON(OVERLAY_POS_KEY, { x: Math.round(p.x), y: Math.round(p.y), bottom: Math.round(p.y + (last || window.innerHeight)) });
    });
    return () => {
      ro.disconnect();
      void moved.then((f) => f());
      void following.then((f) => f());
    };
  });
</script>

<!-- growing up (popup at the bottom of the screen): the title bar sits at the bottom, under the monsters -->
<FloatingShell up={settings.combat.overlayGrow === "up"} bind:top bind:body>
  {#snippet title()}
    {#if fight.active}<span class="dot" title="Combat en cours"></span>{/if}Combat
    {#if fight.seen && !fight.active}<span class="mut">terminé</span>{/if}
  {/snippet}
  {#snippet actions()}
    <button
      class="x grow"
      onclick={() => (settings.combat.overlayGrow = settings.combat.overlayGrow === "up" ? "down" : "up")}
      title={settings.combat.overlayGrow === "up"
        ? "Fenêtre ancrée en bas (barre en bas, s'agrandit vers le haut) : cliquer pour l'ancrer en haut"
        : "Fenêtre ancrée en haut (barre en haut, s'agrandit vers le bas) : cliquer pour l'ancrer en bas"}
    >{settings.combat.overlayGrow === "up" ? "⤒" : "⤓"}</button>
  {/snippet}
  {#if !settings.combat.enabled}
    <div class="mut msg">Suivi des combats désactivé (onglet Combat).</div>
  {:else if !fight.fighters.length}
    <div class="mut msg">En attente d'un combat…</div>
  {/if}
  {#if settings.combat.enabled}
    {#each fight.poisons ?? [] as p (p.action + ":" + p.target)}
      <div class="poison" title={fight.me == null ? "Personnage pas encore identifié (reconnectez-vous) : poisons de tous les joueurs" : ""}>☠ {poisonText(p)}{fight.me == null ? " (un joueur)" : ""}</div>
    {/each}
  {/if}
  {#each list as g (g.key)}
    {@const tags = tagsOf(g.tags)}
    <div class="mon" class:sum={g.summoned} class:gone={!g.alive}>
      <div class="hd">
        <span class="nm">{g.name}</span>
        {#if g.alive > 1}<span class="cnt">×{g.alive}</span>{/if}
        {#if !g.alive}<span class="dead">mort</span>{/if}
        {#if g.summoned}<span class="mut tag">invoc.</span>{/if}
      </div>
      {#if tags.length}
        <div class="tags">{#each tags as t (t.id)}<span class="tg {t.group}">{t.label}</span>{/each}</div>
      {/if}
      {#each notesOf(g) as [kind, lines] (kind)}
        {#each lines as l, i (i)}
          <div class="ln {kind}"><span class="ic">{NOTE_KINDS[kind].icon}</span><span>{l}</span></div>
        {/each}
      {/each}
    </div>
  {/each}
</FloatingShell>

<style>
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--ok); }
  .x.grow:hover { color: var(--acc); }
  .msg { padding: 12px; text-align: center; }
  .mon { padding: 6px 12px; border-top: 1px solid rgba(255, 255, 255, 0.07); display: flex; flex-direction: column; gap: 4px; }
  .mon:first-child { border-top: 0; }
  .mon.sum { opacity: 0.8; }
  .hd { display: flex; align-items: baseline; gap: 5px; flex-wrap: wrap; }
  .nm { font-weight: 600; font-size: 12.5px; }
  .cnt { font-weight: 600; color: var(--acc); }
  .tag { font-size: 10px; text-transform: uppercase; }
  .tags { display: flex; flex-wrap: wrap; gap: 4px; }
  .ln { display: flex; gap: 6px; align-items: flex-start; padding: 2px 6px; border-radius: 4px; line-height: 1.35; }
  .ln .ic { flex: none; width: 14px; text-align: center; }
  .ln.warning { background: rgba(224, 176, 74, 0.16); }
  .ln.warning .ic { color: var(--warn); }
  .ln.invulnerable { background: rgba(90, 169, 255, 0.14); }
  .ln.tips { background: rgba(138, 145, 160, 0.12); }
  .tg { font-size: 10px; padding: 0 5px; border-radius: 4px; line-height: 16px; white-space: nowrap; border: 1px solid; }
  .tg.flag { color: var(--bad); border-color: rgba(229, 100, 110, 0.45); }
  .tg.control { color: var(--warn); border-color: rgba(224, 176, 74, 0.45); }
  .mon.gone { opacity: 0.45; }
  .dead { font-size: 10px; text-transform: uppercase; color: var(--bad); }
  .poison { margin: 6px 8px; padding: 5px 8px; border-radius: 6px; background: rgba(229, 100, 110, 0.22); border: 1px solid rgba(229, 100, 110, 0.6); color: #ffd7da; font-weight: 600; }
</style>
