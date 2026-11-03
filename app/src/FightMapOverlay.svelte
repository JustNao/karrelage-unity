<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow, PhysicalPosition, PhysicalSize } from "@tauri-apps/api/window";
  import { emitTo } from "@tauri-apps/api/event";
  import { call, listenAll } from "./lib/api";
  import { readJSON, writeJSON } from "./lib/persist.svelte";
  import { fight, followFight } from "./lib/stores/combat.svelte";
  import { CALIBRATION_KEY, calibrate, centre, type Calibration, type FightMapState, type MapFighter, type OverlayView } from "./lib/fightMap";
  import LosLayer from "./fightmap/LosLayer.svelte";
  import ClassLayer from "./fightmap/ClassLayer.svelte";

  /**
   * In-game view (fightmap.html): a transparent window over the Dofus window's picture during a fight, opened by a shortcut
   * (lib/fightMap.ts `pressFightMapOverlay`; `?view=` los / classHelp, `&hold=1` = shown while the shortcut is held).
   * - No calibration for this window size: the player clicks the centres of the two fighters named (lib/fightMap.ts
   *   `calibrate`), and it goes on.
   * - Then the view's layer: line of sight from the hovered cell (fightmap/LosLayer.svelte), or the class help
   *   (fightmap/ClassLayer.svelte; view `portals`: an Eliotrope portal simulated on the cell under the mouse, opened by the
   *   game's portal spell key, closed by a click too: the spell is cast, or cancelled).
   * - Closed when the held shortcut is let go (unless a cell is pinned, or during the calibration), by its shortcut pressed
   *   again, Escape, the end of the player's turn or of the fight. Another view's shortcut switches to that view.
   * - Kept for the whole fight (`?prewarm=1`: created hidden by the main window when the fight starts, then put on screen
   *   once loaded, empty): closing empties it and lets the mouse through (`idle`; never hidden: WebView2 flashes white when a
   *   hidden window is shown again), a shortcut draws a view at once (`fightmap:press` → `open`); only the fight's end closes
   *   the window.
   */
  const win = getCurrentWindow();
  const dpr = window.devicePixelRatio || 1;
  const params = new URLSearchParams(location.search);

  const asked = params.get("view");
  let view = $state<OverlayView>(asked === "classHelp" || asked === "portals" ? asked : "los");
  /** shown while the view's shortcut is held: closed when it is let go */
  let hold = $state(params.get("hold") === "1");
  /** on screen (a prewarmed window waits hidden for a shortcut) */
  let shown = $state(false);
  /** a pinned cell (line of sight): the view stays once the shortcut is let go */
  let sticky = $state(false);
  let live = $state.raw<FightMapState>({ map: null, fight: true, fighters: [] });
  let cells = $state.raw<number[] | null>(null);
  let cal = $state.raw<Calibration | null>(null);
  let size = { width: 0, height: 0 };
  let mode = $state<"loading" | "calibrate" | "show">("loading");
  /** the Dofus window's client area the view covers (physical px), as last applied to this window */
  let placed: { x: number; y: number; width: number; height: number } | null = null;
  /** calibration clicks so far (physical px of the client area) */
  let clicks = $state.raw<[number, number][]>([]);
  let failed = $state(false);
  /** when the calibration ended */
  let shownAt = 0;
  /** the mouse during the calibration (CSS px): a cross about a cell wide is drawn there */
  let pointer = $state<[number, number] | null>(null);

  const mine = $derived(new Set(fight.mine ?? []));
  const names = $derived.by(() => {
    const m = new Map<number, string>();
    for (const f of fight.fighters) m.set(f.actor, f.name);
    for (const c of fight.characters ?? []) if (c.name) m.set(c.id, c.name);
    return m;
  });
  const alive = $derived(live.fighters.filter((f) => f.alive && !f.carried && f.cell >= 0));

  /** The two fighters to click: the two farthest apart on both axes (the calibration is more precise the farther apart they
   *  are), the player's character first when it is one of them. */
  const targets = $derived.by((): [MapFighter, MapFighter] | null => {
    const at = alive.map((f) => [f, ...centre(f.cell, 1)] as const);
    let best: [MapFighter, MapFighter] | null = null, most = -1;
    for (let i = 0; i < at.length; i++)
      for (let j = i + 1; j < at.length; j++) {
        const [a, ax, ay] = at[i], [b, bx, by] = at[j];
        const spread = Math.min(Math.abs(bx - ax), 2 * Math.abs(by - ay));
        if (spread > most) [best, most] = [mine.has(b.id) && !mine.has(a.id) ? [b, a] : [a, b], spread];
      }
    return best;
  });
  const nameOf = (f: MapFighter) => names.get(f.id) ?? (mine.has(f.id) ? "votre personnage" : f.kind === "player" ? "ce personnage" : "ce monstre");

  /** Shows `v` over the game: the window placed on the Dofus window (it may have moved), its calibration checked. The
   *  portal simulation does not take the focus (the game keeps its spell selected). */
  async function open(v: OverlayView, h: boolean) {
    [view, hold, sticky, clicks, failed, pointer] = [v, h, false, [], false, null];
    const rect = await place();
    if (!rect || !live.fight || !cells) return void hide();
    size = { width: rect.width, height: rect.height };
    const saved = readJSON<Calibration | null>(CALIBRATION_KEY, null);
    cal = saved && saved.width === rect.width && saved.height === rect.height ? saved : null;
    mode = cal ? "show" : "calibrate";
    shownAt = Date.now();
    const focus = view !== "portals" || mode === "calibrate";
    await win.setFocusable(focus).catch(() => {});
    // the mouse taken only when the view needs it (calibration, line of sight): click-through is the WS_EX_LAYERED style, and
    // its first removal in the window's life flashed white (the class help and the portals keep letting the mouse through)
    if (mode === "calibrate" || view === "los") await win.setIgnoreCursorEvents(false);
    shown = true;
    if (focus) await win.setFocus().catch(() => {});
    else void call("fightmap", "refocus", {}, true).catch(() => {});
    await call("fightmap", "watch_escape", { on: true });
    // let go while the view was opening (a quick press)
    if (hold && !(await call<boolean>("shortcuts", "held", { name: view }).catch(() => true))) letGo();
  }

  /** The window put on the Dofus window's client area (it may have moved since): that area, null without a Dofus window. */
  async function place() {
    const rect = await call<{ x: number; y: number; width: number; height: number } | null>("fightmap", "window");
    if (rect && (!placed || placed.x !== rect.x || placed.y !== rect.y || placed.width !== rect.width || placed.height !== rect.height)) {
      await win.setPosition(new PhysicalPosition(rect.x, rect.y));
      await win.setSize(new PhysicalSize(rect.width, rect.height));
      placed = rect;
    }
    return rect;
  }

  /** Nothing drawn, the mouse and the keyboard left to the game. */
  async function idle() {
    await win.setIgnoreCursorEvents(true);
    await win.setFocusable(false).catch(() => {});
  }

  /** Escape no longer watched and the game focused again first (its own shortcuts work at once), then the view empties (the
   *  window stays on screen, ready for the next shortcut). */
  async function hide() {
    if (!shown) return;
    shown = false;
    await call("fightmap", "release", {}, true).catch(() => {});
    await idle();
  }

  /** The fight is over: the window goes (the main window makes a new one at the next fight). */
  let closing = false;
  async function close() {
    if (closing) return;
    closing = true;
    if (shown) await call("fightmap", "release", {}, true).catch(() => {});
    await win.close();
  }

  /** Shown while held: hides once the shortcut is up, unless something keeps it (calibration, pinned cell). */
  const letGo = () => hold && !sticky && mode === "show" && void hide();

  onMount(() => {
    const following = followFight();
    /** whether the last turn that started was one of the player's: its end (another fighter's turn) closes the view */
    let mineTurn = false;
    const off = listenAll({
      "fightmap:state": (s: FightMapState) => (live = s),
      "fightmap:escape": () => void hide(),
      // (the calibration's last click comes back from the click watcher too: not that one)
      "fightmap:click": () => shown && view === "portals" && !hold && mode === "show" && Date.now() - shownAt > 500 && void hide(),
      "fightmap:turn": ({ id }: { id: number }) => {
        if (mine.has(id)) mineTurn = true;
        else if (mineTurn) void hide();
      },
      "fightmap:press": (p: { view: OverlayView; hold: boolean }) => {
        if (!shown) return void open(p.view, p.hold).catch(() => void hide());
        if (p.view === view) return void hide();
        [view, hold, sticky] = [p.view, p.hold, false];
        void win.setFocusable(view !== "portals" || mode === "calibrate").catch(() => {});
      },
      "shortcuts:released": ({ name }: { name: string }) => name === view && letGo(),
    });
    const key = (e: KeyboardEvent) => e.key === "Escape" && void hide();
    window.addEventListener("keydown", key);
    (async () => {
      await off.ready;
      const state = await call<FightMapState>("fightmap", "state");
      live = state;
      if (!state.fight || state.map === null) return void close();
      cells = await call<number[] | null>("fightmap", "cells", { map: state.map });
      if (!cells) return void close();
      loadedMap = state.map;
      if (params.get("prewarm") !== "1") await open(view, hold);
      else {
        // on screen once, empty and letting everything through: showing it later would flash. Off the screen first: the
        // first time the mouse is taken back (the click-through style removed: F3, calibration) flashes white, so that is done
        // there, before the window is ever over the game
        await idle();
        const rect = await place();
        if (rect) await win.setPosition(new PhysicalPosition(-32000, -32000));
        await win.show();
        await win.setIgnoreCursorEvents(false);
        await new Promise((r) => setTimeout(r, 150)); // painted at least once without the style
        await win.setIgnoreCursorEvents(true);
        if (rect) await win.setPosition(new PhysicalPosition(rect.x, rect.y));
        void call("fightmap", "refocus", {}, true).catch(() => {});
      }
      await emitTo("main", "fightmap:ready");
    })().catch(() => void close());
    return () => {
      off();
      window.removeEventListener("keydown", key);
      void following.then((f) => f());
      void call("fightmap", "watch_escape", { on: false }, true).catch(() => {});
    };
  });

  // the fight is over: nothing left to show
  $effect(() => {
    if (!live.fight) void close();
  });

  // a new map mid-fight (next room of a dungeon): its cells
  let loadedMap: number | null = null;
  $effect(() => {
    const map = live.map;
    if (loadedMap === null || map === null || map === loadedMap) return;
    loadedMap = map;
    call<number[] | null>("fightmap", "cells", { map }).then((c) => (cells = c));
  });

  async function click(e: MouseEvent) {
    if (mode !== "calibrate" || !targets) return;
    const next = [...clicks, [e.clientX * dpr, e.clientY * dpr] as [number, number]];
    if (next.length < 2) return void (clicks = next);
    const c = calibrate(targets[0].cell, next[0], targets[1].cell, next[1], size.width, size.height);
    clicks = [];
    failed = !c;
    if (!c) return;
    writeJSON(CALIBRATION_KEY, c);
    cal = c;
    mode = "show";
    shownAt = Date.now();
    if (view === "portals") void win.setFocusable(false).then(() => call("fightmap", "refocus", {}, true)).catch(() => {});
    // the shortcut was let go during the calibration: the view stays until it is pressed again (or Escape...)
    if (hold && !(await call<boolean>("shortcuts", "held", { name: view }).catch(() => true))) hold = false;
  }

  /** a cell's width (CSS px) guessed before the calibration: the map is 14.5 cells wide and about 10 high, the game's bars
   *  take some of the height */
  const crossW = Math.min(window.innerWidth / 15, window.innerHeight / 12);
