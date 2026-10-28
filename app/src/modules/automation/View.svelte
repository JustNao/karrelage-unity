<script lang="ts">
  import { RULE_DEFAULTS, RULE_TITLES, rules, type RuleBase } from "../../lib/stores/automation.svelte";
  import { playSound, SOUNDS } from "../../lib/sounds";
  import SmoothScroll from "../../lib/components/SmoothScroll.svelte";
  import Switch from "../../lib/components/Switch.svelte";
  import { kamasFull } from "../../lib/fmt";
  import { settings, type CombatShow } from "../../lib/stores/settings.svelte";
  import Segmented from "../../lib/components/Segmented.svelte";
  import { characterNames, fight, openCombatOverlay } from "../../lib/stores/combat.svelte";
  import Banner from "../../lib/components/Banner.svelte";
  import SearchBar, { matcher } from "../../lib/components/SearchBar.svelte";
  import MonsterSearch from "./MonsterSearch.svelte";

  /** Automation: rules checked on every scan, fight end, turn or map change (and the fight following, Combat section) (lib/automation.ts), one section per tab, like Paramètres. A triggered rule
   *  plays its sound and shows up in the header « Notifications ». */

  /** "2,500,000", "2.5M", "500k" -> kamas; empty or unreadable -> null (no limit). */
  function parsePrice(s: string): number | null {
    const m = s.replace(/[\s,]/g, "").match(/^(\d+(?:\.\d+)?)([kKmM]?)$/);
    if (!m) return null;
    const v = parseFloat(m[1]) * (m[2] === "" ? 1 : m[2].toLowerCase() === "k" ? 1e3 : 1e6);
    return v > 0 ? Math.round(v) : null;
  }
  const priceText = (v: number | null) => (v != null ? kamasFull(v).replace(/\s*K$/, "") : "");
  /** Notes shown for each monster of the fight (floating window). */
  const SHOW: { id: CombatShow; label: string; title?: string }[] = [
    { id: "none", label: "Rien", title: "Aucune remarque : seulement les tags des monstres" },
    { id: "warning", label: "Avertissements" },
    { id: "invulnerable", label: "+ Invulnérabilités" },
    { id: "all", label: "+ Conseils" },
  ];
  /** Each rule's description, also searched. */
  const DESC = {
    turn: "Un son au début de chacun de vos tours de jeu, sauf si votre personnage est mort (votre personnage est reconnu à la connexion, application ouverte, ou par son nom ci-dessus).",
    follow: "Détecte les monstres du combat en cours (paquets du placement et des tours) et affiche leurs remarques et les poisons PA / PM dans la fenêtre flottante.",
    songes: "À la fin d'un combat, si quelqu'un du groupe a obtenu une « Légende … » (Ressource des Songes) : un son selon le prix enregistré de la meilleure, au-dessus ou en dessous du seuil (prix inconnu = en dessous), et une notification par Légende.",
    goblins: "En entrant dans une salle, si un gobelin des Songes s'y trouve (King Gob, Gobséric, Gobledore ou Bog) : un son par gobelin, l'un après l'autre, et une notification par gobelin.",
    monsters: "À chaque changement de carte (et quand un groupe apparaît sur la carte), si un des monstres recherchés est dans un groupe : un seul son, même si plusieurs correspondent, et une notification par monstre.",
    exoDo: "À chaque objet scanné à l'hôtel de vente : un lot avec un exo « % dommages » (sorts, armes, distance, mêlée) moins cher que la rune de transcendance de cette stat (prix enregistré), et dont le brisage rembourse le prix à partir d'un coefficient au plus égal au seuil (« Coef. rentable » de l'onglet Brisage), sans dépasser le prix max s'il est fixé. Un même lot ne sonne qu'une fois par session.",
  };
  const FOLLOW = "Suivre les monstres des combats";
  /** Searched text (session only): a rule shows when its section, title or description matches. */
  let q = $state("");
  const SECTION = { turn: "Combat", follow: "Combat", songes: "Songes", goblins: "Songes", monsters: "Monstres", exoDo: "HDV" } as const;
  const shown = $derived.by(() => {
    const m = matcher(q);
    const title = (k: keyof typeof DESC) => (k === "follow" ? FOLLOW : RULE_TITLES[k]);
    return Object.fromEntries((Object.keys(DESC) as (keyof typeof DESC)[]).map((k) => [k, m(SECTION[k], title(k), DESC[k])])) as Record<keyof typeof DESC, boolean>;
  });
  /** No character of the player known yet: neither the turn sound nor the poisons can tell it apart. */
  const unknownMe = $derived(!fight.mine?.length && (rules.turn.enabled || settings.combat.enabled));
  const clampCoef = (v: number) => (Number.isFinite(v) ? Math.max(0, Math.round(v)) : RULE_DEFAULTS.exoDo.maxEven);
