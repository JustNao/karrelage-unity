<script lang="ts">
  import { onMount } from "svelte";
  import { cursorPosition, getCurrentWindow, PhysicalPosition, PhysicalSize } from "@tauri-apps/api/window";
  import { call, listenAll } from "./lib/api";
  import { onStored, readFlag, readJSON, writeFlag, writeJSON } from "./lib/persist.svelte";
  import { ADVANCED_KEY, buttonRect, calibrate, CALIBRATION_KEY, pctText, REF, runeCentre, type FmCalibration, type FmItem } from "./lib/forgemagie";

  /**
   * Forgemagie overlay (forgemagie.html): a transparent window over the Dofus window's picture while an equipment piece is in a
   * forgemagie workshop, made hidden by the main window (lib/stores/forgemagie.svelte.ts).
   * - A button at the right end of the « Fusionner » row: shows / hides each rune's chance of critical success under its icon
   *   (remembered). Right click: calibrate again (the game's window was dragged). Before any calibration it sits at the top of
   *   the picture and starts one.
   * - Calibration: two clicks on the forgemagie window's top-left and bottom-right corners (lib/forgemagie.ts `calibrate`),
   *   kept for this size of the game's window.
   * - The mouse goes through to the game except over the button (the cursor is polled) and during the calibration.
   * - Closes itself when the item leaves the workshop (or the feature is switched off).
   * Never hidden then shown again, the click-through style first removed off screen (`warmUp`): see FightMapOverlay.svelte.
   */

  const CURSOR_POLL_MS = 50;
  /** The Dofus window is looked up again every this many cursor polls (it may have moved). */
  const PLACE_EVERY = 20;
  const WARMUP_PAINT_MS = 150;
  const OFF_SCREEN = -32000;

  const win = getCurrentWindow();
  const dpr = window.devicePixelRatio || 1;

  let item = $state.raw<FmItem | null>(null);
  /** the Dofus window's client area (physical px) the window covers */
  let placed = $state.raw<{ x: number; y: number; width: number; height: number } | null>(null);
  let saved = $state.raw<FmCalibration | null>(readJSON(CALIBRATION_KEY, null));
  /** the saved calibration, when it was taken at this size of the game's window */
  const cal = $derived(saved && placed && saved.width === placed.width && saved.height === placed.height ? saved : null);
  let advanced = $state(readFlag(ADVANCED_KEY, true));
  let calibrating = $state(false);
  let clicks = $state.raw<[number, number][]>([]);
  let failed = $state(false);
  let overButton = $state(false);
  let ready = $state(false);

  const button = $derived(placed ? buttonRect(cal, placed.width, dpr) : null);
  const takesInput = $derived(ready && (calibrating || overButton));

  // applied only when it changes (each call changes a window style)
  let applied = false;
  let applying = Promise.resolve();
  $effect(() => {
    const on = takesInput;
    const focus = calibrating;
    if (on === applied) return;
    applied = on;
    applying = applying
      .then(async () => {
        await win.setIgnoreCursorEvents(!on);
        await win.setFocusable(focus);
        if (focus) await win.setFocus();
        else if (!on) await call("forgemagie", "refocus", {}, true);
      })
      .catch(() => {});
  });

  /** The window put on the Dofus window's client area: that area, null without a Dofus window. */
  async function place() {
    const r = await call<{ x: number; y: number; width: number; height: number } | null>("forgemagie", "window", {}, true);
    if (r && (!placed || placed.x !== r.x || placed.y !== r.y || placed.width !== r.width || placed.height !== r.height)) {
      await win.setPosition(new PhysicalPosition(r.x, r.y));
      await win.setSize(new PhysicalSize(r.width, r.height));
      placed = r;
    }
    return r;
  }

  /** On screen once, letting everything through; the click-through style first taken off off screen (that flashes white). */
  async function warmUp() {
    await win.setIgnoreCursorEvents(true);
    await win.setFocusable(false).catch(() => {});
    const r = await place();
    await win.setPosition(new PhysicalPosition(OFF_SCREEN, OFF_SCREEN));
    await win.show();
    await win.setIgnoreCursorEvents(false);
    await new Promise((res) => setTimeout(res, WARMUP_PAINT_MS));
    await win.setIgnoreCursorEvents(true);
    if (r) await win.setPosition(new PhysicalPosition(r.x, r.y));
    void call("forgemagie", "refocus", {}, true).catch(() => {});
  }

  let closing = false;
  async function close() {
    if (closing) return;
    closing = true;
    await win.close();
  }

  function pressButton(e: MouseEvent) {
    e.preventDefault();
    if (!cal || e.button === 2) return startCalibration();
    advanced = !advanced;
    writeFlag(ADVANCED_KEY, advanced);
  }

  function startCalibration() {
    [calibrating, clicks, failed] = [true, [], false];
  }

  function calibrationClick(e: MouseEvent) {
    if (e.button !== 0 || !placed) return;
    const next = [...clicks, [e.clientX * dpr, e.clientY * dpr] as [number, number]];
    if (next.length < 2) return void (clicks = next);
    const c = calibrate(next[0], next[1], placed.width, placed.height);
    clicks = [];
    failed = !c;
    if (!c) return;
    writeJSON(CALIBRATION_KEY, c);
    saved = c;
    [calibrating, advanced] = [false, true];
    writeFlag(ADVANCED_KEY, true);
  }

  onMount(() => {
    const off = listenAll({ "forgemagie:item": (i: FmItem | null) => (i ? (item = i) : void close()) });
    // the calibration reset from the Automation tab
    const offStored = onStored<FmCalibration>(CALIBRATION_KEY, (c) => (saved = c));
    let on = true;
    let timer = 0;
    let ticks = 0;
    const tick = async () => {
      if (!on) return;
      if (++ticks % PLACE_EVERY === 0) await place().catch(() => null);
      const p = await cursorPosition().catch(() => null);
      const b = button;
      overButton = !!(p && placed && b && p.x - placed.x >= b.x && p.x - placed.x <= b.x + b.w && p.y - placed.y >= b.y && p.y - placed.y <= b.y + b.h);
      timer = setTimeout(tick, CURSOR_POLL_MS);
    };
    (async () => {
      await off.ready;
      item = await call<FmItem | null>("forgemagie", "state", {}, true);
      if (!item) return void close();
      await warmUp();
      ready = true;
      void tick();
    })().catch(() => void close());
    const esc = (e: KeyboardEvent) => e.key === "Escape" && calibrating && (calibrating = false);
    window.addEventListener("keydown", esc);
    return () => {
      on = false;
      clearTimeout(timer);
      off();
      offStored();
      window.removeEventListener("keydown", esc);
    };
  });

  const color = (sc: number) => (sc >= 50 ? "good" : sc >= 15 ? "mid" : "low");
  /** physical px -> CSS px */
  const css = (v: number) => v / dpr;
