<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow, PhysicalPosition, PhysicalSize } from "@tauri-apps/api/window";
  import { call, listenAll } from "./lib/api";
  import { readJSON, writeJSON } from "./lib/persist.svelte";
  import { fight, followFight } from "./lib/stores/combat.svelte";
  import { settings } from "./lib/stores/settings.svelte";
  import { ALL_RANGE, CALIBRATION_KEY, calibrate, cellAt, centre, MOV, screenDiamond, type Calibration, type FightMapState, type MapFighter, type Sight } from "./lib/fightMap";

  /**
   * In-game line of sight (fightmap.html): a transparent window over the Dofus window's picture, opened by the « Ligne de vue
   * en jeu » shortcut during a fight (lib/stores/fightMap.svelte.ts).
   * - No calibration for this window size: the player clicks the centre of their character's cell, then of the monster named
   *   (lib/fightMap.ts `calibrate`), and it goes on.
   * - Hover: the window takes the mouse; the cells in line of sight of the hovered cell are drawn (the player's character
   *   counted as standing there). A click pins the cell: the window lets the mouse through, the player plays, the view follows
   *   the fighters' moves.
   * - Closed by the shortcut again, Escape, the end of the player's turn or of the fight.
   */
  const win = getCurrentWindow();
  const dpr = window.devicePixelRatio || 1;

  let live = $state.raw<FightMapState>({ map: null, fight: true, fighters: [] });
  let cells = $state.raw<number[] | null>(null);
  let cal = $state.raw<Calibration | null>(null);
  let size = { width: 0, height: 0 };
  let mode = $state<"loading" | "calibrate" | "hover" | "pinned">("loading");
  /** calibration clicks so far (physical px of the client area) */
  let clicks = $state.raw<[number, number][]>([]);
  let failed = $state(false);
  /** the mouse during the calibration (CSS px): a cross about a cell wide is drawn there */
  let pointer = $state<[number, number] | null>(null);
  let hovered = $state<number | null>(null);
  let pinned = $state<number | null>(null);
  let sight = $state.raw<Sight | null>(null);

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

  /** Escape no longer watched and the game focused again first (its own shortcuts work at once), then the window goes. */
  async function close() {
    await call("fightmap", "release", {}, true).catch(() => {});
    await win.close();
  }

  onMount(() => {
    const following = followFight();
    /** whether the last turn that started was one of the player's: its end (another fighter's turn) closes the view */
    let mineTurn = false;
    const off = listenAll({
      "fightmap:state": (s: FightMapState) => (live = s),
      "fightmap:escape": () => void close(),
      "fightmap:turn": ({ id }: { id: number }) => {
        if (mine.has(id)) mineTurn = true;
        else if (mineTurn) void close();
      },
    });
    const key = (e: KeyboardEvent) => e.key === "Escape" && void close();
    window.addEventListener("keydown", key);
    (async () => {
      await off.ready;
      const [state, rect] = await Promise.all([call<FightMapState>("fightmap", "state"), call<{ x: number; y: number; width: number; height: number } | null>("fightmap", "window")]);
      live = state;
      if (!rect || !state.fight || state.map === null) return void close();
      cells = await call<number[] | null>("fightmap", "cells", { map: state.map });
      if (!cells) return void close();
      size = { width: rect.width, height: rect.height };
      await win.setPosition(new PhysicalPosition(rect.x, rect.y));
      await win.setSize(new PhysicalSize(rect.width, rect.height));
      await call("fightmap", "watch_escape", { on: true });
      const saved = readJSON<Calibration | null>(CALIBRATION_KEY, null);
      cal = saved && saved.width === rect.width && saved.height === rect.height ? saved : null;
      mode = cal ? "hover" : "calibrate";
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
    if (mode === "loading" || map === null || map === loadedMap) return;
    if (loadedMap !== null) call<number[] | null>("fightmap", "cells", { map }).then((c) => (cells = c));
    loadedMap = map;
  });

  const at = (e: MouseEvent): [number, number] => [e.clientX * dpr, e.clientY * dpr];

  function click(e: MouseEvent) {
    if (mode === "calibrate") {
      if (!targets) return;
      const next = [...clicks, at(e)];
      if (next.length < 2) return void (clicks = next);
      const c = calibrate(targets[0].cell, next[0], targets[1].cell, next[1], size.width, size.height);
      clicks = [];
      failed = !c;
      if (!c) return;
      writeJSON(CALIBRATION_KEY, c);
      cal = c;
      mode = "hover";
    } else if (mode === "hover" && hovered !== null) {
      pinned = hovered;
      mode = "pinned";
      void win.setIgnoreCursorEvents(true);
    }
  }

  /** a cell's width (CSS px) guessed before the calibration: the map is 14.5 cells wide and about 10 high, the game's bars
   *  take some of the height */
  const crossW = $derived(Math.min(window.innerWidth / 15, window.innerHeight / 12));

  function move(e: MouseEvent) {
    if (mode === "calibrate") pointer = [e.clientX, e.clientY];
    if (mode !== "hover" || !cal || !cells) return;
    const c = cellAt(cal, ...at(e));
    hovered = c !== null && cells[c] & MOV ? c : null;
  }

  // line of sight from the hovered / pinned cell, asked again when fighters move
  let asked = 0;
  $effect(() => {
    const from = pinned ?? hovered;
    void live.fighters;
    if (from === null) return void (sight = null);
    const n = ++asked;
    call<Sight | null>("fightmap", "sight", { from, ...ALL_RANGE, ignore: [...mine] }, true).then((s) => n === asked && (sight = s));
  });

  const from = $derived(pinned ?? hovered);
  const keyLabel = $derived(settings.shortcuts.los.vk ? settings.shortcuts.los.label : "le raccourci");
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="overlay" class:calibrating={mode === "calibrate"} onclick={click} onmousemove={move}>
  {#if mode === "calibrate"}
    <div class="popup">
      <b>Calage de la ligne de vue</b>
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
  {:else if cal && (mode === "hover" || mode === "pinned")}
    <svg width="100%" height="100%">
      {#each sight?.seen ?? [] as c (c)}
        <polygon points={screenDiamond(cal, c, dpr)} class="seen" />
      {/each}
      {#if from !== null}
        <polygon points={screenDiamond(cal, from, dpr)} class="from" />
      {/if}
    </svg>
    <div class="pill">
      {#if mode === "hover"}Ligne de vue : survolez une case, clic pour la figer · Échap pour fermer
      {:else}Ligne de vue figée · {keyLabel}, Échap ou la fin du tour l'effacent{/if}
    </div>
  {/if}
</div>

<style>
  :global(html), :global(body) { background: transparent !important; }
  /* nearly transparent rather than transparent: the window must still get the mouse while it is not let through */
  .overlay { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.01); font-family: var(--font); font-size: 13px; cursor: crosshair; }
  .overlay.calibrating { background: rgba(0, 0, 0, 0.18); cursor: none; }
  .cross path { stroke: #fff; stroke-width: 1; }
  .cross path.under { stroke: rgba(0, 0, 0, 0.8); stroke-width: 3; }
  svg { position: absolute; inset: 0; pointer-events: none; }
  .seen { fill: rgba(90, 169, 255, 0.35); stroke: rgba(90, 169, 255, 0.9); stroke-width: 1; }
  .from { fill: rgba(242, 194, 48, 0.45); stroke: #f2c230; stroke-width: 2; }
  .popup, .pill { position: absolute; left: 50%; transform: translateX(-50%); background: var(--panel); color: var(--fg); border: 1px solid var(--line); border-radius: 8px; box-shadow: 0 6px 24px rgba(0, 0, 0, 0.4); pointer-events: none; }
  .popup { top: 18%; padding: 12px 16px; max-width: 420px; text-align: center; }
  .popup p { margin: 8px 0 0; }
  .pill { top: 10px; padding: 4px 12px; font-size: 12px; opacity: 0.9; }
  .bad { color: var(--bad); }
</style>
