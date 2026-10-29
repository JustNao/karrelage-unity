<script lang="ts">
  import { onMount, tick } from "svelte";
  import { slide } from "svelte/transition";
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
  import { fight, followFight, NOTE_KINDS, OVERLAY_POS_KEY, OVERLAY_WIDTH, poisonText, shown, sortedGroups, type Group } from "./lib/stores/combat.svelte";
  import { settings } from "./lib/stores/settings.svelte";
  import { tagsOf } from "./lib/monsterTags";
  import { persisted, writeJSON } from "./lib/persist.svelte";
  import FloatingShell from "./lib/components/FloatingShell.svelte";
  import BossHint from "./lib/components/BossHint.svelte";
  import { bossHints } from "./lib/bosses";

  /** Floating fight window (combat.html): the current fight's monsters, their notes (display level = the app's setting) and the
   *  tags worth showing, in a small always-on-top window over the game. Its height follows its content (up to 80 % of the
   *  screen, then it scrolls); where the user drops it is remembered for the next fight. Only the living monsters worth a
   *  look (a red tag, or a note shown at the display level) are listed, unless "tous" is on; a dead one slides away. */
  const notesOf = (g: Group) => shown(g.notes, settings.combat.show);
  const ui = persisted("karrelage.combat.overlay", { all: false });
  const worth = (g: Group) => tagsOf(g.tags).some((t) => t.group === "flag") || notesOf(g).length > 0 || bossHints(fight, g.gid).length > 0;
  // a finished fight leaves nothing (the window may stay open: opened by hand, or with the automatic opening off)
  const alive = $derived(fight.active ? sortedGroups(fight.fighters, settings.combat.show).filter((g) => g.alive > 0) : []);
  const list = $derived(ui.all ? alive : alive.filter(worth));
  const hidden = $derived(alive.length - list.length);

  let top = $state<HTMLElement>();
  let body = $state<HTMLElement>();

  /** The window stays transparent until the fight's list is in and the window fitted to it, then fades in (no empty box,
   *  no jump). */
  let ready = $state(false);

  onMount(() => {
    const following = followFight();
    const win = getCurrentWindow();
    // natural height = title bar + the list's content (+ the box's 2px border)
    let last = 0;
    /** Growing up: the bottom edge (logical y) the window hangs from, taken when it first fits, moved by the user's drags. */
    let bottom: number | null = null;
    /** Where the window last placed itself: its own move is not a drag. */
    let placed: { x: number; y: number } | null = null;
    // one fit at a time: overlapping ones read the position half-way through another's move and drift
    let queue = Promise.resolve();
    const fit = () => (queue = queue.then(fitNow).catch(() => {}));
    async function fitNow() {
      if (!top || !body) return;
      const h = Math.ceil(Math.min(top.offsetHeight + body.offsetHeight + 2, screen.availHeight * 0.8));
      if (h === last) return;
      const before = last || window.innerHeight;
      last = h;
      if (settings.combat.overlayGrow === "up") {
        const p = (await win.outerPosition()).toLogical(await win.scaleFactor());
        bottom ??= p.y + before;
        placed = { x: Math.round(p.x), y: Math.round(bottom - h) };
        // one step: a move then a resize would show the window a frame at its new top with its old height (flicker)
        await invoke("set_bounds", { ...placed, width: OVERLAY_WIDTH, height: h });
        return;
      }
      bottom = null;
      await win.setSize(new LogicalSize(OVERLAY_WIDTH, h));
    }
    const ro = new ResizeObserver(() => void fit());
    if (top) ro.observe(top);
    if (body) ro.observe(body);
    void (async () => {
      await following.catch(() => {}); // the fight's state is in: the list is complete
      await tick();
      await fit();
      ready = true;
    })();
    const moved = win.onMoved(async ({ payload }) => {
      const p = payload.toLogical(await win.scaleFactor());
      const own = placed && Math.abs(p.x - placed.x) <= 1 && Math.abs(p.y - placed.y) <= 1;
      const h = last || window.innerHeight;
      if (!own && bottom != null) bottom = p.y + h; // dragged by the user: hang from where it was dropped
      writeJSON(OVERLAY_POS_KEY, { x: Math.round(p.x), y: Math.round(p.y), bottom: Math.round(p.y + h) });
    });
    return () => {
      ro.disconnect();
      void moved.then((f) => f());
      void following.then((f) => f());
    };
  });
