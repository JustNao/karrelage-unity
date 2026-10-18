<script lang="ts">
  import { onMount, tick } from "svelte";
  import { call, kamas, on } from "../../lib/api";
  import ItemCard from "../../lib/components/ItemCard.svelte";
  import JsonTree from "../../lib/components/JsonTree.svelte";

  const MAXROWS = 2000;
  let packets = $state.raw<any[]>([]);
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

  // Resizable panels (px): types | list | formatted | detail. The list takes what is left. Remembered between sessions.
  let widths = $state({ types: 170, fmt: 420, detail: 480 });
  try {
    Object.assign(widths, JSON.parse(localStorage.getItem("loutil.packetCols") ?? "{}"));
  } catch {}
  /** Drag a divider: `dir` = +1 when the panel is on the left of the handle (grows with the pointer), -1 when on the right. */
  function resize(key: "types" | "fmt" | "detail", dir: 1 | -1, e: PointerEvent) {
    const x0 = e.clientX, w0 = widths[key];
    const move = (ev: PointerEvent) => (widths[key] = Math.min(900, Math.max(100, w0 + dir * (ev.clientX - x0))));
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      try {
        localStorage.setItem("loutil.packetCols", JSON.stringify(widths));
      } catch {}
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    e.preventDefault();
  }

  const nm = (p: any) => p.name || p.type || "?";
  const fmtT = (t: number) => {
    const d = new Date(t * 1000);
    return d.toTimeString().slice(0, 8) + "." + String(d.getMilliseconds()).padStart(3, "0");
  };

  function summary(p: any): string {
    const f = p.fmt;
    if (f?.kind === "auction_items") {
      const mins = f.rows.map((r: any) => r.prices[0]?.price).filter((x: number) => x > 0);
      return `${f.title || "?"} · ${f.rows.length} lots` + (mins.length ? ` · min ${kamas(Math.min(...mins))}` : "");
    }
    const o = p.decoded ?? p.body ?? p.unparsed;
    if (o == null) return "";
    const s = JSON.stringify(o, (k, v) => (k.startsWith("__") ? undefined : v));
    return s.length > 300 ? s.slice(0, 300) + "…" : s;
  }

  const visible = $derived(
    packets
      .filter((p) => {
        if (hidden.has(nm(p)) || (onlyType && nm(p) !== onlyType)) return false;
        if ((fdir && p.dir !== fdir) || (fkind && p.kind !== fkind)) return false;
        const needle = q.trim().toLowerCase();
        return !needle || (nm(p) + " " + (p.type ?? "") + " " + summary(p)).toLowerCase().includes(needle);
      })
      .slice(-MAXROWS),
  );
  const typeStats = $derived.by(() => {
    const m = new Map<string, number>();
    for (const p of packets) m.set(nm(p), (m.get(nm(p)) ?? 0) + 1);
    return [...m].sort((a, b) => b[1] - a[1]);
  });
  const current = $derived(packets.find((p) => p.id === sel));

  function add(p: any) {
    packets = packets.length >= MAXROWS * 2 ? [...packets.slice(MAXROWS), p] : [...packets, p];
    if (follow) tick().then(() => listEl && (listEl.scrollTop = listEl.scrollHeight));
  }

  onMount(() => {
    const offs: Promise<() => void>[] = [on("packets:packet", add)];
    // the backend only records while this tab is open, and drops its buffer when it closes
    call("packets", "set_active", { on: true }).then(() => call<any[]>("packets", "list")).then((l) => (packets = l));
    return () => {
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
    packets = [];
  }

  // ---- editable names: the backend re-applies them to its whole buffer, so we just reload it
  async function reload() {
    packets = await call<any[]>("packets", "list");
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
  <input type="search" placeholder="Filtrer (nom, champ, valeur)…" bind:value={q} />
  <select bind:value={fdir}><option value="">Les deux sens</option><option>C-&gt;S</option><option>S-&gt;C</option></select>
  <select bind:value={fkind}><option value="">Tous types</option><option value="event">événement</option><option value="request">requête</option><option value="response">réponse</option></select>
  <label><input type="checkbox" bind:checked={follow} /> Suivre</label>
  <button onclick={clear}>Vider</button>
  <span class="mut">{visible.length} affichés / {packets.length} en mémoire</span>
</div>
<div class="grid" style:grid-template-columns={`${widths.types}px 5px 1fr 5px ${current?.fmt ? widths.fmt + "px 5px " : ""}${widths.detail}px`}>
  <div class="types">
    {#each typeStats as [k, c] (k)}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class:hid={hidden.has(k)} class:sel={onlyType === k} title="clic : seulement celui-ci · maj+clic : masquer" onclick={(e) => clickType(k, e)}>
        <span>{k}</span><span class="mut">{c}</span>
      </div>
    {/each}
  </div>
  <div class="grip" role="separator" aria-orientation="vertical" onpointerdown={(e) => resize("types", 1, e)}></div>
  <div class="list" bind:this={listEl}>
    <table>
      <tbody>
        {#each visible as p (p.id)}
          <tr class:sel={sel === p.id} onclick={() => (sel = p.id)}>
            <td class="t">{fmtT(p.t)}</td>
            <td class="d {p.dir === 'C->S' ? 'c2s' : 's2c'}">{p.dir === "C->S" ? "→ S" : "← S"}</td>
            <td class="k">{p.kind ?? ""}</td>
            <td class="n">{nm(p)}{#if p.name && p.type && p.name !== p.type}<span class="badge {p.conf ?? 'none'}" title="obfusqué : {p.type}">{p.type} · {p.conf ?? "none"}</span>{/if}</td>
            <td>{summary(p)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
  <div class="grip" role="separator" aria-orientation="vertical" onpointerdown={(e) => resize(current?.fmt ? "fmt" : "detail", -1, e)}></div>
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
  {#if current?.fmt}<div class="grip" role="separator" aria-orientation="vertical" onpointerdown={(e) => resize("detail", -1, e)}></div>{/if}
  <div class="detail">
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
      <pre class="mut">{(current.hex ?? "").replace(/(..)/g, "$1 ")}</pre>
    {/if}
  </div>
</div>

<style>
  .bar { display: flex; gap: 10px; align-items: center; padding: 6px 12px; border-bottom: 1px solid var(--line); }
  input[type="search"] { width: 240px; }
  .grid { flex: 1; min-height: 0; display: grid; }
  .grip { cursor: col-resize; background: var(--line); transition: background 0.15s; }
  .grip:hover { background: var(--acc); }
  .types, .detail, .fmt { background: var(--panel); overflow: auto; }
  .types { border-right: 1px solid var(--line); }
  .detail { border-left: 1px solid var(--line); padding: 10px; }
  .fmt { border-left: 1px solid var(--line); padding: 10px; }
  .types div { padding: 3px 8px; cursor: pointer; display: flex; justify-content: space-between; gap: 6px; border-bottom: 1px solid var(--line); }
  .types div:hover { background: var(--bg); }
  .types div.sel { outline: 1px solid var(--acc); }
  .types div.hid { opacity: 0.35; text-decoration: line-through; }
  .types span:first-child { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .list { overflow: auto; }
  table { width: 100%; border-collapse: collapse; table-layout: fixed; }
  td { padding: 2px 8px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 0; border-bottom: 1px solid var(--line); }
  td.t { width: 84px; color: var(--mut); max-width: none; }
  td.d { width: 44px; max-width: none; }
  td.k { width: 64px; color: var(--mut); max-width: none; }
  td.n { width: 190px; max-width: none; }
  tr { cursor: pointer; }
  tr:hover { background: var(--panel); }
  tr.sel { background: color-mix(in srgb, var(--acc) 22%, transparent); }
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
