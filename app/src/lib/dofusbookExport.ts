/**
 * Export of the character's worn equipment as a Dofusbook build, through the import Dofusbook offers to Dofus Stuffer:
 * `https://www.dofusbook.net/fr/equipement/dofus-stuffer/objets?stuff=<base64 msgpack>` (format read from Dofusbook's decoder):
 *   "0": Dofusbook stat index (`KE`) -> value: vitality = 100 scroll + base HP + fm, the other characteristics = scroll + fm
 *        (a value >= 100 is read as a 100 scroll), PA / PM / PO / prospection / invocations / pods = character base + fm, others = fm
 *   "1": base points [vi sa fo in ch ag], "2": level, "3": exo flags (unused here: exos go in "0"),
 *   "4": item count per slot group (`GROUPS`; missing = 1), "5": game gids, group after group.
 * Dofusbook puts every item at its max roll; the exos the worn items carry (added effects, part above the base max) are sent as
 * forgemagie. The last group holds the familier or, without one, the mount (as in game: one or the other). Base points and
 * scrolls are the character's (inventory `character`; without them: 0 points, 100 scrolls). The level is not captured: 200.
 */
import type { Character, InvItem } from "./stores/inventory.svelte";
import { EXO_BASE, FM_STATS, lotStats, type Build } from "./stuff";

const URL = "https://www.dofusbook.net/fr/equipement/dofus-stuffer/objets?stuff=";
/** Dofusbook's stat order of the format. */
const KE = ["vi", "sa", "fo", "in", "ch", "ag", "pa", "pm", "ii", "pp", "po", "ic", "rpa", "epa", "rpm", "epm", "cc", "so", "ta", "fu", "dmg", "pu", "dc", "pd",
  "dnf", "dtf", "dff", "def", "daf", "rv", "pi", "pip", "dp", "ds", "dw", "dm", "dd", "rn", "rnp", "rt", "rtp", "rf", "rfp", "re", "rep", "ra", "rap", "rc",
  "rp", "rd", "rm", "rw"];
/** Slot groups of the format (cape, coiffe, ceinture, bottes, amulette, anneaux, Dofus, bouclier, arme, familier / monture) as the
 *  game's worn positions (16 = mount). */
const GROUPS = [[7], [6], [3], [5], [0], [2, 4], [9, 10, 11, 12, 13, 14], [15], [1], [8, 16]];
/** Character base values Dofusbook subtracts from the "0" entries (its `characterStats`), by `KE` code. */
const base = (level: number): Record<string, number> => ({ vi: (level - 1) * 5 + 55, pa: level < 100 ? 6 : 7, pm: 3, pp: 100, ic: 1, pd: 1000 });
/** The six characteristics in the format's order ("1" and the first `KE` entries) as game characteristic ids. */
const CARAC_IDS = [11, 12, 10, 15, 13, 14];

export function dofusbookLink(worn: InvItem[], character: Character | null, level = 200): string {
  const at = new Map(worn.map((i) => [i.position, i]));
  const stats: Record<string, number> = base(level);
  // scrolls: Dofusbook reads a value >= 100 as a 100 scroll + forgemagie, below as forgemagie only (same totals either way)
  CARAC_IDS.forEach((id, i) => (stats[KE[i]] = (stats[KE[i]] ?? 0) + (character ? character.carac[id]?.scroll ?? 0 : 100)));
  const points = CARAC_IDS.map((id) => character?.carac[id]?.base ?? 0);
  for (const it of worn) {
    for (const [s, v] of lotStats(it)) {
      if (s < EXO_BASE) continue;
      const code = Object.keys(FM_STATS).find((c) => FM_STATS[c].stat === s - EXO_BASE);
      if (code && KE.includes(code)) stats[code] = (stats[code] ?? 0) + v;
    }
  }
  const counts: Record<string, number> = {};
  const gids: number[] = [];
  GROUPS.forEach((ps, g) => {
    let here = ps.map((p) => at.get(p)).filter((i): i is InvItem => !!i);
    if (g === GROUPS.length - 1) here = here.slice(0, 1); // one slot: the familier first
    if (here.length !== 1) counts[g] = here.length;
    gids.push(...here.map((i) => i.gid));
  });
  const s0: Record<string, number> = {};
  KE.forEach((c, i) => { if (stats[c]) s0[i] = stats[c]; });
  const bytes = pack({ 0: s0, 1: points, 2: level, 3: 0, 4: counts, 5: gids });
  return URL + btoa(String.fromCharCode(...bytes)).replace(/=+$/, "");
}

/** Minimal msgpack encoder: integers, strings, arrays, objects (keys as strings, like Dofus Stuffer's). */
function pack(v: unknown, out: number[] = []): number[] {
  const be = (n: number, len: number) => { for (let i = len - 1; i >= 0; i--) out.push(Math.floor(n / 2 ** (8 * i)) & 0xff); };
  if (typeof v === "number") {
    const n = Math.trunc(v);
    if (n >= 0) {
      if (n < 0x80) out.push(n);
      else if (n < 0x100) out.push(0xcc, n);
      else if (n < 0x10000) { out.push(0xcd); be(n, 2); }
      else { out.push(0xce); be(n, 4); }
    } else if (n >= -32) out.push(n & 0xff);
    else if (n >= -0x80) out.push(0xd0, n & 0xff);
    else if (n >= -0x8000) { out.push(0xd1); be(n & 0xffff, 2); }
    else { out.push(0xd2); be(n >>> 0, 4); }
  } else if (typeof v === "string") {
    const b = new TextEncoder().encode(v);
    if (b.length < 32) out.push(0xa0 | b.length); else out.push(0xd9, b.length);
    out.push(...b);
  } else if (Array.isArray(v)) {
    if (v.length < 16) out.push(0x90 | v.length); else { out.push(0xdc); be(v.length, 2); }
    for (const x of v) pack(x, out);
  } else {
    const e = Object.entries(v as Record<string, unknown>);
    if (e.length < 16) out.push(0x80 | e.length); else { out.push(0xde); be(e.length, 2); }
    for (const [k, x] of e) { pack(k, out); pack(x, out); }
  }
  return out;
}

/** Build slot -> the game's worn position (as in crates/modules/src/stuff.rs `POSITION_SLOTS`). */
const SLOT_POSITIONS: Record<string, number> = {
  am: 0, ar: 1, a1: 2, ce: 3, a2: 4, bo: 5, ch: 6, ca: 7, fa: 8, d1: 9, d2: 10, d3: 11, d4: 12, d5: 13, d6: 14, br: 15, mo: 16,
};

/** Export link of a player's build (read on their card in game): their items, exos of their exact rolls as forgemagie. Their
 *  characteristics are not on the card: 0 points, 100 scrolls. */
export function playerBuildLink(b: Build): string {
  const worn = Object.entries(b.rolls ?? {}).flatMap(([slot, lot]) => (slot in SLOT_POSITIONS ? [{ ...lot, position: SLOT_POSITIONS[slot] } as InvItem] : []));
  return dofusbookLink(worn, null, b.level);
}