</script>

{#snippet pick(r: Record<string, any>, key: string, title: string)}
  <select value={r[key]} onchange={(e) => (r[key] = e.currentTarget.value)} {title}>
    {#each SOUNDS as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
  </select>
  <button class="mini" title="Écouter" onclick={() => playSound(r[key], r.volume)}>▶</button>
{/snippet}

{#snippet volume(r: RuleBase)}
  <input type="range" min="0" max="1" step="0.05" value={r.volume} oninput={(e) => (r.volume = e.currentTarget.valueAsNumber)} title="Volume" />
  <span class="mut vol">{Math.round(r.volume * 100)} %</span>
{/snippet}

{#snippet sound(r: RuleBase)}
  <div class="snd">
    {@render pick(r, "sound", "Son joué quand la règle se déclenche")}
    {@render volume(r)}
  </div>
{/snippet}

<div class="view">
  <SmoothScroll>
    <div class="auto">
      <div class="search"><SearchBar bind:value={q} placeholder="Rechercher une règle…" grow /></div>
      {#if shown.turn || shown.follow}
        <section>
          <h2>Combat</h2>
          {#if unknownMe}
            <Banner tone="warn" inset>
              Personnage non reconnu : le son « {RULE_TITLES.turn} » ne se déclenchera pas et aucun poison ne sera affiché.
              {#if characterNames(settings.combat.characters).length}Il sera reconnu au placement du prochain combat.{:else}Reconnectez-vous avec l'application ouverte, ou indiquez son nom ci-dessous.{/if}
            </Banner>
          {/if}
          <label class="me" title="Noms de vos personnages, séparés par des virgules : reconnus au placement d'un combat (utile si l'application n'était pas ouverte à la connexion)">
            Vos personnages
            <input type="text" placeholder="Nom1, Nom2" bind:value={settings.combat.characters} />
            {#if fight.mine?.length}<span class="mut">{fight.mine.length} reconnu{fight.mine.length > 1 ? "s" : ""}</span>{/if}
          </label>
        {#if shown.turn}
        <div class="rule" class:off={!rules.turn.enabled}>
          <div class="rh">
            <Switch bind:checked={rules.turn.enabled} label="Activer la règle" />
            <div class="lb">
              <div><b>{RULE_TITLES.turn}</b></div>
              <div class="mut">{DESC.turn}</div>
            </div>
          </div>
          <div class="params">
            <label class="chk" title="Ne pas sonner quand la fenêtre de Dofus est au premier plan">
              <input type="checkbox" bind:checked={rules.turn.unfocusedOnly} /> Seulement si le jeu n'est pas au premier plan
            </label>
            {@render sound(rules.turn)}
          </div>
        </div>
        {/if}
        {#if shown.follow}
        <div class="rule" class:off={!settings.combat.enabled}>
          <div class="rh">
            <Switch bind:checked={settings.combat.enabled} label="Suivre les combats" />
            <div class="lb">
              <div><b>{FOLLOW}</b></div>
              <div class="mut">{DESC.follow}</div>
            </div>
          </div>
          <div class="params">
            <Segmented bind:value={settings.combat.show} options={SHOW} label="Informations affichées" />
            <label class="chk" title="Petite fenêtre toujours au premier plan (sauf jeu en plein écran exclusif) : elle s'ouvre au début d'un combat, là où vous l'avez laissée, et se ferme à la fin">
              <input type="checkbox" bind:checked={settings.combat.overlay} /> Fenêtre flottante pendant les combats
            </label>
            <button class="mini" onclick={() => openCombatOverlay()} title="Afficher la fenêtre flottante maintenant (pour la placer sur le jeu : elle rouvrira au même endroit)">⧉ Afficher</button>
          </div>
        </div>
        {/if}
        </section>
      {/if}
      {#if shown.songes || shown.goblins}
        <section>
          <h2>Songes</h2>
        {#if shown.songes}
        <div class="rule" class:off={!rules.songes.enabled}>
          <div class="rh">
            <Switch bind:checked={rules.songes.enabled} label="Activer la règle" />
            <div class="lb">
              <div><b>{RULE_TITLES.songes}</b></div>
              <div class="mut">{DESC.songes}</div>
            </div>
          </div>
          <div class="params">
            <label title="Prix enregistré (onglet Prix enregistrés) à partir duquel la Légende est une bonne nouvelle">
              Seuil
              <input class="price" type="text" inputmode="numeric" value={priceText(rules.songes.threshold)} onchange={(e) => (e.currentTarget.value = priceText((rules.songes.threshold = parsePrice(e.currentTarget.value) ?? RULE_DEFAULTS.songes.threshold)))} />
              <span class="mut">K</span>
            </label>
            <div class="snd">
              <span class="mut">Au-dessus</span>
              {@render pick(rules.songes, "sound", "Son joué quand la Légende vaut au moins le seuil")}
            </div>
            <div class="snd">
              <span class="mut">En dessous</span>
              {@render pick(rules.songes, "soundLow", "Son joué quand la Légende vaut moins que le seuil")}
            </div>
            <div class="snd">{@render volume(rules.songes)}</div>
          </div>
        </div>
        {/if}
        {#if shown.goblins}
        <div class="rule" class:off={!rules.goblins.enabled}>
          <div class="rh">
            <Switch bind:checked={rules.goblins.enabled} label="Activer la règle" />
            <div class="lb">
              <div><b>{RULE_TITLES.goblins}</b></div>
              <div class="mut">{DESC.goblins}</div>
            </div>
          </div>
          <div class="params">
            {@render sound(rules.goblins)}
          </div>
        </div>
        {/if}
        </section>
      {/if}
      {#if shown.monsters}
        <section>
          <h2>Monstres</h2>
        {#if shown.monsters}
        <div class="rule" class:off={!rules.monsters.enabled}>
          <div class="rh">
            <Switch bind:checked={rules.monsters.enabled} label="Activer la règle" />
            <div class="lb">
              <div><b>{RULE_TITLES.monsters}</b></div>
              <div class="mut">{DESC.monsters}</div>
            </div>
          </div>
          <div class="params">
            <MonsterSearch bind:gids={rules.monsters.gids} />
            {@render sound(rules.monsters)}
          </div>
        </div>
        {/if}
        </section>
      {/if}
      {#if shown.exoDo}
        <section>
          <h2>HDV</h2>
        {#if shown.exoDo}
        <div class="rule" class:off={!rules.exoDo.enabled}>
          <div class="rh">
            <Switch bind:checked={rules.exoDo.enabled} label="Activer la règle" />
            <div class="lb">
              <div><b>{RULE_TITLES.exoDo}</b></div>
              <div class="mut">{DESC.exoDo}</div>
            </div>
          </div>
          <div class="params">
            <label title="Coefficient de brisage à partir duquel le lot est rentable (meilleur résultat, avec ou sans focus)">
              Coef. rentable max
              <input type="number" min="0" step="10" value={rules.exoDo.maxEven} onchange={(e) => (rules.exoDo.maxEven = clampCoef(e.currentTarget.valueAsNumber))} />
              <span class="mut">%</span>
            </label>
            <label title="Prix du lot au-delà duquel la règle ne se déclenche pas (vide = sans limite)">
              Prix max
              <input class="price" type="text" inputmode="numeric" placeholder="sans limite" value={priceText(rules.exoDo.maxPrice)} onchange={(e) => (e.currentTarget.value = priceText((rules.exoDo.maxPrice = parsePrice(e.currentTarget.value))))} />
              <span class="mut">K</span>
            </label>
            {@render sound(rules.exoDo)}
          </div>
        </div>
        {/if}
        </section>
      {/if}
      {#if !Object.values(shown).some(Boolean)}<p class="mut">Aucune règle ne correspond à « {q} ».</p>{/if}
    </div>
  </SmoothScroll>
</div>

<style>
  .view { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .view > :global(.wrap) { flex: 1; }
  .auto { max-width: 760px; font-family: var(--font); font-size: 13px; }
  section { margin-bottom: 20px; }
  .search { margin-bottom: 14px; display: flex; }
  h2 { font: 600 12px var(--font); letter-spacing: 0.08em; text-transform: uppercase; color: var(--mut); margin: 4px 0 8px; padding-bottom: 4px; border-bottom: 1px solid var(--line); }
  .rule { padding: 8px 0; display: flex; flex-direction: column; gap: 8px; }
  .rule.off .params { opacity: 0.5; }
  .rh { display: flex; gap: 12px; align-items: flex-start; }
  .lb .mut { font-size: 12px; margin-top: 2px; }
  .params { display: flex; flex-wrap: wrap; gap: 8px 18px; align-items: center; padding-left: 56px; }
  .params label { display: inline-flex; align-items: center; gap: 6px; }
  .params input[type="number"] { width: 70px; }
  .params .chk { cursor: pointer; }
  .params .chk input { margin: 0; }
  .params input.price { width: 110px; text-align: right; }
  .snd { display: inline-flex; align-items: center; gap: 6px; }
  .snd input[type="range"] { width: 110px; }
  .vol { min-width: 38px; }
  .me { display: flex; align-items: center; gap: 8px; padding: 4px 0 4px 56px; }
  .me input { width: 220px; }
</style>
