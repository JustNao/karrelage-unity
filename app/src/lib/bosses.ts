import type { Fight, Fighter } from "./stores/combat.svelte";

/**
 * Boss helpers: hints computed from the live fight (round, the player's characteristics, damage taken...), shown in the
 * boss's card of the floating fight window. One entry per boss in `BOSSES`. Rounds count from the boss's appearance (1 = the
 * round it appeared in: the fight's first round, or later in multi-wave fights); `played` tells whether its turn of the current
 * round already started (a mechanic at the start of its turn is then for a later round).
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
  /** `alive` = the living fighters of these gids */
  hints(f: Fight, alive: Fighter[]): Hint[];
}

const mod4 = (n: number) => ((n % 4) + 4) % 4;

/** The fighter's round: 1 = the round it appeared in (0 = placement). */
const roundOf = (f: Fight, x: Fighter) => (f.round ? f.round - (x.since ?? 1) + 1 : 0);

/** The next round of the fighter (its own count, see `roundOf`) matching `when` at the start of its turn: this one while its
 *  turn has not started yet. */
function next(f: Fight, x: Fighter, when: (n: number) => boolean): number {
  let n = Math.max(1, roundOf(f, x) + (x.played === f.round && f.round ? 1 : 0));
  while (!when(n) && n < 1000) n++;
  return n;
}

/** "à son tour" / "dans 1 tour" / "dans N tours" for a round `n` of the fighter. */
function inRounds(f: Fight, x: Fighter, n: number): string {
  const d = n - Math.max(1, roundOf(f, x));
  return d <= 0 ? "à son tour" : `dans ${d} tour${d > 1 ? "s" : ""}`;
}

/** Where to aim for each `aim` (quarter turns clockwise from the target). */
export const AIM_TEXT = ["Sur la cible", "À droite de la cible", "Derrière toi (à l'opposé de la cible)", "À gauche de la cible"];

export const ELEMENTS = ["Neutre", "Terre", "Feu", "Eau", "Air"];

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

/** Obsidiantre's Pougnette: explodes at the end of its 5th turn (summoned every 3 turns), killing every character further
 *  than 7 range from it. */
const pougnette: Boss = {
  gids: [2951],
  hints(f, alive) {
    return alive.map((x) => {
      // its turns started so far (its first: the round it was summoned in, once it played)
      const done = Math.max(0, roundOf(f, x) - (x.played === f.round && f.round ? 0 : 1));
      const left = 5 - done;
      return {
        text: left <= 0 ? "Explose à la fin de ce tour !" : left === 1 ? "Explose à la fin de son prochain tour" : `Explose dans ${left} tours`,
        detail: "Tue les personnages à plus de 7 PO d'elle (Pacifiste l'en empêche)",
      };
    });
  },
};

/** Nileza: at the start of its turn on odd rounds from 3, characters too far get Pacifiste for 2 turns, and everyone a malus. */
const NILEZA: [number, string][] = [
  [11, "−200 fuite"],
  [9, "−200 tacle"],
  [7, "Pesanteur"],
  [5, "Insoignable"],
];
const nileza: Boss = {
  gids: [3397],
  hints(f, alive) {
    return alive.slice(0, 1).map((x) => {
      const n = next(f, x, (n) => n >= 3 && n % 2 === 1);
      const [po, malus] = NILEZA[Math.min((n - 3) / 2, NILEZA.length - 1)];
      return { text: `Tour ${n} : reste à ${po} PO max de Nileza`, detail: `${inRounds(f, x, n)} · sinon Pacifiste 2 tours · tous : ${malus} 2 tours` };
    });
  },
};

/** Klime: Peau lisse covers the first two rounds; from the end of round 2, ending a turn off a white glyph = Pacifiste the
 *  next round. */
const klime: Boss = {
  gids: [3384],
  hints(f, alive) {
    return alive.slice(0, 1).map((x) => {
      const n = roundOf(f, x);
      return n < 2
        ? { text: "Pas besoin de glyphe ce tour", detail: "Peau lisse protège jusqu'au tour 2" }
        : { text: "Finis ton tour sur une glyphe blanche", detail: `Sinon Pacifiste au tour ${n + 1}` };
    });
  },
};

/** Kimbo: the last element it took since its turn started sets its glyphs: earth / water = odd (glyphs at odd distances from
 *  the Disciple), air / fire = even; a character starting their turn on a glyph dies. */
const kimbo: Boss = {
  gids: [1045],
  hints(f, alive) {
    return alive.slice(0, 1).map((x) =>
      x.hit ? { text: `Dernier élément ${ELEMENTS[x.hit]}, glyphes ${x.hit === 1 || x.hit === 3 ? "impaires" : "paires"}` } : { text: "Kimbo pas encore tapé" },
    );
  },
};

/** Dantinéa: every 5 rounds (6, 11, 16...) the living Grokillages are killed and new ones summoned. */
const dantinea: Boss = {
  gids: [4444],
  hints(f, alive) {
    return alive.slice(0, 1).map((x) => {
      const n = next(f, x, (n) => n >= 6 && n % 5 === 1);
      return { text: `Grokillages réinvoqués au tour ${n}`, detail: `${inRounds(f, x, n)} · les vivants sont tués` };
    });
  },
};

/** Phossile: Abîme at the start of its turn on round 5 then every 4 (9, 13...) kills every character within 3 range. */
const phossile: Boss = {
  gids: [3651],
  hints(f, alive) {
    return alive.slice(0, 1).map((x) => {
      const n = next(f, x, (n) => n >= 5 && (n - 5) % 4 === 0);
      return { text: `Abîme au tour ${n} : à plus de 3 PO !`, detail: `${inRounds(f, x, n)} · OS tout personnage à 3 PO ou moins` };
    });
  },
};

export const BOSSES: Boss[] = [harebourg, pougnette, nileza, klime, kimbo, dantinea, phossile];

/** The hints for a monster of the fight (none once the fight is over, or for a monster that is no boss). */
export function bossHints(f: Fight, gid: number): Hint[] {
  if (!f.active) return [];
  const b = BOSSES.find((b) => b.gids.includes(gid));
  return b ? b.hints(f, f.fighters.filter((x) => x.gid === gid && !x.dead)) : [];
}