</script>

{#if ready && placed}
  <div class="overlay">
    {#if calibrating}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="calibration" onclick={calibrationClick}>
        <div class="popup">
          <b>Calibration de la forgemagie</b>
          <p>
            {#if clicks.length === 0}1/2 : cliquez sur le <b>coin haut-gauche</b> de la fenêtre de forgemagie.
            {:else}2/2 : cliquez sur son <b>coin bas-droit</b>.{/if}
          </p>
          {#if failed}<p class="bad">Ces deux clics ne ressemblent pas à la fenêtre de forgemagie : recommencez.</p>{/if}
          <p class="mut">À refaire si vous déplacez la fenêtre ou changez l'échelle de l'interface (clic droit sur le bouton). Échap pour annuler.</p>
        </div>
      </div>
    {:else if advanced && cal && item}
      {@const fs = Math.max(9, 10 * cal.sy)}
      {#each item.lines.slice(0, REF.rows) as line, row (line.stat)}
        {#each line.runes as rune, col (rune.gid)}
          {@const [x, y] = runeCentre(cal, row, col)}
          <div class="odds {rune.odds ? color(rune.odds.sc) : 'refused'}" style:left="{css(x)}px" style:top="{css(y + (REF.rune.size / 2 - 3) * cal.sy)}px" style:font-size="{css(fs)}px">
            {rune.odds ? pctText(rune.odds.sc) : "✕"}
          </div>
        {/each}
      {/each}
    {/if}
    {#if button && !calibrating}
      <button
        class="fm"
        class:on={advanced && cal}
        style:left="{css(button.x)}px"
        style:top="{css(button.y)}px"
        style:width="{css(button.w)}px"
        style:height="{css(button.h)}px"
        style:font-size="{css(Math.max(9, 10 * (cal?.sy ?? dpr)))}px"
        onmousedown={pressButton}
        oncontextmenu={(e) => e.preventDefault()}
        title={cal ? "Clic : afficher / masquer les chances de chaque rune · clic droit : recalibrer" : "Calibrer la fenêtre de forgemagie"}
      >
        {cal ? (advanced ? "Avancé ✓" : "Avancé") : "Forgemagie : calibrer"}
      </button>
    {/if}
  </div>
{/if}

<style>
  :global(html), :global(body) { background: transparent !important; }
  .overlay { position: fixed; inset: 0; font-family: var(--font); pointer-events: none; }
  .calibration { position: absolute; inset: 0; background: rgba(0, 0, 0, 0.18); cursor: crosshair; pointer-events: auto; }
  .popup { position: absolute; left: 50%; top: 12%; transform: translateX(-50%); padding: 12px 16px; max-width: 420px; text-align: center; font-size: 13px; background: var(--panel); color: var(--fg); border: 1px solid var(--line); border-radius: 8px; box-shadow: 0 6px 24px rgba(0, 0, 0, 0.4); pointer-events: none; }
  .popup p { margin: 8px 0 0; }
  .bad { color: var(--bad); }
  .odds { position: absolute; transform: translateX(-50%); padding: 0 3px; line-height: 1.15; border-radius: 3px; font-weight: 700; white-space: nowrap; background: rgba(10, 12, 24, 0.85); text-shadow: 0 1px 1px #000; }
  .odds.good { color: #6fe07a; }
  .odds.mid { color: #f2c94c; }
  .odds.low { color: #ff8a7a; }
  .odds.refused { color: #9aa; }
  .fm { position: absolute; pointer-events: auto; display: flex; align-items: center; justify-content: center; padding: 0 6px; font-weight: 600; letter-spacing: 0.04em; text-transform: uppercase; white-space: nowrap; color: #d9dbe8; background: #4a4c5e; border: 1px solid #6a6c80; border-radius: 3px; cursor: pointer; }
  .fm:hover { background: #5a5c70; }
  .fm.on { color: #fff; background: #3d6b4a; border-color: #5f9a6e; }
</style>