</script>

<!-- growing up (popup at the bottom of the screen): the title bar sits at the bottom, under the monsters -->
<FloatingShell up={settings.combat.overlayGrow === "up"} shown={ready} bind:top bind:body>
  {#snippet title()}
    {#if fight.active}<span class="dot" title="Combat en cours"></span>{/if}Combat
    {#if fight.seen && !fight.active}<span class="mut">terminé</span>{/if}
  {/snippet}
  {#snippet actions()}
    {#if alive.length}
      <button class="x all" class:on={ui.all} onclick={() => (ui.all = !ui.all)}
        title={ui.all ? "Tous les monstres affichés : cliquer pour ne garder que ceux à surveiller" : "Seuls les monstres à surveiller (tag rouge ou note) sont affichés : cliquer pour tous les voir"}
      >{ui.all ? "tous" : `+${hidden}`}</button>
    {/if}
    <button
      class="x grow"
      onclick={() => (settings.combat.overlayGrow = settings.combat.overlayGrow === "up" ? "down" : "up")}
      title={settings.combat.overlayGrow === "up"
        ? "Fenêtre ancrée en bas (barre en bas, s'agrandit vers le haut) : cliquer pour l'ancrer en haut"
        : "Fenêtre ancrée en haut (barre en haut, s'agrandit vers le bas) : cliquer pour l'ancrer en bas"}
    >{settings.combat.overlayGrow === "up" ? "⤒" : "⤓"}</button>
  {/snippet}
  {#if !settings.combat.enabled}
    <div class="mut msg">Suivi des combats désactivé (onglet Automation).</div>
  {:else if !fight.active}
    <div class="mut msg">En attente d'un combat…</div>
  {:else if !list.length && fight.active}
    <div class="mut msg">Rien à surveiller{hidden ? ` (${hidden} monstre${hidden > 1 ? "s" : ""})` : ""}</div>
  {/if}
  {#if settings.combat.enabled}
    {#each fight.poisons ?? [] as p (p.action + ":" + p.target)}
      <div class="poison">{poisonText(p)}</div>
    {/each}
  {/if}
  {#each list as g (g.key)}
    {@const tags = tagsOf(g.tags)}
    <div class="mon" class:sum={g.summoned} transition:slide={{ duration: 220 }}>
      <div class="hd">
        <span class="nm">{g.name}</span>
        {#if g.alive > 1}<span class="cnt">×{g.alive}</span>{/if}
        {#if g.summoned}<span class="mut tag">invoc.</span>{/if}
      </div>
      {#if tags.length}
        <div class="tags">{#each tags as t (t.id)}<span class="tg {t.group}">{t.label}</span>{/each}</div>
      {/if}
      {#each bossHints(fight, g.gid) as h, i (i)}
        <BossHint hint={h} />
      {/each}
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
  .x.all { font-size: 11px; }
  .x.all.on, .x.all:hover { color: var(--acc); }
  .poison { margin: 6px 8px; padding: 5px 8px; border-radius: 6px; background: rgba(229, 100, 110, 0.22); border: 1px solid rgba(229, 100, 110, 0.6); color: #ffd7da; font-weight: 600; animation: flash 0.45s ease-in-out 4 alternate; }
  /* a poison just applied: the background flashes to catch the eye, then settles */
  @keyframes flash {
    from { background: rgba(229, 100, 110, 0.22); }
    to { background: rgba(229, 100, 110, 0.85); color: #fff; }
  }
</style>
