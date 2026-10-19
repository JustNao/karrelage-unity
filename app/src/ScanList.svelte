<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { SCAN_LIST_KEY, type ScanItem } from "./lib/scanlist";

  /** Floating "items to scan" list: filled by the Stuff tab (shared localStorage), items vanish as their auction listing is read. */
  let items = $state<ScanItem[]>([]);
  let copied = $state<number | null>(null);

  const load = () => {
    try { items = JSON.parse(localStorage.getItem(SCAN_LIST_KEY) ?? "[]"); } catch { items = []; }
  };
  const save = () => { try { localStorage.setItem(SCAN_LIST_KEY, JSON.stringify(items)); } catch {} };
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
    load();
    const onStorage = (e: StorageEvent) => e.key === SCAN_LIST_KEY && load(); // the Stuff tab exported a new list
    window.addEventListener("storage", onStorage);
    // every auction listing read by the app: that item has been scanned
    const off = listen<{ gid: number }>("auction:item", (e) => {
      const n = items.length;
      items = items.filter((i) => i.gid !== e.payload.gid);
      if (items.length !== n) save();
      if (n > 0 && !items.length) void close(); // everything scanned: the window goes away by itself
    });
    return () => { window.removeEventListener("storage", onStorage); void off.then((f) => f()); };
  });
</script>

<div class="box">
  <div class="top" data-tauri-drag-region>
    <span class="t" data-tauri-drag-region>À scanner <span class="mut" data-tauri-drag-region>{items.length}</span></span>
    <button class="clear" onclick={clear} title="Vider la liste et fermer la fenêtre">Vider</button>
  </div>
  <div class="list">
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
  </div>
</div>

<style>
  .box { height: 100%; box-sizing: border-box; display: flex; flex-direction: column; background: rgba(28, 31, 38, 0.82); border: 1px solid rgba(90, 169, 255, 0.45); border-radius: 10px; overflow: hidden; font-family: var(--font); font-size: 12px; user-select: none; }
  .top { display: flex; align-items: center; gap: 6px; padding: 6px 8px; background: rgba(20, 22, 26, 0.7); cursor: move; }
  .t { flex: 1; font-weight: 600; }
  .list { flex: 1; overflow: auto; }
  .grp { padding: 5px 8px 3px; font-weight: 600; font-size: 11px; letter-spacing: 0.04em; text-transform: uppercase; color: var(--mut); background: rgba(20, 22, 26, 0.45); }
  .row { display: flex; align-items: center; gap: 6px; padding: 4px 8px; border-top: 1px solid rgba(255, 255, 255, 0.07); }
  .n { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  button { padding: 1px 7px; font-size: 11px; background: rgba(20, 22, 26, 0.8); }
  .clear { color: var(--bad); border-color: var(--bad); }
  .done { padding: 12px; text-align: center; }
</style>