</script>

<!-- hidden: nothing drawn, the layers (their mouse polling) stopped -->
{#if shown}
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="overlay" class:calibrating={mode === "calibrate"} onclick={click} onmousemove={(e) => mode === "calibrate" && (pointer = [e.clientX, e.clientY])}>
  {#if mode === "calibrate"}
    <div class="popup">
      <b>Calibration de la vue en jeu</b>
      {#if !targets}
        <p>Il faut au moins deux combattants placés. Échap pour fermer.</p>
      {:else}
        <p>
          {#if clicks.length === 0}1/2 : cliquez au centre de la case de <b>{nameOf(targets[0])}</b>.
          {:else}2/2 : cliquez au centre de la case de <b>{nameOf(targets[1])}</b>.{/if}
        </p>
        {#if failed}<p class="bad">Ces deux clics ne collent pas à la grille : recommencez.</p>{/if}
        <p class="mut">Une seule fois par taille de fenêtre du jeu. Échap pour annuler.</p>
      {/if}
    </div>
    {#if pointer}
      {@const [x, y] = pointer}
      <svg width="100%" height="100%" class="cross">
        <path d="M{x - crossW / 2} {y}H{x + crossW / 2}M{x} {y - crossW / 4}V{y + crossW / 4}" class="under" />
        <path d="M{x - crossW / 2} {y}H{x + crossW / 2}M{x} {y - crossW / 4}V{y + crossW / 4}" />
      </svg>
    {/if}
  {:else if mode === "show" && cal && cells}
    {#if view === "los"}
      <LosLayer {cal} {cells} {live} {mine} {dpr} {win} {hold} bind:sticky />
    {:else}
      <ClassLayer {cal} {live} {mine} {dpr} {win} {hold} simulate={view === "portals"} />
    {/if}
  {/if}
</div>
{/if}

<style>
  :global(html), :global(body) { background: transparent !important; }
  /* nearly transparent rather than transparent: the window must still get the mouse while it is not let through */
  .overlay { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.01); font-family: var(--font); font-size: 13px; cursor: crosshair; }
  .overlay.calibrating { background: rgba(0, 0, 0, 0.18); cursor: none; }
  .overlay :global(svg) { position: absolute; inset: 0; pointer-events: none; }
  .cross path { stroke: #fff; stroke-width: 1; }
  .cross path.under { stroke: rgba(0, 0, 0, 0.8); stroke-width: 3; }
  .popup, .overlay :global(.pill) { position: absolute; left: 50%; transform: translateX(-50%); background: var(--panel); color: var(--fg); border: 1px solid var(--line); border-radius: 8px; box-shadow: 0 6px 24px rgba(0, 0, 0, 0.4); pointer-events: none; }
  .popup { top: 18%; padding: 12px 16px; max-width: 420px; text-align: center; }
  .popup p { margin: 8px 0 0; }
  .overlay :global(.pill) { top: 10px; padding: 4px 12px; font-size: 12px; opacity: 0.9; white-space: nowrap; }
  .bad { color: var(--bad); }
</style>
