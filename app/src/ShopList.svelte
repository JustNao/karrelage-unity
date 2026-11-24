<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onStored, readJSON, writeJSON } from "./lib/persist.svelte";
  import { SHOP_LIST_KEY, SHOP_LIST_POS_KEY, type Owned, type ShopItem, type ShopList } from "./lib/shoplist";
  import { ago, fmtInt, kamas } from "./lib/fmt";
  import FloatingShell from "./lib/components/FloatingShell.svelte";
  import { call, listenAll } from "./lib/api";

  /** Floating "to buy" list (Crafting « XP métiers »): every resource with its exact quantity; a click on a row copies its name
   *  (to paste in the auction house search), its box ticks it off. Auction house purchases made while it is open count
   *  towards each row (`purchases:bought`) and tick it off once the quantity is bought. With `stock`, what the inventory and
   *  the bank hold counts too, read live (`inventory.owned`: the bank as last seen, at its window or merged at an auction
   *  house): a row is ticked off once owned. Ticks stay (crafting then uses the resources up). Scrolls: a long plan takes
   *  hundreds of resources. */
  const EMPTY: ShopList = { title: "", buy: [], craft: [], done: [] };
  let list = $state<ShopList>(readJSON<ShopList>(SHOP_LIST_KEY, EMPTY) ?? EMPTY);
  let copied = $state<number | null>(null);
  const done = $derived(new Set(list.done ?? []));
  const left = $derived(list.buy.filter((i) => !done.has(i.gid)));
  const total = $derived(left.reduce((s, i) => s + (i.unit ?? 0) * leftOf(i), 0));

  /** inventory + bank, followed while the window is open (only for a list with `stock`) */
  let owned = $state.raw<Owned | null>(null);
  const ownedOf = (gid: number) => owned?.items[gid] ?? 0;
  const bankOf = (gid: number) => owned?.bank.items?.[gid] ?? 0;
  /** What counts towards a row: owned now, or owned when the list opened + bought since (the inventory may not be followed:
   *  app started after login), without counting a purchase twice. */
  const covered = (it: ShopItem) => (list.stock ? Math.max(ownedOf(it.gid), (list.had?.[it.gid] ?? 0) + gotOf(it.gid)) : gotOf(it.gid));
  const leftOf = (it: ShopItem) => Math.max(0, it.qty - covered(it));
  const NL = "\n";
  function stockTitle(it: ShopItem): string {
    if (!list.stock) return gotOf(it.gid) ? `Acheté : ${fmtInt(gotOf(it.gid))}` : "";
    const bank = bankOf(it.gid);
    return [`Possédé : ${fmtInt(ownedOf(it.gid))}${bank ? ` (dont ${fmtInt(bank)} en banque)` : ""}`, gotOf(it.gid) ? `Acheté depuis l'ouverture : ${fmtInt(gotOf(it.gid))}` : "", `Reste à acheter : ${fmtInt(leftOf(it))}`].filter(Boolean).join(NL);
  }
  /** The bank's age: older than a day, it may be off. */
  const STALE_S = 24 * 3600;
  const bankSeen = $derived(owned?.bank.t ?? null);
  const bankStale = $derived(bankSeen != null && Date.now() / 1000 - bankSeen > STALE_S);
  // a row owned or bought in full is ticked off (and stays ticked)
  $effect(() => {
    const full = list.buy.filter((i) => !done.has(i.gid) && i.qty > 0 && leftOf(i) === 0).map((i) => i.gid);
    if (!full.length) return;
    untrack(() => {
      list = { ...list, done: [...done, ...full] };
      writeJSON(SHOP_LIST_KEY, list);
    });
  });

  const copy = async (it: ShopItem) => {
    try { await navigator.clipboard.writeText(it.name); copied = it.gid; setTimeout(() => (copied = null), 900); } catch {}
  };
  const gotOf = (gid: number) => list.got?.[gid] ?? 0;
  /** A purchase: counted on its row (ticked off by the effect above once the quantity is there). */
  function bought(gid: number, quantity: number) {
    if (!list.buy.some((i) => i.gid === gid)) return;
    list = { ...list, got: { ...(list.got ?? {}), [gid]: gotOf(gid) + quantity } };
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
    // owned resources change with every loot or purchase: asked again once they settle
    let timer: ReturnType<typeof setTimeout> | undefined;
    const fetchOwned = () => call<Owned>("inventory", "owned", {}, true).then((o) => (owned = o)).catch(() => {});
    const offBuy = listenAll({
      "purchases:bought": (b: { gid: number; quantity: number }) => bought(b.gid, b.quantity),
      "inventory:owned": () => (clearTimeout(timer), (timer = setTimeout(fetchOwned, 500))),
    });
    void fetchOwned();
    const win = getCurrentWindow();
    const moved = win.onMoved(async ({ payload }) => {
      const p = payload.toLogical(await win.scaleFactor());
      writeJSON(SHOP_LIST_POS_KEY, { x: Math.round(p.x), y: Math.round(p.y) });
    });
    return () => (off(), offBuy(), clearTimeout(timer), void moved.then((f) => f()));
  });
</script>

<FloatingShell>
  {#snippet title()}À acheter <span class="mut">{left.length} / {list.buy.length}</span>{/snippet}
  {#if list.title}<div class="sub mut">{list.title} · reste {kamas(Math.round(total))}</div>{/if}
  {#if list.stock}
    <div class="sub mut" class:warn={bankSeen == null || bankStale} title="Inventaire et banque comptés dans chaque ligne. La banque est lue en l'ouvrant (son coffre, ou le bouton « banque » de l'hôtel de vente)">
      {#if bankSeen == null}Banque jamais vue : ouvrez-la pour compter son contenu{:else}Banque vue {ago(bankSeen)}{owned?.bank.source === "hdv" ? " (HDV)" : ""}{bankStale ? " : ouvrez-la pour la relire" : ""}{/if}
    </div>
  {/if}
  {#each list.buy as it (it.gid)}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="row" class:done={done.has(it.gid)} onclick={() => copy(it)} title={["Cliquer pour copier le nom", stockTitle(it)].filter(Boolean).join(NL)}>
      <input type="checkbox" checked={done.has(it.gid)} onclick={(e) => (e.stopPropagation(), tick(it.gid))} title="Acheté" />
      {#if covered(it) && !done.has(it.gid)}
        <span class="q"><span class="got">{fmtInt(Math.min(it.qty, covered(it)))}</span>/{fmtInt(it.qty)}</span>
      {:else}
        <span class="q">{fmtInt(it.qty)}×</span>
      {/if}
      <span class="n">{it.name}</span>
      {#if copied === it.gid}<span class="ok">copié</span>{:else if it.unit !== null}<span class="p mut">{kamas(Math.round(it.unit * (done.has(it.gid) ? it.qty : leftOf(it))))}</span>{:else}<span class="p bad">sans prix</span>{/if}
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
  .warn { color: var(--warn, #d8a040); }
  .empty { padding: 12px; text-align: center; }
</style>
