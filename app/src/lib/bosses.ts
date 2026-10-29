import type { Fight } from "./stores/combat.svelte";

/**
 * Boss helpers: hints computed from the live fight (the player's characteristics, buffs, turn...), shown in the boss's card of
 * the floating fight window. One entry per boss in `BOSSES`.
 */
export interface Hint {
  /** the main instruction, in big */
  text: string;
  detail?: string;
  /** where to aim, in quarter turns clockwise from the target around the player (0 = the target): drawn as a compass */
  aim?: number;
}

interface Boss {
  gids: number[];
  hints(f: Fight): Hint[];
}

const mod4 = (n: number) => ((n % 4) + 4) % 4;

/** Where to aim for each `aim` (quarter turns clockwise from the target). */
export const AIM_TEXT = ["Sur la cible", "À droite de la cible", "Derrière toi (à l'opposé de la cible)", "À gauche de la cible"];

/** Comte Harebourg: the player's spells are turned around them by their confusion (set at each turn start from their life,
 *  +1 quarter turn per melee hit): aiming at the target turned back by as much makes the spell land on it. */
const harebourg: Boss = {
  gids: [3416, 8184, 8185],
  hints(f) {
    if (f.confusion == null)
      return [
        f.me == null
          ? { text: "Personnage inconnu", detail: "Ajoute le nom de ton personnage dans l'onglet Automation (section Combat)." }
          : { text: "Confusion pas encore lue", detail: "Elle est connue dès le début de ton prochain tour." },
      ];
    const aim = mod4(-f.confusion);
    return [{ text: AIM_TEXT[aim], aim }];
  },
};

export const BOSSES: Boss[] = [harebourg];

/** The hints for a monster of the fight (none once the fight is over, or for a monster that is no boss). */
export function bossHints(f: Fight, gid: number): Hint[] {
  if (!f.active) return [];
  return BOSSES.find((b) => b.gids.includes(gid))?.hints(f) ?? [];
}
