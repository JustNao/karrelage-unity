<script lang="ts">
  import { onMount } from "svelte";
  import { call } from "../api";
  import { confirmGameServer } from "../stores/gameServer.svelte";
  import { settings } from "../stores/settings.svelte";
  import Modal from "./Modal.svelte";
  import GameServerSelect from "./GameServerSelect.svelte";

  /** First start (and the first start after prices became per server): the player's game server, asked once. It can't be
   *  closed without a choice; prices saved until now move to the chosen server. */
  let id = $state(settings.general.gameServer);
  let saved = $state(false);
  let busy = $state(false);
  let err = $state("");
  onMount(() => void call<{ unconfirmed: number | null }>("prices", "server", {}, true).then((r) => (saved = r.unconfirmed != null)).catch(() => {}));

  async function ok() {
    busy = true;
    err = "";
    try {
      await confirmGameServer(id);
    } catch (e) {
      err = String(e);
      busy = false;
    }
  }
</script>

<Modal title="Sur quel serveur jouez-vous ?" closable={false} onclose={() => {}}>
  <p class="mut">
    Les prix dépendent du serveur : Karrelage les garde séparément pour chacun. Vous pourrez en changer dans Paramètres « Général ».
    {#if saved}<br />Les prix déjà enregistrés seront rangés sur ce serveur.{/if}
  </p>
  <div class="ctl">
    <GameServerSelect value={id} onchange={(v) => (id = v)} />
    <button onclick={ok} disabled={busy}>Valider</button>
  </div>
  {#if err}<p class="bad">{err}</p>{/if}
</Modal>

<style>
  p { margin: 0; font-size: 12px; line-height: 1.5; }
  .ctl { display: flex; gap: 8px; align-items: center; justify-content: flex-end; }
</style>
