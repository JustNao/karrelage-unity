<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onStored, readJSON, writeJSON } from "./lib/persist.svelte";
  import { SHOP_LIST_KEY, SHOP_LIST_POS_KEY, type ShopItem, type ShopList } from "./lib/shoplist";
  import { fmtInt, kamas } from "./lib/fmt";
  import FloatingShell from "./lib/components/FloatingShell.svelte";
  import { listenAll } from "./lib/api";

  /** Floating "to buy" list (Crafting « XP métiers »): every resource with its exact quantity; a click on a row copies its name
   *  (to paste in the auction house search), its box ticks it off. Auction house purchases made while it is open count
   *  towards each row (`purchases:bought`) and tick it off once the quantity is bought. Scrolls: a long plan takes hundreds
   *  of resources. */
  const EMPTY: ShopList = { title: "", buy: [], craft: [], done: [] };
  let list = $state<ShopList>(readJSON<ShopList>(SHOP_LIST_KEY, EMPTY) ?? EMPTY);
  let copied = $state<number | null>(null);
  const done = $derived(new Set(list.done ?? []));
  const left = $derived(list.buy.filter((i) => !done.has(i.gid)));
  const total = $derived(left.reduce((s, i) => s + (i.unit ?? 0) * Math.max(0, i.qty - (list.got?.[i.gid] ?? 0)), 0));

  const copy = async (it: ShopItem) => {
    try { await navigator.clipboard.writeText(it.name); copied = it.gid; setTimeout(() => (copied = null), 900); } catch {}
  };
  const gotOf = (gid: number) => list.got?.[gid] ?? 0;
  /** A purchase: counted on its row, ticked off once the whole quantity is bought. */
  function bought(gid: number, quantity: number) {
    const it = list.buy.find((i) => i.gid === gid);
    if (!it) return;
    const got: Record<string, number> = { ...(list.got ?? {}), [gid]: gotOf(gid) + quantity };
    const d = new Set(done);
    if (got[gid] >= it.qty) d.add(gid);
    list = { ...list, got, done: [...d] };
    writeJSON(SHOP_LIST_KEY, list);
  }
  function tick(gid: number) {
    const d = new Set(done);
    d.has(gid) ? d.delete(gid) : d.add(gid);
    list = { ...list, done: [...d] };
    writeJSON(SHOP_LIST_KEY, list);
  }

  onMount(() => {
    const off = onStored<ShopList>(SHOP_LIST_KEY, (l) => (list = l ?? EMPTY));
    const offBuy = listenAll({ "purchases:bought": (b: { gid: number; quantity: number }) => bought(b.gid, b.quantity) });
    const win = getCurrentWindow();
    const moved = win.onMoved(async ({ payload }) => {
      const p = payload.toLogical(await win.scaleFactor());
      writeJSON(SHOP_LIST_POS_KEY, { x: Math.round(p.x), y: Math.round(p.y) });
    });
    return () => (off(), offBuy(), void moved.then((f) => f()));
  });
</script>

<FloatingShell>
  {#snippet title()}À acheter <span class="mut">{left.length} / {list.buy.length}</span>{/snippet}
  {#if list.title}<div class="sub mut">{list.title} · reste {kamas(Math.round(total))}</div>{/if}
  {#each list.buy as it (it.gid)}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="row" class:done={done.has(it.gid)} onclick={() => copy(it)} title="Cliquer pour copier le nom">
      <input type="checkbox" checked={done.has(it.gid)} onclick={(e) => (e.stopPropagation(), tick(it.gid))} title="Acheté" />
      {#if gotOf(it.gid) && !done.has(it.gid)}
        <span class="q" title="Acheté / à acheter"><span class="got">{fmtInt(gotOf(it.gid))}</span>/{fmtInt(it.qty)}</span>
      {:else}
        <span class="q">{fmtInt(it.qty)}×</span>
      {/if}
      <span class="n">{it.name}</span>
      {#if copied === it.gid}<span class="ok">copié</span>{:else if it.unit !== null}<span class="p mut">{kamas(Math.round(it.unit * it.qty))}</span>{:else}<span class="p bad">sans prix</span>{/if}
    </div>
  {:else}
    <div class="mut empty">Rien à acheter.</div>
  {/each}
  {#if list.craft.length}
    <div class="grp">À crafter en chemin <span class="mut">{list.craft.length}</span></div>
    {#each list.craft as it (it.gid)}
      <div class="row"><span class="q">{fmtInt(it.qty)}×</span><span class="n">{it.name}</span></div>
    {/each}
  {/if}
</FloatingShell>

<style>
  .sub { padding: 5px 10px; font-size: 11px; }
  .grp { padding: 5px 8px 3px; font-weight: 600; font-size: 11px; letter-spacing: 0.04em; text-transform: uppercase; color: var(--mut); background: rgba(20, 22, 26, 0.45); }
  .row { display: flex; align-items: center; gap: 6px; padding: 3px 8px; border-top: 1px solid rgba(255, 255, 255, 0.07); cursor: pointer; }
  .row:hover { background: rgba(255, 255, 255, 0.04); }
  .row.done { opacity: 0.45; }
  .row.done .n { text-decoration: line-through; }
  .row input { margin: 0; }
  .q { flex: none; min-width: 46px; text-align: right; font-weight: 600; font-variant-numeric: tabular-nums; }
  .n { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .p { flex: none; font-variant-numeric: tabular-nums; }
  .ok { color: var(--ok); font-size: 10px; }
  .got { color: var(--ok); }
  .bad { color: var(--bad); }
  .empty { padding: 12px; text-align: center; }
</style>
