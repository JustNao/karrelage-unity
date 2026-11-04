<script lang="ts">
  import { onMount } from "svelte";
  import { cursorPosition, getCurrentWindow, PhysicalPosition, PhysicalSize } from "@tauri-apps/api/window";
  import { emitTo } from "@tauri-apps/api/event";
  import { call, listenAll } from "./lib/api";
  import { readJSON, writeJSON } from "./lib/persist.svelte";
  import { fight, followFight } from "./lib/stores/combat.svelte";
  import { CALIBRATION_KEY, calibrate, cellAt, centre, type Calibration, type FightMapState, type MapFighter } from "./lib/fightMap";
  import type { OverlayPress, OverlayView } from "./lib/fightMapOverlay";
  import LosLayer from "./fightmap/LosLayer.svelte";
  import ClassLayer from "./fightmap/ClassLayer.svelte";

  /**
   * In-game view (fightmap.html): a transparent window over the Dofus window's picture during a fight, made hidden by the main
   * window when the fight starts (lib/fightMapOverlay.ts), a view drawn in it by a shortcut (`fightmap:press` → `open`).
   * - No calibration for this window size: the player clicks the centres of the two fighters named (lib/fightMap.ts
   *   `calibrate`), and it goes on.
   * - Then the view's layer: line of sight from the hovered cell (fightmap/LosLayer.svelte), or the class help
   *   (fightmap/ClassLayer.svelte; view `portals`: an Eliotrope portal simulated on the cell under the mouse, opened by the
   *   game's portal spell key, closed by a click too: the spell is cast, or cancelled).
   * - Closed when the held shortcut is let go (unless a cell is pinned, or during the calibration), by its shortcut pressed
   *   again, Escape, the end of the player's turn or of the fight. Another view's shortcut switches to that view.
   * - Kept for the whole fight: put on screen once loaded, empty (`warmUp`); closing just empties it (never hidden: WebView2
   *   flashes white when a hidden window is shown again); only the fight's end closes it.
   * - The mouse and the keyboard stay the game's unless the view needs them (`takesInput`).
   */
  /** The cursor is asked for again this long after each answer (about a frame, never two asks at once). */
  const CURSOR_POLL_MS = 16;

  const win = getCurrentWindow();
  const dpr = window.devicePixelRatio || 1;

  let view = $state<OverlayView>("los");
  /** shown while the view's shortcut is held: closed when it is let go */
  let hold = $state(false);
  /** a view is drawn (else the window is on screen, empty, waiting for a shortcut) */
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
  let calibratedAt = 0;
  /** `open` is under way (the window placed, the focus set...): a close asked meanwhile waits for it (`closeAsked`: "any" =
   *  Escape, the shortcut again, the end of the turn; "click" = a click, which only closes the portal simulation) */
  let opening = false;
  let closeAsked: "any" | "click" | null = null;
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

  /** The window takes the mouse and the focus only for what needs a click on it: the calibration, the line of sight until a
   *  cell is pinned. Everything else lets all through to the game (it keeps the Portail spell selected, its shortcuts...). */
  const takesInput = $derived(shown && (mode === "calibrate" || (view === "los" && !sticky)));

  // Applied only when it changes, in order: each call changes a window style, which is what can flash (click-through is the
  // WS_EX_LAYERED style, its first removal in the window's life flashes white: `warmUp`).
  let applied = false;
  let applying = Promise.resolve();
  $effect(() => {
    const on = takesInput;
    if (on === applied) return;
    applied = on;
    applying = applying
      .then(async () => {
        await win.setFocusable(on);
        await win.setIgnoreCursorEvents(!on);
        if (on) await win.setFocus();
        else await call("fightmap", "refocus", {}, true);
      })
      .catch(() => {});
  });

  /** The cell under the mouse, for the views that let it through (no mouse event then: polled). */
  let pointed = $state<number | null>(null);
  /** the cursor was read once: `pointed` is where it was when the view opened */
  let polled = $state(false);
  $effect(() => {
    if (!shown || mode !== "show" || view === "los" || !cal || !placed) return;
    const [grid, origin] = [cal, placed];
    let on = true;
    let timer = 0;
    const tick = async () => {
      const p = await cursorPosition().catch(() => null);
      if (!on) return;
      [pointed, polled] = [p && cellAt(grid, p.x - origin.x, p.y - origin.y), true];
      timer = setTimeout(tick, CURSOR_POLL_MS);
    };
    polled = false;
    void tick();
    return () => {
      on = false;
      clearTimeout(timer);
    };
  });

  /** Shows `v` over the game: the window placed on the Dofus window (it may have moved), its calibration checked. */
  async function open(v: OverlayView, h: boolean) {
    [view, hold, sticky, clicks, failed, pointer] = [v, h, false, [], false, null];
    [opening, closeAsked] = [true, null];
    // Escape and clicks watched from now on: one while the view is still opening closes it once it is open
    void call("fightmap", "watch_escape", { on: true }).catch(() => {});
    try {
      const rect = await place();
      if (!rect || !live.fight || !cells) return void (await release());
      size = { width: rect.width, height: rect.height };
      const saved = readJSON<Calibration | null>(CALIBRATION_KEY, null);
      cal = saved && saved.width === rect.width && saved.height === rect.height ? saved : null;
      mode = cal ? "show" : "calibrate";
      shown = true;
    } finally {
      opening = false;
    }
    if (closeAsked === "any" || (closeAsked === "click" && closesOnClick())) return void hide();
    // let go while the view was opening (a quick press)
    if (hold && !(await call<boolean>("shortcuts", "held", { name: view }).catch(() => true))) letGo();
  }

  /** A click (cast or cancelled spell) closes the portal simulation opened by its key, not during its calibration. */
  const closesOnClick = () => view === "portals" && !hold && mode === "show";
  /** Closes the view, or once it is open when it is still opening. */
  const closeSoon = (by: "any" | "click") => (opening ? void (closeAsked = closeAsked === "any" ? "any" : by) : void hide());

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

  /** On screen once, empty and letting everything through: showing it later would flash. Off the screen first: the first time
   *  the mouse is taken back (the click-through style removed: F3, calibration) flashes white, so that is done there, before
   *  the window is ever over the game. */
  async function warmUp() {
    await win.setIgnoreCursorEvents(true);
    await win.setFocusable(false).catch(() => {});
    const rect = await place();
    await win.setPosition(new PhysicalPosition(-32000, -32000));
    await win.show();
    await win.setIgnoreCursorEvents(false);
    await new Promise((r) => setTimeout(r, 150)); // painted at least once without the style
    await win.setIgnoreCursorEvents(true);
    // (no Dofus window found: left off the screen, `open` places it)
    if (rect) await win.setPosition(new PhysicalPosition(rect.x, rect.y));
    void call("fightmap", "refocus", {}, true).catch(() => {});
  }

  /** Escape and clicks no longer watched, the game focused again (its own shortcuts work at once). */
  const release = () => call("fightmap", "release", {}, true).catch(() => {});

  /** The view empties (the window stays on screen, ready for the next shortcut). */
  async function hide() {
    if (!shown) return;
    shown = false;
    await release();
  }

  /** The fight is over: the window goes (the main window makes a new one at the next fight). */
  let closing = false;
  async function close() {
    if (closing) return;
    closing = true;
    if (shown) await release();
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
      "fightmap:escape": () => closeSoon("any"),
      // (the calibration's last click comes back from the click watcher too: not that one)
      "fightmap:click": () => (opening || (shown && closesOnClick() && Date.now() - calibratedAt > 500)) && closeSoon("click"),
      "fightmap:turn": ({ id }: { id: number }) => {
        if (mine.has(id)) mineTurn = true;
        else if (mineTurn) closeSoon("any");
      },
      "fightmap:press": (p: OverlayPress) => {
        // pressed again while opening: closed once open (another view's shortcut meanwhile is left out)
        if (opening) return void (p.view === view && closeSoon("any"));
        if (!shown) return void open(p.view, p.hold).catch(() => void hide());
        if (p.view === view) return void hide();
        [view, hold, sticky] = [p.view, p.hold, false];
      },
      "shortcuts:released": ({ name }: { name: string }) => name === view && letGo(),
    });
    (async () => {
      await off.ready;
      live = await call<FightMapState>("fightmap", "state");
      if (!live.fight) return void close();
      await warmUp();
      await emitTo("main", "fightmap:ready");
    })().catch(() => void close());
    return () => {
      off();
      void following.then((f) => f());
      void call("fightmap", "watch_escape", { on: false }, true).catch(() => {});
    };
  });

  // the fight is over: nothing left to show
  $effect(() => {
    if (!live.fight) void close();
  });

  // the map's cells, asked again when it changes (the roleplay map, then the fight's own; the next room of a dungeon): a map
  // missing from the pack leaves the view unavailable until the next one
  let loadedMap: number | null = null;
  $effect(() => {
    const map = live.map;
    if (map === null || map === loadedMap) return;
    loadedMap = map;
    call<number[] | null>("fightmap", "cells", { map }, true).then((c) => map === loadedMap && (cells = c)).catch(() => {});
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
    calibratedAt = Date.now();
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
      <LosLayer {cal} {cells} {live} {mine} {dpr} {hold} bind:sticky />
    {:else}
      <ClassLayer {cal} {live} {mine} {dpr} {hold} {pointed} {polled} simulate={view === "portals"} />
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
