<script lang="ts">
  import { clickOutside } from "../actions/clickOutside";
  import { app } from "../app.svelte";
  import { ago } from "../fmt";
  import { notifications, RULE_TITLES, type Notification } from "../stores/automation.svelte";

  /** Header area (right of the capture switch): hidden until a rule triggers; a pulsing green dot while some are unread;
   *  clicking opens the recent notifications under it, clicking one opens its lot in the HDV tab. */
  const POP_W = 380;
  let open = $state(false);
  let el = $state<HTMLElement | null>(null);

  /** The popup opens under the area, towards the left when there is no room on the right (narrow window). */
  let right = $state(false);
  function toggle() {
    open = !open;
    const r = el?.getBoundingClientRect();
    right = !!r && r.left + POP_W + 8 > innerWidth;
    if (open) notifications.seen();
  }
  function go(n: Notification) {
    open = false;
    app.showLot(n.gid, n.lot);
  }
</script>

{#if notifications.list.length}
  <div class="notif" bind:this={el} use:clickOutside={() => (open = false)}>
    <button class="nbtn" class:on={open} onclick={toggle} title="Règles d'automatisation déclenchées">
      {#if notifications.unread}<span class="dot"></span>{/if}
      Notifications <span class="mut">{notifications.list.length}</span>
    </button>
    {#if open}
      <div class="pop" class:right style:width="{POP_W}px">
        <div class="ph">
          <b>Notifications récentes</b>
          <button class="mini" onclick={() => ((open = false), notifications.clear())}>Tout effacer</button>
        </div>
        <div class="items">
          {#each notifications.list as n (n.id)}
            <div class="it">
              <button class="go" onclick={() => go(n)} title="Voir le lot dans l'onglet HDV">
                <span class="rule">{RULE_TITLES[n.rule]} · {ago(n.t)}</span>
                <span class="tt">{n.title}</span>
                <span class="mut">{n.detail}</span>
              </button>
              <button class="x" title="Retirer" onclick={() => notifications.remove(n.id)}>✕</button>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </div>
{/if}

<style>
  .notif { position: relative; }
  .nbtn { display: inline-flex; align-items: center; gap: 6px; padding: 3px 10px; line-height: 1.4; }
  .nbtn.on { border-color: var(--acc); }
  .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--ok); animation: pulse 1.4s ease-out infinite; }
  @keyframes pulse {
    0% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--ok) 70%, transparent); }
    100% { box-shadow: 0 0 0 7px transparent; }
  }
  .pop { position: absolute; top: calc(100% + 6px); left: 0; z-index: 50; max-width: calc(100vw - 16px); max-height: 60vh; display: flex; flex-direction: column; background: var(--panel); border: 1px solid var(--line); box-shadow: 0 4px 16px #0008; font-size: 12px; }
  .pop.right { left: auto; right: 0; }
  .ph { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 6px 10px; border-bottom: 1px solid var(--line); }
  .items { overflow: auto; }
  .it { display: flex; align-items: stretch; border-bottom: 1px solid var(--line); }
  .it:last-child { border-bottom: 0; }
  .go { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; text-align: left; border: 0; background: none; padding: 6px 10px; cursor: pointer; }
  .go:hover { background: var(--bg); }
  .rule { color: var(--ok); font-size: 10px; text-transform: uppercase; letter-spacing: 0.05em; }
  .tt { font-weight: 600; color: var(--fg); }
  .x { border: 0; background: none; color: var(--mut); padding: 0 10px; }
  .x:hover { color: var(--bad); }
</style>
