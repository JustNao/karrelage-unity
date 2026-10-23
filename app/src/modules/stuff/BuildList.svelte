<script lang="ts">
  import { call } from "../../lib/api";
  import type { Columns } from "../../lib/columns.svelte";
  import type { Build } from "../../lib/stuff";
  import ConfirmButton from "../../lib/components/ConfirmButton.svelte";
  import SideList from "../../lib/components/SideList.svelte";

  /** Stuff left panel: the imported builds (Dofusbook link to add one), renamed in place, deleted with a confirmation. */
  let {
    cols,
    builds = $bindable(),
    sel = $bindable(),
    err = $bindable(),
    onselect,
  }: { cols: Columns<"left">; builds: Build[]; sel: string | null; err: string; onselect: (id: string) => Promise<void> } = $props();

  let link = $state("");
  let busy = $state(false);
  let renaming = $state<string | null>(null);
  let draft = $state("");

  async function add() {
    if (!link.trim() || busy) return;
    busy = true; err = "";
    try {
      const b = await call<Build>("stuff", "import", { url: link });
      builds = [...builds, b];
      link = "";
      await onselect(b.id);
    } catch (e) { err = String(e); }
    busy = false;
  }
  const rename = async (b: Build) => {
    const name = draft.trim();
    renaming = null;
    if (!name || name === b.name) return;
    try { await call("stuff", "rename", { id: b.id, name }); b.name = name; } catch (e) { err = String(e); }
  };
  const del = async (b: Build) => {
    try { await call("stuff", "delete", { id: b.id }); } catch (e) { err = String(e); return; }
    builds = builds.filter((x) => x.id !== b.id);
    if (sel === b.id) sel = builds[0]?.id ?? null;
  };
</script>

<SideList {cols} name="left" title="Stuffs">
  {#snippet bar()}
    <input class="link" type="search" placeholder="Lien Dofusbook…" bind:value={link} onkeydown={(e) => e.key === "Enter" && add()} />
    <button class="mini" onclick={add} disabled={busy}>{busy ? "…" : "Ajouter"}</button>
  {/snippet}
  {#if err}<div class="bad err">{err}</div>{/if}
  {#each builds as b, i (b.id)}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="irow brow" class:first={i === 0} class:sel={b.id === sel} onclick={() => renaming !== b.id && onselect(b.id)}>
      {#if renaming === b.id}
        <input class="rn" bind:value={draft} onclick={(e) => e.stopPropagation()} onkeydown={(e) => { if (e.key === "Enter") rename(b); if (e.key === "Escape") renaming = null; }} onblur={() => rename(b)} />
      {:else}
        <span class="nm"><b>{b.name}</b><br /><span class="mut">niv. {b.level} · {Object.keys(b.slots).length} objets</span></span>
        <button class="mini" title="Renommer" onclick={(e) => { e.stopPropagation(); renaming = b.id; draft = b.name; }}>✎</button>
        <ConfirmButton mini label="✕" confirmLabel="Supprimer ?" title="Supprimer ce stuff" onconfirm={() => del(b)} />
      {/if}
    </div>
  {:else}
    <div class="mut empty">Collez un lien Dofusbook ci-dessus pour charger un stuff.</div>
  {/each}
</SideList>

<style>
  /* builds list (SideList, same look as the HDV item list) */
  .link { flex: 1; min-width: 0; padding: 3px 6px; }
  .irow.brow { display: flex; align-items: center; gap: 4px; }
  .err { padding: 0 10px 6px; font-size: 12px; margin-bottom: 6px; }
  .nm { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rn { flex: 1; min-width: 0; }
</style>
