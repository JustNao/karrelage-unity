import type { Fight } from "./stores/combat.svelte";

/**
 * Boss helpers: hints computed from the live fight (the player's characteristics, buffs, turn...), shown at the top of the
 * floating fight window while the boss is alive in the fight. One entry per boss in `BOSSES`.
 */
export interface Hint {
  boss: string;
  /** the main instruction, in big */
  text: string;
  detail?: string;
  /** where to aim, in quarter turns clockwise from the target around the player (0 = the target): drawn as a compass */
  aim?: number;
}

interface Boss {
  name: string;
  gids: number[];
  hints(f: Fight): Hint[];
}

const mod4 = (n: number) => ((n % 4) + 4) % 4;

/** Where to aim for each `aim` (quarter turns clockwise from the target). */
export const AIM_TEXT = ["Sur la cible", "À droite de la cible", "Derrière toi (à l'opposé de la cible)", "À gauche de la cible"];

const quarter = (n: number) => {
  const q = Math.abs(n);
  return `${q} quart${q > 1 ? "s" : ""} de tour ${n > 0 ? "horaire" : "contre horaire"}`;
};

/** Comte Harebourg: the player's spells are turned around them by their confusion (set at each turn start from their life,
 *  +1 quarter turn per melee hit): aiming at the target turned back by as much makes the spell land on it. */
const harebourg: Boss = {
  name: "Comte Harebourg",
  gids: [3416, 8184, 8185],
  hints(f) {
    if (f.confusion == null)
      return [
        f.me == null
          ? { boss: this.name, text: "Personnage inconnu", detail: "Ajoute le nom de ton personnage dans l'onglet Automation (section Combat)." }
          : { boss: this.name, text: "Confusion pas encore lue", detail: "Elle est connue dès le début de ton prochain tour (en groupe : ajoute ton personnage dans Automation)." },
      ];
    const c = f.confusion;
    return [
      {
        boss: this.name,
        text: AIM_TEXT[mod4(-c)],
        detail: c ? `Confusion : ${quarter(c)} (+1 horaire par coup au corps à corps)` : "Pas de confusion",
        aim: mod4(-c),
      },
    ];
  },
};

export const BOSSES: Boss[] = [harebourg];

/** The hints of the bosses alive in the fight. */
export function bossHints(f: Fight): Hint[] {
  if (!f.active) return [];
  const alive = new Set(f.fighters.filter((x) => !x.dead).map((x) => x.gid));
  return BOSSES.filter((b) => b.gids.some((g) => alive.has(g))).flatMap((b) => b.hints(f));
}
