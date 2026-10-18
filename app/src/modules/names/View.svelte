<script lang="ts">
  import { onMount, tick } from "svelte";
  import { call, on } from "../../lib/api";

  interface Check { role: string; ok: boolean; detail: string }
  interface Report {
    verdict: "compatible" | "uncertain" | "incompatible";
    pack: { generated: number; messages: number; named: number; dir: string };
    game: { state: "match" | "changed" | "unknown"; detail: string; pack?: any; game?: any };
    bindings: Check[];
    stale: { count: number; examples: string[] };
    live: { types: number; messages: number; unknownTypes: number; unknownMessages: number; ratio: number; enough: boolean; unnamedTypes: number; unknown: { type: string; count: number }[] };
    canCompute: boolean;
    running: boolean;
  }

  let report = $state<Report | null>(null);
  let error = $state("");
  let log = $state.raw<string[]>([]);
  let running = $state(false);
  let finished = $state<null | boolean>(null);
  let logEl: HTMLElement | undefined = $state();
  /** Step reported by update_names.py ("@@STEP k n title" lines): k-th of n steps, "fin" = everything done. */
  let progress = $state<{ k: number; n: number; title: string } | null>(null);
  const pct = $derived(!progress ? 0 : progress.title === "fin" ? 100 : Math.round(((progress.k - 1) / progress.n) * 100));

  const VERDICT = {
    compatible: { label: "Compatible", cls: "ok" },
    uncertain: { label: "À confirmer", cls: "warn" },
    incompatible: { label: "Incompatible", cls: "bad" },
  };
  const date = (t: number) => new Date(t * 1000).toLocaleString("fr-FR");
  const num = (n: number | null | undefined) => (n == null ? "?" : n.toLocaleString("fr-FR"));

  async function refresh() {
    try {
      report = await call<Report>("names", "check");
      running = report.running || running;
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  async function compute(mode: "names" | "full") {
    if (mode === "full" && !confirm("Régénérer tout depuis les fichiers du jeu (copie, dump IL2CPP, protos, données) ? Cela peut prendre plusieurs minutes.")) return;
    log = [];
    finished = null;
    progress = { k: 1, n: mode === "full" ? 7 : 2, title: "Démarrage…" };
    try {
      await call("names", "compute", { mode });
      running = true;
    } catch (e) {
      log = [String(e)];
      finished = false;
    }
  }

  onMount(() => {
    call("names", "set_active", { on: true }).then(refresh);
    const timer = setInterval(refresh, 3000);
    const offs = [
      on<string>("names:log", async (l) => {
        const m = /^@@STEP (\d+) (\d+) (.*)$/.exec(l);
        if (m) return void (progress = { k: +m[1], n: +m[2], title: m[3] });
        log = [...log.slice(-400), l];
        await tick();
        if (logEl) logEl.scrollTop = logEl.scrollHeight;
      }),
      on<{ ok: boolean }>("names:done", (d) => {
        running = false;
        finished = d.ok;
        if (d.ok && progress) progress = { ...progress, title: "fin" };
        refresh();
      }),
    ];
    return () => {
      clearInterval(timer);
      call("names", "set_active", { on: false });
      offs.forEach((o) => o.then((f) => f()));
    };
  });
</script>

<div class="view">
  <div class="head">
    <div class="fh">Noms des paquets <span class="mut">· compatibilité du pack avec la version du jeu</span></div>
    {#if report}<span class="badge {VERDICT[report.verdict].cls}">{VERDICT[report.verdict].label}</span>{/if}
    <button onclick={refresh}>Vérifier</button>
    <button disabled={running || !report?.canCompute} onclick={() => compute("names")} title="Relance la correspondance des noms (match_protos) puis l'export du pack">Calculer les noms</button>
    <button disabled={running || !report?.canCompute} onclick={() => compute("full")} title="Après une mise à jour du jeu : copie des fichiers, dump IL2CPP, protos, données, noms, export">Tout régénérer depuis le jeu</button>
  </div>
  {#if progress && (running || finished !== null)}
    <div class="prog" class:fail={finished === false}>
      <div class="bar"><div class="fill" class:busy={running} style:width="{pct}%"></div></div>
      <div class="plabel">
        {#if finished === true}Terminé{:else if finished === false}Échec à l'étape {progress.k}/{progress.n}{:else}Étape {progress.k}/{progress.n} — {progress.title}{/if}
        <span class="mut">{pct} %</span>
      </div>
    </div>
  {/if}
  {#if error}<div class="banner bad">{error}</div>{/if}
  {#if report && !report.canCompute}<div class="mut note">Le calcul n'est disponible que depuis le dépôt de développement (le dossier pipeline/ est introuvable).</div>{/if}
  {#if finished === true}<div class="banner ok">Terminé : redémarrez Karrelage pour charger le nouveau pack.</div>{/if}
  {#if finished === false}<div class="banner bad">Le calcul a échoué (voir le journal).</div>{/if}

  {#if report}
    <div class="cols">
      <section>
        <h3>Pack chargé</h3>
        <div>Généré le {date(report.pack.generated)}</div>
        <div>{num(report.pack.messages)} messages · {num(report.pack.named)} nommés</div>
        <div class="mut small">{report.pack.dir}</div>

        <h3>Jeu installé</h3>
        <div class="state {report.game.state}">{report.game.detail}</div>
        {#if report.game.pack}
          <div class="mut small">GameAssembly : pack {num(report.game.pack.assembly)} · jeu {num(report.game.game?.assembly)} octets</div>
          <div class="mut small">global-metadata : pack {num(report.game.pack.metadata)} · jeu {num(report.game.game?.metadata)} octets</div>
        {/if}

        <h3>Trafic observé <button class="mini" onclick={() => call("names", "reset")}>Réinitialiser</button></h3>
        {#if report.live.types === 0}
          <div class="mut">Rien observé pour l'instant : activez la capture et jouez (ouvrez l'hôtel de vente, changez de carte…). L'observation ne tourne que pendant que cet onglet est ouvert.</div>
        {:else}
          <div>{num(report.live.messages)} messages · {report.live.types} types distincts</div>
          <div class:bad-t={report.live.unknownTypes > 0}>{report.live.unknownTypes} types inconnus du pack ({(report.live.ratio * 100).toFixed(1)} % des messages)</div>
          <div class="mut">{report.live.unnamedTypes} types connus mais sans nom clair</div>
          {#if !report.live.enough}<div class="mut small">Échantillon encore faible (moins de 15 types) : verdict provisoire.</div>{/if}
          {#each report.live.unknown as u (u.type)}<div class="mut small">• {u.type} ×{u.count}</div>{/each}
        {/if}
      </section>

      <section>
        <h3>Contrôles du pack</h3>
        {#each report.bindings as c (c.role)}
          <div class="chk"><span class={c.ok ? "ok-t" : "bad-t"}>{c.ok ? "✓" : "✗"}</span> {c.role} <span class="mut small">{c.detail}</span></div>
        {/each}
        <div class="chk">
          <span class={report.stale.count ? "warn-t" : "ok-t"}>{report.stale.count ? "!" : "✓"}</span> Noms obsolètes
          <span class="mut small">{report.stale.count ? `${report.stale.count} noms pointent vers des messages absents (${report.stale.examples.join(", ")}…)` : "aucun"}</span>
        </div>

        <h3>Journal</h3>
        <pre class="log" bind:this={logEl}>{log.length ? log.join("\n") : running ? "En cours…" : "—"}</pre>
      </section>
    </div>
  {/if}
</div>

<style>
  .view { flex: 1; min-height: 0; overflow: auto; padding: 12px; font-family: system-ui, sans-serif; font-size: 13px; }
  .head { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; margin-bottom: 10px; }
  .head .fh { flex: 1; margin: 0; font-weight: 600; }
  .badge { padding: 2px 10px; border-radius: 10px; font-weight: 600; border: 1px solid; }
  .badge.ok, .ok-t { color: var(--ok); }
  .badge.warn, .warn-t { color: var(--warn, orange); }
  .badge.bad, .bad-t { color: var(--bad, #e5534b); }
  .banner { padding: 6px 10px; margin-bottom: 8px; border-radius: 6px; }
  .banner.ok { background: color-mix(in srgb, var(--ok) 20%, transparent); }
  .banner.bad { background: color-mix(in srgb, var(--bad, #e5534b) 20%, transparent); }
  .prog { margin-bottom: 10px; }
  .bar { height: 10px; border-radius: 5px; background: var(--line); overflow: hidden; }
  .fill { height: 100%; background: var(--acc); transition: width 0.4s; }
  .fill.busy { background-image: linear-gradient(110deg, transparent 30%, #fff4 50%, transparent 70%); background-size: 200% 100%; animation: sweep 1.4s linear infinite; background-color: var(--acc); }
  .prog.fail .fill { background: var(--bad, #e5534b); }
  .plabel { margin-top: 4px; display: flex; gap: 10px; font-size: 12px; }
  @keyframes sweep { from { background-position: 200% 0; } to { background-position: -200% 0; } }
  .note { margin-bottom: 8px; }
  .cols { display: grid; grid-template-columns: minmax(280px, 1fr) minmax(320px, 1.4fr); gap: 20px; }
  h3 { margin: 14px 0 4px; font-size: 11px; letter-spacing: 0.08em; text-transform: uppercase; color: var(--mut); display: flex; gap: 8px; align-items: center; }
  .small { font-size: 11px; }
  .chk { padding: 2px 0; }
  .state.match { color: var(--ok); }
  .state.changed { color: var(--bad, #e5534b); font-weight: 600; }
  .state.unknown { color: var(--warn, orange); }
  .mini { padding: 1px 6px; font-size: 11px; text-transform: none; letter-spacing: 0; }
  .log { margin: 0; height: 260px; overflow: auto; background: #0a0b0e; border: 1px solid var(--line); border-radius: 6px; padding: 8px; font: 11px/1.4 ui-monospace, Consolas, monospace; white-space: pre-wrap; }
</style>
