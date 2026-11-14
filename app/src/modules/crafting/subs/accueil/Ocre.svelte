<script lang="ts">
  import { onMount } from "svelte";
  import { call } from "../../../../lib/api";
  import { effectivePrice } from "../../../../lib/price";
  import { prices } from "../../../../lib/stores/prices.svelte";
  import { fmtInt, kamas } from "../../../../lib/fmt";
  import { fitRows } from "../../../../lib/actions/fitRows";
  import { copyName } from "./home";
  import OCRE from "./ocre.json";

  /**
   * Accueil card: the Dofus Ocre if every soul is bought at the auction house. ocre.json = [step, soul gid]: metamob.fr's list
   * (discovery/1, 2026-11-14: 337 monsters, steps 1–3 dungeon bosses, 4–18 archimonsters) matched to the game's « Âme capturée »
   * items (types 327 / 328) by name; by hand: Gelée Royale Bleuet = « Bleue », and the souls a dungeon gives together (Blop
   * Multicolore Royal & Blops Royaux, Hanshi & Shihan, Minotot & Minotoror, Sphincter Cell & Rats, the four Tynrils in one):
   * 334 items. Price: cheapest read, else the weekly average.
   */
  const SOULS = OCRE as [number, number][];
  const BOSS_STEPS = 3;
  /** at most this many: the card shows the ones its height holds (fitRows) */
  const SHOWN = 40;

  let names = $state.raw<Record<string, string>>({});
  let err = $state("");
  let showMissing = $state(false);
  onMount(() => {
    call<Record<string, string>>("prices", "item_names", { gids: SOULS.map(([, g]) => g) }).then((n) => (names = n)).catch((e) => (err = String(e)));
  });
  const nameOf = (gid: number) => (names[gid] ?? `#${gid}`).replace(/^Âmes? capturées? : /, "");

  const rows = $derived(
    SOULS.map(([step, gid]) => {
      const p = effectivePrice(prices.map.get(gid));
      return { step, gid, price: p?.value ?? null, avg: p?.source === "moyenne" };
    }),
  );
  const priced = $derived(rows.filter((r) => r.price !== null));
  const sum = (l: typeof rows) => l.reduce((s, r) => s + (r.price ?? 0), 0);
  const total = $derived(sum(priced));
  const bosses = $derived(priced.filter((r) => r.step <= BOSS_STEPS));
  const archis = $derived(priced.filter((r) => r.step > BOSS_STEPS));
  const missing = $derived(rows.filter((r) => r.price === null));
  const dearest = $derived([...priced].sort((a, b) => b.price! - a.price!).slice(0, SHOWN));
  const fromAvg = $derived(priced.filter((r) => r.avg).length);
</script>

<div class="fill">
  {#if err}<div class="bad">{err}</div>{/if}
  <div class="total">
    <span class="big">{kamas(Math.round(total))}</span>
    <span class="mut">pour {fmtInt(priced.length)} âme{priced.length > 1 ? "s" : ""} sur {SOULS.length} avec un prix{#if fromAvg}{" "}({fmtInt(fromAvg)} au prix moyen de la semaine){/if}</span>
  </div>
  <div class="split">
    <div><span class="mut">Gardiens de donjon (étapes 1–3)</span><br /><b>{kamas(Math.round(sum(bosses)))}</b> <span class="mut">· {bosses.length} âmes</span></div>
    <div><span class="mut">Archimonstres (étapes 4–18)</span><br /><b>{kamas(Math.round(sum(archis)))}</b> <span class="mut">· {archis.length} âmes</span></div>
  </div>
  {#if showMissing && missing.length}
    <div class="sec">Sans prix</div>
    <div class="missing" use:fitRows>
      {#each missing as r (r.gid)}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <span class="chip" onclick={() => copyName(names[r.gid] ?? "")} title="Étape {r.step} · cliquer pour copier le nom">{nameOf(r.gid)}</span>
      {/each}
    </div>
  {:else if dearest.length}
    <div class="sec">Les plus chères</div>
    <div class="rows" use:fitRows>
      {#each dearest as r (r.gid)}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <div class="row" onclick={() => copyName(names[r.gid] ?? "")} title="Cliquer pour copier le nom">
          <span class="n"><b>{nameOf(r.gid)}</b> <span class="mut">étape {r.step}</span></span>
          <span class="p">{kamas(Math.round(r.price!))}{#if r.avg} <span class="mut avg">moy.</span>{/if}</span>
        </div>
      {/each}
    </div>
  {:else}
    <div class="rows"></div>
  {/if}
  {#if missing.length}
    <button class="mini more" onclick={() => (showMissing = !showMissing)}>
      {showMissing ? "Voir les plus chères" : `Voir les ${missing.length} âme${missing.length > 1 ? "s" : ""} sans prix (non comptée${missing.length > 1 ? "s" : ""})`}
    </button>
  {/if}
</div>

<style>
  .fill { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .rows { flex: 1; min-height: 0; }
  .total { flex: none; display: flex; align-items: baseline; gap: 10px; flex-wrap: wrap; margin-bottom: 8px; }
  .big { font-size: 22px; font-weight: 700; color: var(--gold); }
  .split { flex: none; display: grid; grid-template-columns: 1fr 1fr; gap: 10px; margin-bottom: 4px; }
  .sec { flex: none; margin: 8px 0 2px; font-size: 11px; font-weight: 600; letter-spacing: 0.04em; text-transform: uppercase; color: var(--mut); }
  .row { display: flex; align-items: baseline; gap: 10px; padding: 3px 0; border-top: 1px solid var(--line); cursor: pointer; }
  .row:hover .n b { text-decoration: underline; }
  .n { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .p { flex: none; font-variant-numeric: tabular-nums; }
  .avg { font-size: 10px; }
  .more { flex: none; align-self: flex-start; margin-top: 8px; }
  .missing { flex: 1; min-height: 0; display: flex; flex-wrap: wrap; align-content: flex-start; gap: 4px; margin-top: 4px; }
  .chip { padding: 1px 7px; border: 1px solid var(--line); border-radius: 10px; font-size: 11px; cursor: pointer; }
  .chip:hover { border-color: var(--acc); }
</style>
