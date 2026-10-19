<script lang="ts">
  import { onMount, tick } from "svelte";
  import { call, on } from "../../lib/api";
  import { kamas } from "../../lib/fmt";
  import { norm } from "../../lib/effects";
  import Grip from "../../lib/components/Grip.svelte";
  import PanelHead from "../../lib/components/PanelHead.svelte";
  import SearchBar from "../../lib/components/SearchBar.svelte";
  import SideList from "../../lib/components/SideList.svelte";
  import { Columns } from "../../lib/columns.svelte";
  import ItemCard from "../../lib/components/ItemCard.svelte";
  import JsonTree from "../../lib/components/JsonTree.svelte";
  import VirtualList from "../../lib/components/VirtualList.svelte";

  const MAXROWS = 2000;
  /** Packets kept in memory: a ring buffer (the oldest is overwritten), so a packet costs O(1) instead of copying the list. */
  const CAP = MAXROWS * 2;
  const ring: any[] = new Array(CAP);
  let head = 0; // next write position
  let size = 0;
  /** Packets per type name, kept up to date as packets come in and fall out of the ring. */
  const counts = new Map<string, number>();
  /** Bumped once per batch of arrivals (at most once per frame): everything derived from the ring re-reads it. */
  let version = $state(0);
  let hidden = $state(new Set<string>());
  let onlyType = $state<string | null>(null);
  let sel = $state<number | null>(null);
  let q = $state("");
  let fdir = $state("");
  let fkind = $state("");
  let follow = $state(true);
  let listEl: HTMLElement | undefined = $state();
  let editingName = $state(false);
  let nameDraft = $state("");

  // resizable panels: types | list (flexible) | formatted | detail
  const cols = new Columns("karrelage.cols.packets", { types: 170, fmt: 420, detail: 480 });

  const nm = (p: any) => p.name || p.type || "?";
  const fmtT = (t: number) => {
    const d = new Date(t * 1000);
    return d.toTimeString().slice(0, 8) + "." + String(d.getMilliseconds()).padStart(3, "0");
  };

  /** One-line summary of a packet, computed once per packet (packets are never mutated; a rename reloads them). */
  const summaries = new WeakMap<object, string>();
  function summary(p: any): string {
    let s = summaries.get(p);
    if (s === undefined) {
      const f = p.fmt;
      if (f?.kind === "auction_items") {
        const mins = f.rows.map((r: any) => r.prices[0]?.price).filter((x: number) => x > 0);
        s = `${f.title || "?"} · ${f.rows.length} lots` + (mins.length ? ` · min ${kamas(Math.min(...mins))}` : "");
      } else {
        const o = p.decoded ?? p.body ?? p.unparsed;
        const j = o == null ? "" : JSON.stringify(o, (k, v) => (k.startsWith("__") ? undefined : v));
        s = j.length > 300 ? j.slice(0, 300) + "…" : j;
      }
      summaries.set(p, s);
    }
    return s;
  }
  /** Normalized (accent / case-insensitive) text the filter box searches in, cached like the summary. */
  const haystacks = new WeakMap<object, string>();
  const haystack = (p: any) => {
    let h = haystacks.get(p);
    if (h === undefined) haystacks.set(p, (h = norm(nm(p) + " " + (p.type ?? "") + " " + summary(p))));
    return h;
  };

  function push(p: any) {
    if (size === CAP) {
      const k = nm(ring[head]);
      const n = (counts.get(k) ?? 1) - 1;
      if (n > 0) counts.set(k, n);
      else counts.delete(k);
    } else size++;
    ring[head] = p;
    head = (head + 1) % CAP;
    counts.set(nm(p), (counts.get(nm(p)) ?? 0) + 1);
  }
  function reset(list: any[] = []) {
    ring.fill(undefined);
    head = size = 0;
    counts.clear();
    for (const p of list.slice(-CAP)) push(p);
    version++;
  }

  /** The ring, oldest first. */
  const packets = $derived.by(() => {
    version;
    const out = new Array(size);
    const start = (head - size + CAP) % CAP;
    for (let i = 0; i < size; i++) out[i] = ring[(start + i) % CAP];
    return out;
  });
  const visible = $derived.by(() => {
    const needle = norm(q.trim());
    const out = packets.filter((p) => {
      if (hidden.has(nm(p)) || (onlyType && nm(p) !== onlyType)) return false;
      if ((fdir && p.dir !== fdir) || (fkind && p.kind !== fkind)) return false;
      return !needle || haystack(p).includes(needle);
    });
    return out.length > MAXROWS ? out.slice(-MAXROWS) : out;
  });
  const typeStats = $derived.by(() => {
    version;
    return [...counts].sort((a, b) => b[1] - a[1]);
  });
  const current = $derived(sel == null ? undefined : packets.find((p) => p.id === sel));

  /** Hex of the selected packet: sent with it, or fetched on demand when the backend leaves it out. */
  let hex = $state("");
  $effect(() => {
    const c = current;
    hex = c?.hex ?? "";
    if (c && c.hex == null) {
      const id = c.id;
      call<string>("packets", "hex", { id })
        .then((h) => current?.id === id && (hex = h ?? ""))
        .catch(() => {});
    }
  });

  // arrivals are queued and applied once per frame: a burst of packets costs one update, not one per packet
  let queue: any[] = [];
  let frame = 0;
  function add(p: any | any[]) {
    if (Array.isArray(p)) queue.push(...p);
    else queue.push(p);
    if (queue.length > CAP) queue = queue.slice(-CAP); // window hidden (no frames): keep only what the ring could hold
    if (!frame) frame = requestAnimationFrame(flush);
  }
  function flush() {
    frame = 0;
    const batch = queue;
    queue = [];
    for (const p of batch) push(p);
    version++;
    if (follow) tick().then(() => listEl && (listEl.scrollTop = listEl.scrollHeight));
  }

  onMount(() => {
    const offs: Promise<() => void>[] = [on("packets:packet", add), on("packets:batch", add)];
    // the backend only records while this tab is open, and drops its buffer when it closes
    call("packets", "set_active", { on: true })
      .then(() => call<any[]>("packets", "list"))
      .then((l) => reset(l));
    return () => {
      cancelAnimationFrame(frame);
      call("packets", "set_active", { on: false });
      offs.forEach((o) => o.then((f) => f()));
    };
  });

  function clickType(k: string, e: MouseEvent) {
    if (e.shiftKey) {
      const h = new Set(hidden);
      h.has(k) ? h.delete(k) : h.add(k);
      hidden = h;
    } else onlyType = onlyType === k ? null : k;
  }

  async function clear() {
    await call("packets", "clear");
    reset();
  }

  // ---- editable names: the backend re-applies them to its whole buffer, so we just reload it
  async function reload() {
    reset(await call<any[]>("packets", "list"));
  }
  async function renameField(msg: string, field: string, name: string) {
    await call("packets", "rename", { msg, field, name });
    await reload();
  }
  async function renameMessage() {
    const t = current?.decoded?.__t;
    editingName = false;
    if (!t) return;
    await call("packets", "rename", { msg: t, name: nameDraft.trim() });
    await reload();
  }
