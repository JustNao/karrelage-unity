/**
 * Automation engine: watches every auction listing captured live (whatever tab is open), the loot of every fight and the
 * monster groups and NPCs of every map, and
 * checks the enabled rules of lib/stores/automation.svelte.ts; a match plays the rule's sound (once per scan / fight) and
 * raises one notification per lot / looted item. Started once by App.svelte.
 *
 * exoDo: a lot with an exo (added effect) on a "% dommages" stat (an effect some "Rune Ta Do Per …" transcendance rune
 * adds), cheaper than that rune, whose break-even coefficient (Brisage tab: price / runes value at 100 %, best focus) is
 * at most `maxEven`, and that costs at most `maxPrice` (when set).
 */
import { cached, on } from "./api";
import { breakEven, lotEffects, outcome, type BrisPack } from "./brisage";
import { kamas } from "./fmt";
import { lotKeys, unitPriceOrNull } from "./price";
import { playSound } from "./sounds";
import { auction } from "./stores/auction.svelte";
import { prices } from "./stores/prices.svelte";
import { notifications, rules } from "./stores/automation.svelte";
import type { Rune } from "./transcend";

/** Transcendance runes of the "% dommages" stats (Rune Ta Do Per So / Ar / Di / Mé). */
const isDoPerRune = (r: Rune) => /\bDo Per\b/.test(r.name);

let started = false;
let doRunes: Rune[] = [];
/** Breakable items (gid -> level) and the base rune of each effect, from the Brisage module. */
let levels = new Map<number, number>();
let brisRunes: BrisPack["runes"] = {};
/** Lots already notified this session (gid:lot key): a rescan of the same item does not ring again. */
const notified = new Set<string>();

/** The rune for an exo of this value: same value, else the strongest one below it, else the weakest. */
function runeFor(a: number, value: number): Rune | null {
  const l = doRunes.filter((r) => r.a === a).sort((x, y) => x.value - y.value);
  return l.find((r) => r.value === value) ?? l.filter((r) => r.value <= value).pop() ?? l[0] ?? null;
}

function checkExoDo(item: any) {
  const r = rules.exoDo;
  const level = levels.get(item.gid);
  if (!r.enabled || item.companion || level == null) return 0;
  const keys = lotKeys(item.rows);
  const price = (gid: number) => prices.price(gid);
  let hits = 0;
  for (const lot of item.rows) {
    const buy = unitPriceOrNull(lot);
    if (buy == null || (r.maxPrice != null && buy > r.maxPrice)) continue;
    for (const e of lot.effects ?? []) {
      if (e.status !== "added" || e.hi == null) continue;
      const rune = runeFor(e.action, e.hi);
      const runePrice = rune && price(rune.gid);
      if (!rune || runePrice == null || buy >= runePrice) continue;
      const even = breakEven(buy, outcome(lotEffects(lot), level, brisRunes, price).best.value);
      if (even == null || even > r.maxEven) continue;
      const key = `${item.gid}:${keys.get(lot)}`;
      if (notified.has(key)) break;
      notified.add(key);
      hits++;
      notifications.push({
        rule: "exoDo",
        title: `${item.name} · exo ${e.text ?? rune.name}`,
        detail: `Lot ${kamas(buy)} < ${rune.name} ${kamas(runePrice)} · rentable dès ${Math.round(even)} %`,
        gid: item.gid,
        lot: keys.get(lot)!,
      });
      break; // one notification per lot
    }
  }
  return hits;
}

/** A looted object, from `loot:fight` (crates/modules/src/loot.rs). */
interface Looted { fighter: number | null; gid: number; name: string; type_id: number | null }
/** Ressource des Songes (item type) and the names of the ones the songes rule rings for. */
const SONGES_TYPE = 219;
const isLegend = (l: Looted) => l.type_id === SONGES_TYPE && l.name.startsWith("Légende");

