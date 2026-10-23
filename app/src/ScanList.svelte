<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { listenAll } from "./lib/api";
  import { onStored, readJSON, writeJSON } from "./lib/persist.svelte";
  import { SCAN_LIST_KEY, type ScanItem } from "./lib/scanlist";
  import FloatingShell from "./lib/components/FloatingShell.svelte";

  /** Floating "items to scan" list: filled by the Stuff tab (shared localStorage), items vanish as their auction listing is read. */
  let items = $state<ScanItem[]>(readJSON<ScanItem[]>(SCAN_LIST_KEY, []));
  let copied = $state<number | null>(null);

  const save = () => writeJSON(SCAN_LIST_KEY, items);
  const copy = async (it: ScanItem) => {
    try { await navigator.clipboard.writeText(it.name); copied = it.gid; setTimeout(() => (copied = null), 900); } catch {}
  };
  const close = () => getCurrentWindow().close();
  const clear = async () => {
    items = [];
    save();
    await close();
  };
  const GROUPS = [
    { id: "equip", title: "Équipement" },
    { id: "companion", title: "Familier / monture" },
  ] as const;
  const groups = $derived(GROUPS.map((g) => ({ ...g, items: items.filter((i) => (i.group ?? "equip") === g.id) })).filter((g) => g.items.length));

  onMount(() => {
    const offStored = onStored<ScanItem[]>(SCAN_LIST_KEY, (l) => (items = l ?? [])); // the Stuff tab exported a new list
    // every auction listing read by the app: that item has been scanned
    const off = listenAll({
      "auction:item": ({ gid }: { gid: number }) => {
        const n = items.length;
        items = items.filter((i) => i.gid !== gid);
        if (items.length !== n) save();
        if (n > 0 && !items.length) void close(); // everything scanned: the window goes away by itself
      },
    });
    return () => {
      offStored();
      off();
    };
  });
</script>

<FloatingShell closable={false}>
  {#snippet title()}À scanner <span class="mut">{items.length}</span>{/snippet}
  {#snippet actions()}<button class="clear" onclick={clear} title="Vider la liste et fermer la fenêtre">Vider</button>{/snippet}
  {#each groups as g (g.id)}
    <div class="grp">{g.title} <span class="mut">{g.items.length}</span></div>
    {#each g.items as it (it.gid)}
      <div class="row">
        <span class="n" title={it.name}>{it.name}</span>
        <button onclick={() => copy(it)}>{copied === it.gid ? "Copié" : "Copier"}</button>
      </div>
    {/each}
  {:else}
    <div class="mut done">Tout est scanné ✓</div>
  {/each}
</FloatingShell>

<style>
  .grp { padding: 5px 8px 3px; font-weight: 600; font-size: 11px; letter-spacing: 0.04em; text-transform: uppercase; color: var(--mut); background: rgba(20, 22, 26, 0.45); }
  .row { display: flex; align-items: center; gap: 6px; padding: 4px 8px; border-top: 1px solid rgba(255, 255, 255, 0.07); }
  .n { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  button { padding: 1px 7px; font-size: 11px; background: rgba(20, 22, 26, 0.8); }
  .clear { color: var(--bad); border-color: var(--bad); }
  .done { padding: 12px; text-align: center; }
</style>
