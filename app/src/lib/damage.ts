/**
 * A spell's damage on a target (dofuspourlesnoobs.com/les-dommages.html), from the fight map's characteristics
 * (crates/modules/src/fight_map `DAMAGE_STATS`: a fighter's `stats`, the character's `character`, keyed by characteristic id):
 *   raw   = floor(roll × (100 + characteristic + power) / 100 + damages + elemental damages)
 *   taken = floor(max(0, raw − fixed resistance) × (100 − % resistance) / 100)
 * Neutral goes with strength. Not counted: critical hits, the final % multipliers (dealt / received, spell / weapon, melee /
 * distance: the same for every element, they do not change which one is best), area distance.
 */
import type { FightMapState, MapFighter } from "./fightMap";

export type Stats = Record<string, number>;

/** The roll of the reference spell the overlay compares elements with. */
export const REFERENCE_DAMAGE = 30;

/** Per element, ELEMENTS' order (neutral, earth, fire, water, air): characteristic, elemental damages, fixed resistance. */
const CHARACTERISTIC = [10, 10, 15, 13, 14];
const ELEMENT_DAMAGE = [92, 88, 89, 90, 91];
const FIXED_RESIST = [58, 54, 55, 56, 57];
const POWER = 25, DAMAGES = 16;

const get = (s: Stats, id: number) => s[id] ?? 0;

/** Damage a `roll` of `element` deals to a target with `resist` % in it and fixed resistances in `target`. */
export function damage(roll: number, attacker: Stats, element: number, resist: number, target: Stats): number {
  const multiplier = Math.max(0, 100 + get(attacker, CHARACTERISTIC[element]) + get(attacker, POWER)) / 100;
  const raw = Math.floor(multiplier * roll + get(attacker, DAMAGES) + get(attacker, ELEMENT_DAMAGE[element]));
  return Math.floor((Math.max(0, raw - get(target, FIXED_RESIST[element])) * Math.max(0, 100 - resist)) / 100);
}

/** The elements (earth, fire, water, air: neutral left out, as for the weakest element) dealing the most damage with the
 *  reference spell, and that damage. */
export function bestElement(attacker: Stats, target: MapFighter): { damage: number; elements: number[] } | null {
  if (!target.resists) return null;
  const defence = values(target.stats ?? {});
  const hits = [1, 2, 3, 4].map((e) => damage(REFERENCE_DAMAGE, attacker, e, target.resists![e], defence));
  const best = Math.max(...hits);
  return { damage: best, elements: hits.flatMap((d, i) => (d === best ? [i + 1] : [])) };
}

const values = (told: Record<string, [number, number]>): Stats => Object.fromEntries(Object.entries(told).map(([id, [v]]) => [id, v]));

/** The player's damage characteristics, per characteristic the newest of the character's sheet (login, after each fight,
 *  each equipment change: a set switched during the placement or between fights) and what the fight told about their
 *  fighter (its sheet at the start, buffs and debuffs). The fighter: whose turn it is (`playing`) when one of `mine`, else
 *  the first of `mine` in the fight. Null: nothing known yet. */
export function attackerStats(live: FightMapState, mine: Set<number>, playing: number | null): Stats | null {
  const own = live.fighters.filter((f) => mine.has(f.id) && !f.summoned);
  const fighter = own.find((f) => f.id === playing) ?? own[0];
  const told = { ...(live.character ?? {}) };
  for (const [id, t] of Object.entries(fighter?.stats ?? {})) if (!told[id] || t[1] > told[id][1]) told[id] = t;
  return "10" in told ? values(told) : null;
}
