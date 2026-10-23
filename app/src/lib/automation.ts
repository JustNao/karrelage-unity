/**
 * Automation engine: watches every auction listing captured live (whatever tab is open) and checks the enabled rules of
 * lib/stores/automation.svelte.ts; a match plays the rule's sound (once per scan) and raises one notification per lot.
 * Started once by App.svelte.
 *
 * exoDo: a lot with an exo (added effect) on a "% dommages" stat (an effect some "Rune Ta Do Per …" transcendance rune
 * adds), cheaper than that rune, whose break-even coefficient (Brisage tab: price / runes value at 100 %, best focus) is
 * at most `maxEven`, and that costs at most `maxPrice` (when set).
 */
import { call, cached } from "./api";
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

export async function startAutomation() {
  if (started) return;
  started = true;
  void prices.ensure();
  void auction.ensure().catch(() => {});
  cached<Rune[]>("prices", "runes").then((l) => (doRunes = l.filter(isDoPerRune))).catch(() => {});
  cached<BrisPack>("brisage", "items")
    .then((p) => ((levels = new Map(p.items.map((i) => [i.gid, i.level]))), (brisRunes = p.runes)))
    .catch(() => {});
  auction.subscribe((c) => {
    if (c.kind !== "put" || !c.live) return;
    for (const item of c.values) if (checkExoDo(item)) playSound(rules.exoDo.sound, rules.exoDo.volume);
  });
}
