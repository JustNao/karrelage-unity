<script lang="ts">
  import { call } from "../../lib/api";
  import type { Columns } from "../../lib/columns.svelte";
  import { className, type Build } from "../../lib/stuff";
  import ConfirmButton from "../../lib/components/ConfirmButton.svelte";
  import SideList from "../../lib/components/SideList.svelte";
  import SearchBar, { matcher } from "../../lib/components/SearchBar.svelte";
  import Modal from "../../lib/components/Modal.svelte";
  import { inventory } from "../../lib/stores/inventory.svelte";
  import { dofusbookLink } from "../../lib/dofusbookExport";

  /** Stuff left panel: the imported builds (filtered by name; "Ajouter" asks for a Dofusbook link in a modal), renamed in place, deleted with a confirmation; export of the
   *  character's worn equipment to Dofusbook. "Builds des joueurs": the builds read from other players' cards in game (one per player, newest first). */
  let {
    cols,
    builds = $bindable(),
    sel = $bindable(),
    err = $bindable(),
    onselect,
  }: { cols: Columns<"left">; builds: Build[]; sel: string | null; err: string; onselect: (id: string) => Promise<void> } = $props();

  let q = $state("");
  const shown = $derived.by(() => { const m = matcher(q); return builds.filter((b) => m(b.name)); });
  const mine = $derived(shown.filter((b) => !b.player));
  const players = $derived(shown.filter((b) => b.player).sort((a, b) => b.player!.seen - a.player!.seen));
  let adding = $state(false);
  let addErr = $state("");
  let link = $state("");
  let busy = $state(false);
  let renaming = $state<string | null>(null);
  let draft = $state("");

  async function add() {
    if (!link.trim() || busy) return;
    busy = true; addErr = "";
    try {
      const b = await call<Build>("stuff", "import", { url: link });
      builds = [...builds, b];
      link = "";
      adding = false;
      await onselect(b.id);
    } catch (e) { addErr = String(e); }
    busy = false;
  }
  const worn = $derived(inventory.items.filter((i) => i.equipped));
  /** Export link shown in its modal (null = closed). */
  let exported = $state<string | null>(null);
  let copied = $state<boolean | null>(false); // null = the clipboard refused
  const exportWorn = () => ((exported = dofusbookLink(worn, inventory.character)), (copied = false));
  async function copyExport() {
    try { await navigator.clipboard.writeText(exported!); copied = true; } catch { copied = null; }
  }
  const select = (n: HTMLInputElement) => { n.focus(); n.select(); };
  const focus = (n: HTMLElement) => n.focus();
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
    <SearchBar bind:value={q} placeholder="Rechercher un build…" grow compact>
      {#snippet after()}<button class="mini" onclick={() => ((adding = true), (addErr = ""))}>Ajouter</button>{/snippet}
    </SearchBar>
  {/snippet}
  <div class="exp">
    <button class="mini" onclick={exportWorn} disabled={!worn.length}
      title={worn.length ? "Lien d'un stuff Dofusbook avec les objets portés (jets max + exos en forgemagie, caractéristiques du personnage ; niveau 200)" : "Équipement inconnu : il est lu à la connexion du personnage"}>
      Exporter mon équipement vers Dofusbook</button>
  </div>
  {#if err}<div class="bad err">{err}</div>{/if}
  {#each mine as b, i (b.id)}
    {@render row(b, i)}
  {:else}
    <div class="mut empty">{builds.some((b) => !b.player) ? "Aucun stuff ne correspond au filtre." : "« Ajouter » charge un stuff depuis un lien Dofusbook."}</div>
  {/each}
  <div class="sec">Builds des joueurs</div>
  {#each players as b, i (b.id)}
    {@render row(b, i)}
  {:else}
    <div class="mut empty">{builds.some((b) => b.player) ? "Aucun build ne correspond au filtre." : "Clique sur un joueur en jeu et ouvre l'équipement de sa fiche : son build arrive ici (jets exacts)."}</div>
  {/each}
</SideList>

{#snippet row(b: Build, i: number)}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="irow brow" class:first={i === 0} class:sel={b.id === sel} onclick={() => renaming !== b.id && onselect(b.id)}>
    {#if renaming === b.id}
      <input class="rn" bind:value={draft} onclick={(e) => e.stopPropagation()} onkeydown={(e) => { if (e.key === "Enter") rename(b); if (e.key === "Escape") renaming = null; }} onblur={() => rename(b)} />
    {:else}
      <span class="nm"><b>{b.name}</b><br /><span class="mut">niv. {b.level}{b.player && b.class ? ` · ${className(b.class)}` : ""} · {Object.keys(b.slots).length} objets</span></span>
      <button class="mini" title="Renommer" onclick={(e) => { e.stopPropagation(); renaming = b.id; draft = b.name; }}>✎</button>
      <ConfirmButton mini label="✕" confirmLabel="Supprimer ?" title="Supprimer ce stuff" onconfirm={() => del(b)} />
    {/if}
  </div>
{/snippet}

{#if exported}
  <Modal title="Exporter mon équipement vers Dofusbook" width="520px" onclose={() => (exported = null)}>
    <div class="mut small">Lien à ouvrir dans le navigateur : Dofusbook y crée un stuff avec les objets portés (jets max, exos en forgemagie ; {inventory.character ? "points de caractéristiques et parchemins du personnage" : "caractéristiques pas encore lues : points à 0, parchemins 100"} ; niveau 200).</div>
    <input class="link" readonly value={exported} use:select />
    {#if copied === null}<div class="bad small">Copie refusée : le lien est sélectionné, Ctrl+C pour le copier.</div>{/if}
    <div class="acts">
      <button class="mini" onclick={() => (exported = null)}>Fermer</button>
      <button class="mini" onclick={copyExport}>{copied ? "Copié ✓" : "Copier"}</button>
    </div>
  </Modal>
{/if}

{#if adding}
  <Modal title="Ajouter un stuff Dofusbook" onclose={() => !busy && (adding = false)}>
    <input class="link" type="search" placeholder="Lien Dofusbook (dofusbook.net/…/equipement/…, d-bk.net/…)" bind:value={link} use:focus onkeydown={(e) => e.key === "Enter" && add()} />
    {#if addErr}<div class="bad small">{addErr}</div>{/if}
    <div class="acts">
      <button class="mini" onclick={() => (adding = false)} disabled={busy}>Annuler</button>
      <button class="mini" onclick={add} disabled={busy || !link.trim()}>{busy ? "…" : "Ajouter"}</button>
    </div>
  </Modal>
{/if}

<style>
  /* builds list (SideList, same look as the HDV item list) */
  .link { width: 100%; box-sizing: border-box; padding: 5px 8px; }
  .acts { display: flex; justify-content: flex-end; gap: 6px; }
  .small { font-size: 12px; }
  .irow.brow { display: flex; align-items: center; gap: 4px; }
  .exp { padding: 0 10px 6px; }
  .exp button { width: 100%; }
  .err { padding: 0 10px 6px; font-size: 12px; margin-bottom: 6px; }
  .nm { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rn { flex: 1; min-width: 0; }
  .sec { padding: 10px 10px 4px; font-size: 10px; letter-spacing: 0.08em; text-transform: uppercase; color: var(--mut); }
</style>