</script>

<div class="bar">
  <SearchBar bind:value={q} placeholder="Filtrer (nom, champ, valeur)…" width="240px" compact>
    {#snippet after()}
      <select bind:value={fdir}><option value="">Les deux sens</option><option>C-&gt;S</option><option>S-&gt;C</option></select>
      <select bind:value={fkind}><option value="">Tous types</option><option value="event">événement</option><option value="request">requête</option><option value="response">réponse</option></select>
      <label><input type="checkbox" bind:checked={follow} /> Suivre</label>
      <button onclick={clear}>Vider</button>
      <span class="mut">{visible.length} affichés / {packets.length} en mémoire</span>
    {/snippet}
  </SearchBar>
</div>
<div class="grid cgrid" class:anim={!cols.dragging} style:grid-template-columns={`${cols.px("types")}px 5px 1fr 5px ${current?.fmt ? cols.w.fmt + "px 5px " : ""}${cols.px("detail")}px`}>
  <SideList {cols} name="types" title="Types">
    {#each typeStats as [k, c] (k)}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="irow ty" class:hid={hidden.has(k)} class:sel={onlyType === k} title="clic : seulement celui-ci · maj+clic : masquer" onclick={(e) => clickType(k, e)}>
        <span>{k}</span><span class="mut">{c}</span>
      </div>
    {/each}
  </SideList>
  <Grip {cols} name="types" dir={1} />
  <div class="list" bind:this={listEl}>
    <VirtualList items={visible} key={(p) => p.id} estimate={23} overscan={20}>
      {#snippet row(p)}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <div class="tr" class:sel={sel === p.id} onclick={() => (sel = p.id)}>
          <div class="td t">{fmtT(p.t)}</div>
          <div class="td d {p.dir === 'C->S' ? 'c2s' : 's2c'}">{p.dir === "C->S" ? "→ S" : "← S"}</div>
          <div class="td k">{p.kind ?? ""}</div>
          <div class="td n">{nm(p)}{#if p.name && p.type && p.name !== p.type}<span class="badge {p.conf ?? 'none'}" title="obfusqué : {p.type}">{p.type} · {p.conf ?? "none"}</span>{/if}</div>
          <div class="td">{summary(p)}</div>
        </div>
      {/snippet}
    </VirtualList>
  </div>
  <Grip {cols} name={current?.fmt ? "fmt" : "detail"} dir={-1} />
  {#if current?.fmt}
    <div class="fmt">
      {#if current.fmt.kind === "auction_items"}
        <div class="fh">{current.fmt.title ?? "(objet inconnu)"} <span class="mut">{current.fmt.type ?? ""}{current.fmt.gid ? " · gid " + current.fmt.gid : ""}</span></div>
        {#each current.fmt.rows as r}<ItemCard row={r} />{:else}<div class="mut">aucun lot</div>{/each}
      {:else if current.fmt.kind === "error"}
        <div>erreur du formateur : {current.fmt.error}</div>
      {:else}
        <pre><JsonTree v={current.fmt} /></pre>
      {/if}
    </div>
  {/if}
  {#if current?.fmt}<Grip {cols} name="detail" dir={-1} />{/if}
  <div class="detail" class:cz={cols.collapsed.detail}>
    <PanelHead {cols} name="detail" title="Détail" dir={-1} />
    {#if !current}
      <span class="mut">Sélectionnez un paquet.</span>
    {:else}
      <div>
        {#if editingName}
          <!-- svelte-ignore a11y_autofocus -->
          <input class="ren" bind:value={nameDraft} placeholder="nom du message" autofocus onkeydown={(e) => (e.key === "Enter" ? renameMessage() : e.key === "Escape" && (editingName = false))} onblur={() => (editingName = false)} />
        {:else}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <b class:ed={!!current.decoded?.__t} title="cliquer pour renommer ce message" onclick={() => current.decoded?.__t && ((nameDraft = current.name === current.type ? "" : current.name), (editingName = true))}>{nm(current)}</b>
        {/if}
        <span class="mut">{current.type && current.type !== current.name ? `(${current.type})` : ""}</span>
      </div>
      <div class="mut">{current.dir} · {current.kind ?? "?"} · uid {current.uid ?? "-"} · {current.len} octets · {current.srv}{current.conf ? " · correspondance : " + current.conf : ""}</div>
      <h3>{current.decoded ? "Décodé (noms associés ; « a (x|y?) » = candidats ambigus)" : "Sans schéma"}</h3>
      <pre><JsonTree v={current.decoded ?? current.body ?? current.unparsed} onrename={renameField} /></pre>
      <h3>Hexadécimal brut</h3>
      <pre class="mut">{hex.replace(/(..)/g, "$1 ")}</pre>
    {/if}
  </div>
</div>

<style>
  .bar { display: flex; gap: 10px; align-items: center; padding: 6px 12px; border-bottom: 1px solid var(--line); }
  .grid { flex: 1; min-height: 0; display: grid; }
  .detail, .fmt { background: var(--panel); overflow: auto; }
  .detail { border-left: 1px solid var(--line); padding: 10px; }
  .fmt { border-left: 1px solid var(--line); padding: 10px; }
  /* compact rows: many message types */
  .irow.ty { padding: 3px 8px; display: flex; justify-content: space-between; gap: 6px; }
  .irow.ty.hid { opacity: 0.35; text-decoration: line-through; }
  .irow.ty span:first-child { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .list { overflow: auto; }
  /* rows are divs (the list is virtualized): same columns as the former table */
  .tr { display: grid; grid-template-columns: 84px 44px 64px 190px minmax(0, 1fr); cursor: pointer; border-bottom: 1px solid var(--line); }
  .td { padding: 2px 8px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .td.t, .td.k { color: var(--mut); }
  .tr:hover { background: var(--panel); }
  .tr.sel { background: color-mix(in srgb, var(--acc) 22%, transparent); }
  .c2s { color: var(--c2s); }
  .s2c { color: var(--s2c); }
  .badge { font-size: 10px; padding: 0 5px; border-radius: 8px; margin-left: 6px; border: 1px solid; }
  .high { color: var(--ok); }
  .med { color: var(--warn); }
  .manual, .ref { color: var(--acc); }
  .none { color: var(--mut); }
  b.ed { cursor: pointer; border-bottom: 1px dotted var(--mut); }
  .ren { width: 240px; }
</style>