/** songes: a « Légende … » looted by someone of the group; the best one's stored price picks the sound. */
function checkSonges(items: Looted[]) {
  const r = rules.songes;
  const legends = items.filter(isLegend);
  if (!r.enabled || !legends.length) return;
  let best = 0;
  for (const l of legends) {
    const p = prices.price(l.gid);
    best = Math.max(best, p ?? 0);
    notifications.push({ rule: "songes", title: l.name, detail: p != null ? `Prix enregistré ${kamas(p)}` : "Prix inconnu", gid: l.gid, lot: null });
  }
  playSound(best >= r.threshold ? r.sound : r.soundLow, r.volume);
}

/** A monster of the pack (`monsters` module `list`), for the Monstres search. */
export interface MonsterName { gid: number; name: string }
let monsterNames = new Map<number, string>();
/** Searched monsters already on the map (or seen spawning on it): a group showing up again does not ring twice. */
let onMap = new Set<number>();

/** monsters: a searched monster in a group of the map (`monsters:map`, crates/modules/src/map_monsters.rs) or of a group
 *  appearing on it (`monsters:spawn`): one sound for all of them, one notification per monster. */
function checkMonsters(gids: number[], map: boolean) {
  const r = rules.monsters;
  if (map) onMap = new Set();
  const wanted = new Set(r.gids);
  const hits = [...new Set(gids)].filter((g) => wanted.has(g) && !onMap.has(g));
  hits.forEach((g) => onMap.add(g));
  if (!r.enabled || !hits.length) return;
  for (const g of hits) notifications.push({ rule: "monsters", title: monsterNames.get(g) ?? `Monstre #${g}`, detail: "Sur la carte", gid: g, lot: null });
  playSound(r.sound, r.volume);
}

/** The goblin NPCs that can stand in a Songe room (pack NPC ids; King Gob seen on a real capture, the others share its batch:
 *  consecutive ids, names and look). */
export const SONGE_GOBLINS = [7848, 7849, 7850, 7853];
interface NpcName { id: number; name: string }
let npcNames = new Map<number, string>();
/** Goblins already on the map: one spawning again does not ring twice. */
let goblinsOnMap = new Set<number>();

/** goblins: a Songe goblin among the NPCs of the map entered (`monsters:map` npcs) or appearing on it (`monsters:spawn`). */
function checkGoblins(npcs: number[] | undefined, map: boolean) {
  const r = rules.goblins;
  if (map) goblinsOnMap = new Set();
  const hits = [...new Set(npcs ?? [])].filter((n) => SONGE_GOBLINS.includes(n) && !goblinsOnMap.has(n));
  hits.forEach((n) => goblinsOnMap.add(n));
  if (!r.enabled || !hits.length) return;
  for (const n of hits) notifications.push({ rule: "goblins", title: npcNames.get(n) ?? `PNJ #${n}`, detail: "Dans la salle", gid: n, lot: null });
  playSound(r.sound, r.volume);
}

export async function startAutomation() {
  if (started) return;
  started = true;
  void prices.ensure();
  void auction.ensure().catch(() => {});
  cached<Rune[]>("prices", "runes").then((l) => (doRunes = l.filter(isDoPerRune))).catch(() => {});
  cached<BrisPack>("brisage", "items")
    .then((p) => ((levels = new Map(p.items.map((i) => [i.gid, i.level]))), (brisRunes = p.runes)))
    .catch(() => {});
  void on<{ items: Looted[] }>("loot:fight", (e) => checkSonges(e.items));
  cached<MonsterName[]>("monsters", "list").then((l) => (monsterNames = new Map(l.map((m) => [m.gid, m.name])))).catch(() => {});
  cached<NpcName[]>("monsters", "npcs").then((l) => (npcNames = new Map(l.map((n) => [n.id, n.name])))).catch(() => {});
  void on<{ gids: number[]; npcs?: number[] }>("monsters:map", (e) => (checkMonsters(e.gids, true), checkGoblins(e.npcs, true)));
  void on<{ gids: number[]; npcs?: number[] }>("monsters:spawn", (e) => (checkMonsters(e.gids, false), checkGoblins(e.npcs, false)));
  auction.subscribe((c) => {
    if (c.kind !== "put" || !c.live) return;
    for (const item of c.values) if (checkExoDo(item)) playSound(rules.exoDo.sound, rules.exoDo.volume);
  });
}
