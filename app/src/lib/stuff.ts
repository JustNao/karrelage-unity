/** Stuff tab logic: slots, stat panels, totals (items + set bonuses + base / scrolls / exo), prices and the cheapest-combination solver.
 *  Item stats and set bonuses come from the data pack (`stuff.item_info`), never from Dofusbook. */
import { craftCost, type Recipes } from "./craft";
import type { StoredPrice } from "./transcend";

export const FRESH_SECS = 30 * 60;

export interface SlotRef { db_id: number; gid: number; picture: number; name: string; level: number }
export interface Build {
  id: string; name: string; url: string; level: number; class: number; avatar?: string; fetched: number;
  carac: Record<string, number>; fm: Record<string, number>; slots: Record<string, SlotRef>;
  /** stat id -> wanted minimum total */
  targets: Record<string, number>;
  /** slot -> uid of the chosen auction lot */
  chosen: Record<string, number | null>;
  /** slots left out of the price (they still count in stats) */
  ignored: string[];
}
export interface InfoEffect { a: number; min: number; max: number; stat: number; label: string | null; group: string | null; density: number | null }
export interface ItemInfo { name: string; level: number; type_id: number; type: string; set_id: number | null; icon_id: number; effects: InfoEffect[] }
export interface SetInfo { name: string | null; items: number[]; effects: InfoEffect[][] }
export interface Info { items: Record<string, ItemInfo>; sets: Record<string, SetInfo> }

/** Layout of the slots around the character (screenshot): left column, right column, bottom row. */
export const SLOT_LAYOUT = {
  left: ["ar", "bo", "am", "ce", "br"],
  right: ["ch", "ca", "a1", "a2", "fa"],
  bottom: ["mo", "d1", "d2", "d3", "d4", "d5", "d6"],
};
export const SLOT_TITLES: Record<string, string> = {
  ar: "Arme", bo: "Bouclier", am: "Amulette", ce: "Ceinture", br: "Bottes", ch: "Coiffe", ca: "Cape", a1: "Anneau 1", a2: "Anneau 2",
  fa: "Familier", mo: "Monture", d1: "Dofus 1", d2: "Dofus 2", d3: "Dofus 3", d4: "Dofus 4", d5: "Dofus 5", d6: "Dofus 6",
};

// ---- stats ------------------------------------------------------------------------------------------------------------

export interface StatRow { stat: number; label: string; icon: string }
const r = (stat: number, label: string, icon: string): StatRow => ({ stat, label, icon });
/** Characteristic ids (game `characteristics` table) shown in each panel; icon = Dofusbook element code. */
export const PANELS = {
  main: [r(1, "PA", "pa"), r(23, "PM", "pm"), r(19, "PO", "po"), r(48, "Prospection", "pp"), r(44, "Initiative", "ii"), r(18, "Critique", "cc"), r(26, "Invocations", "ic"), r(49, "Soin", "so")],
  carac: [r(11, "Vitalité", "vi"), r(12, "Sagesse", "sa"), r(10, "Force", "fo"), r(15, "Intelligence", "in"), r(13, "Chance", "ch"), r(14, "Agilité", "ag"), r(25, "Puissance", "pu")],
  secondary: [r(78, "Fuite", "fu"), r(79, "Tacle", "ta"), r(27, "Esq. PA", "rpa"), r(28, "Esq. PM", "rp"), r(82, "Ret. PA", "rap"), r(83, "Ret. PM", "rp"), r(40, "Pods", "")],
  damage: [r(92, "Do Neutre", "dnf"), r(88, "Do Terre", "dtf"), r(89, "Do Feu", "dff"), r(90, "Do Eau", "def"), r(91, "Do Air", "daf"), r(16, "Dommages", "dm"), r(86, "Do Critique", "dc"), r(84, "Do Poussée", "dp"),
    r(31, "% Do Armes", "dw"), r(98, "% Do Sorts", "sp"), r(125, "% Do Mêlée", "da"), r(120, "% Do Dist", "da")],
  resist: [r(58, "Ré Neutre", "rn"), r(54, "Ré Terre", "rt"), r(55, "Ré Feu", "rf"), r(56, "Ré Eau", "re"), r(57, "Ré Air", "ra"), r(87, "Ré Critique", "rc"), r(85, "Ré Poussée", "rp"),
    r(37, "%Ré Neutre", "rnp"), r(33, "%Ré Terre", "rtp"), r(34, "%Ré Feu", "rfp"), r(35, "%Ré Eau", "rep"), r(36, "%Ré Air", "rap"), r(124, "%Ré Mêlée", "rc"), r(121, "%Ré Dist", "rc"), r(142, "%Ré Armes", "rc")],
};
export const STAT_ICON_CODES = [...new Set([...Object.values(PANELS).flat().map((x) => x.icon).filter(Boolean), "pv"])];
export const STAT_LABEL = new Map<number, string>(Object.values(PANELS).flat().map((x) => [x.stat, x.label]));

/** Dofusbook characteristic codes -> stat id (base / scroll). */
export const CARAC: [string, number][] = [["vi", 11], ["sa", 12], ["fo", 10], ["in", 15], ["ch", 13], ["ag", 14]];
/** Base values every character has at level 100+. */
export const BASE_STATS: Record<number, number> = { 1: 7, 23: 3, 48: 100, 26: 1, 40: 1000 };

/** Forgemagie / exo codes of Dofusbook's `fmGlobal` -> game characteristic id. */
export const FM_STATS: Record<string, { stat: number; label: string }> = {
  pa: { stat: 1, label: "PA" }, pm: { stat: 23, label: "PM" }, po: { stat: 19, label: "PO" }, vi: { stat: 11, label: "Vitalité" }, sa: { stat: 12, label: "Sagesse" },
  fo: { stat: 10, label: "Force" }, in: { stat: 15, label: "Intelligence" }, ch: { stat: 13, label: "Chance" }, ag: { stat: 14, label: "Agilité" }, pu: { stat: 25, label: "Puissance" },
  ii: { stat: 44, label: "Initiative" }, cc: { stat: 18, label: "Critique" }, ic: { stat: 26, label: "Invocations" }, so: { stat: 49, label: "Soin" }, pp: { stat: 48, label: "Prospection" },
  fu: { stat: 78, label: "Fuite" }, ta: { stat: 79, label: "Tacle" }, dmg: { stat: 16, label: "Dommages" }, dnf: { stat: 92, label: "Do Neutre" }, dtf: { stat: 88, label: "Do Terre" },
  dff: { stat: 89, label: "Do Feu" }, def: { stat: 90, label: "Do Eau" }, daf: { stat: 91, label: "Do Air" }, dc: { stat: 86, label: "Do Critique" }, dp: { stat: 84, label: "Do Poussée" },
  dw: { stat: 31, label: "% Do Armes" }, ds: { stat: 98, label: "% Do Sorts" }, dm: { stat: 125, label: "% Do Mêlée" }, dd: { stat: 120, label: "% Do Dist" },
  rn: { stat: 58, label: "Ré Neutre" }, rt: { stat: 54, label: "Ré Terre" }, rf: { stat: 55, label: "Ré Feu" }, re: { stat: 56, label: "Ré Eau" }, ra: { stat: 57, label: "Ré Air" },
  rnp: { stat: 37, label: "%Ré Neutre" }, rtp: { stat: 33, label: "%Ré Terre" }, rfp: { stat: 34, label: "%Ré Feu" }, rep: { stat: 35, label: "%Ré Eau" }, rap: { stat: 36, label: "%Ré Air" },
  rc: { stat: 87, label: "Ré Critique" }, rp: { stat: 85, label: "Ré Poussée" }, rm: { stat: 124, label: "%Ré Mêlée" }, rd: { stat: 121, label: "%Ré Dist" }, rw: { stat: 142, label: "%Ré Armes" },
  epa: { stat: 27, label: "Esq. PA" }, epm: { stat: 28, label: "Esq. PM" }, rpa: { stat: 82, label: "Ret. PA" }, rpm: { stat: 83, label: "Ret. PM" },
};
/** Forgemagie bonus a build gives to one stat (shown as a blue "+X"). */
export const fmOf = (b: Build, stat: number) => Object.entries(b.fm ?? {}).reduce((n, [c, v]) => n + (FM_STATS[c]?.stat === stat ? v : 0), 0);

export type Totals = Map<number, number>;
const add = (m: Totals, stat: number, v: number) => m.set(stat, (m.get(stat) ?? 0) + v);
const negative = (e: { label?: string | null; density?: number | null; text?: string }) => (e.label ?? e.text ?? "").startsWith("-") || (e.density ?? 0) < 0;
const countable = (group: string | null | undefined) => !!group && group !== "weapon" && group !== "other";

/** Lot of the auction store (see crates/modules/src/auction.rs). */
export interface Lot { uid: number; gid: number; prices: { qty: number; price: number }[]; effects: any[] }
export const lotUnit = (lot: Lot): number | null => {
  const v = lot.prices.filter((p) => p.price > 0 && p.qty > 0).map((p) => p.price / p.qty);
  return v.length ? Math.min(...v) : null;
};

/** Pseudo-stat ids (EXO_BASE + stat) holding the part of a lot's stat that is an exo (effect added on the item, or forged over its base maximum): the build's forgemagie
 *  (e.g. +1 PA / +1 PM) is then looked for on the items themselves. */
export const EXO_BASE = 100000;

/** Stat totals of a lot (removed effects don't count). */
export function lotStats(lot: Lot): Totals {
  const m: Totals = new Map();
  for (const e of lot.effects) {
    if (e.status === "removed" || !countable(e.group)) continue;
    const v = Number(e.hi ?? (Array.isArray(e.params) ? e.params[0] : 0)) || 0;
    add(m, e.stat, negative(e) ? -v : v);
    // exo = what the lot carries beyond its natural effects: a whole added effect, or the part of an effect forgemagé over its base maximum
    const exo = e.status === "added" ? v : e.status === "above" && e.range ? Math.max(0, v - e.range[1]) : 0;
    if (exo > 0 && !negative(e)) add(m, EXO_BASE + e.stat, exo);
  }
  return m;
}

/** Theoretical maximum of an item: top of its default ranges. */
export function itemStats(it: ItemInfo): Totals {
  const m: Totals = new Map();
  for (const e of it.effects) if (countable(e.group)) add(m, e.stat, negative(e) ? -e.max : e.max);
  return m;
}

export function setStats(sets: Record<string, SetInfo>, counts: Map<number, number>): Totals {
  const m: Totals = new Map();
  for (const [sid, n] of counts) for (const e of sets[sid]?.effects[n - 1] ?? []) if (countable(e.group)) add(m, e.stat, negative(e) ? -e.max : e.max);
  return m;
}

/** Pieces worn per set (distinct items only). */
export function setCounts(info: Info, gids: number[]): Map<number, number> {
  const c = new Map<number, number>();
  for (const g of new Set(gids)) {
    const s = info.items[g]?.set_id;
    if (s != null) c.set(s, (c.get(s) ?? 0) + 1);
  }
  return c;
}

export interface Resolved { slot: string; gid: number; lot: Lot | null; stats: Totals }

/** Everything that adds to the characteristics apart from the items: base, scrolls, exo, level. */
export function fixedStats(b: Build, withFm = true): Totals {
  const m: Totals = new Map();
  for (const [stat, v] of Object.entries(BASE_STATS)) add(m, +stat, v);
  for (const [code, stat] of CARAC) add(m, stat, (b.carac[`base_${code}`] ?? 0) + (b.carac[`scroll_${code}`] ?? 0));
  if (withFm) for (const [code, v] of Object.entries(b.fm ?? {})) { const stat = FM_STATS[code]?.stat; if (stat != null && v) add(m, stat, v); }
  return m;
}

export interface Computed { totals: Totals; items: Totals; fixed: Totals; hp: number; unmapped: Map<number, number> }

/** Panel values from the resolved slots. `lines` = per-slot item stats (lot when chosen, else the theoretical maximum). */
export function compute(b: Build, info: Info, resolved: Resolved[]): Computed {
  const fixed = fixedStats(b, false);
  const items: Totals = new Map();
  for (const x of resolved) for (const [s, v] of x.stats) add(items, s, v);
  for (const [s, v] of setStats(info.sets, setCounts(info, resolved.map((x) => x.gid)))) add(items, s, v);
  // forgemagie: free unless the chosen lots already carry that exo themselves (then it is inside their stats)
  for (const [code, v] of Object.entries(b.fm ?? {})) { const stat = FM_STATS[code]?.stat; if (stat != null && v) add(fixed, stat, Math.max(0, v - (items.get(EXO_BASE + stat) ?? 0))); }
  const totals: Totals = new Map(fixed);
  for (const [s, v] of items) add(totals, s, v);
  const sa = totals.get(12) ?? 0, ag = totals.get(14) ?? 0;
  // derived values (verified against the Dofusbook sheet): initiative = main stats, dodge / tackle / withdrawal = wisdom or agility / 10, pods = force x 5
  add(totals, 44, (totals.get(10) ?? 0) + (totals.get(15) ?? 0) + (totals.get(13) ?? 0) + ag);
  for (const s of [27, 28, 82, 83]) add(totals, s, Math.floor(sa / 10));
  for (const s of [78, 79]) add(totals, s, Math.floor(ag / 10));
  add(totals, 40, 5 * (totals.get(10) ?? 0));
  add(totals, 48, Math.floor((totals.get(13) ?? 0) / 10)); // prospection: 1 per 10 chance
  const known = new Set(Object.values(PANELS).flat().map((x) => x.stat));
  const unmapped: Totals = new Map([...items].filter(([s, v]) => s > 0 && s < EXO_BASE && !known.has(s) && v !== 0));
  return { totals, items, fixed, hp: 50 + 5 * b.level + (totals.get(11) ?? 0), unmapped };
}

// ---- prices -----------------------------------------------------------------------------------------------------------

/** Real prices only: a manual override or the price read in the auction house. Weekly averages are estimates and are not used. */
export const realPrice = (prices: Map<number, StoredPrice>, gid: number): number | null => {
  const p = prices.get(gid);
  return p ? (p.override ?? p.unit ?? null) : null;
};

export interface SlotPrice { craft: number | null; estimated: boolean; first: number | null; chosen: number | null; missing: string[]; scanned: boolean }

export function slotPrices(gid: number, lots: Lot[], chosenUid: number | null | undefined, recipes: Recipes, prices: Map<number, StoredPrice>): SlotPrice {
  // craft cost needs no scan of the item: ingredients use override > AH price > weekly average (only items themselves never get estimated prices)
  const c = craftCost(gid, recipes, prices);
  // items without a recipe (Dofus, ...) can't be crafted: their price is bought, so the stored price (weekly average included) stands in
  const uncraftable = !recipes.recipes[gid];
  const stored = uncraftable ? prices.get(gid) : undefined;
  const bought = stored ? (stored.override ?? stored.unit ?? stored.avg ?? null) : null;
  const units = lots.map(lotUnit).filter((x): x is number => x != null);
  const ch = chosenUid != null ? lots.find((l) => l.uid === chosenUid) : null;
  return { craft: uncraftable ? bought : c?.cost ?? null, estimated: uncraftable && bought != null && stored?.override == null && stored?.unit == null, first: units.length ? Math.min(...units) : null, chosen: ch ? lotUnit(ch) : null, missing: c?.missing ?? [], scanned: lots.length > 0 };
}

export interface Totalled { total: number; priced: number; missingSlots: string[] }
/** Sum of one price per slot over the non-ignored slots; slots without a real price are listed, never estimated. */
export function sumSlots(slots: string[], ignored: Set<string>, pick: (slot: string) => number | null): Totalled {
  let total = 0, priced = 0;
  const missingSlots: string[] = [];
  for (const s of slots) {
    if (ignored.has(s)) continue;
    const v = pick(s);
    if (v == null) missingSlots.push(s);
    else { total += v; priced++; }
  }
  return { total, priced, missingSlots };
}

// ---- solver -----------------------------------------------------------------------------------------------------------

export interface Cand { slot: string; gid: number; uid: number; price: number; stats: Totals }
export interface SolveResult { picks: Record<string, number>; cost: number; totals: Totals; approx: boolean; nodes: number; fixedSlots: string[]; }

/** Cheapest lot combination whose stat totals reach `targets` (stat -> minimum). One lot per free slot; ignored slots and slots without any scanned
 *  lot keep their current item (chosen lot, else theoretical maximum) at no cost. Set bonuses are tracked incrementally. Returns null when infeasible. */
export function solve(b: Build, info: Info, lotsOf: (slot: string) => Lot[], targets: Record<string, number>, nodeLimit = 4_000_000, progress?: (fraction: number) => void): SolveResult | null {
  const tgt = Object.entries(targets).map(([s, v]) => [+s, v] as const).filter(([, v]) => Number.isFinite(v) && v > 0);
  // the build's forgemagie is expected on the items: each fm stat looks for that much exo (added, or forged over the base max) among the chosen lots
  for (const [code, v] of Object.entries(b.fm ?? {})) { const stat = FM_STATS[code]?.stat; if (stat != null && v > 0) tgt.push([EXO_BASE + stat, v] as const); }
  const stats = tgt.map(([s]) => s);
  const slots = Object.keys(b.slots);
  const ignored = new Set(b.ignored);
  const fixedSlots: string[] = [];
  const free: { slot: string; cands: Cand[] }[] = [];
  const base: Totals = fixedStats(b, false);
  const baseGids: number[] = [];
  const fixedSet: Totals = new Map();

  const resolveFixed = (slot: string) => {
    const gid = b.slots[slot].gid;
    const lot = lotsOf(slot).find((l) => l.uid === b.chosen[slot]);
    for (const [s, v] of lot ? lotStats(lot) : itemStats(info.items[gid] ?? { effects: [] } as any)) add(base, s, v);
    baseGids.push(gid);
    fixedSlots.push(slot);
  };
  for (const slot of slots) {
    const gid = b.slots[slot].gid;
    const lots = lotsOf(slot).filter((l) => lotUnit(l) != null);
    if (ignored.has(slot) || !lots.length) { resolveFixed(slot); continue; }
    // dominance inside one item: same set membership, so a lot that is dearer and no better on the targeted stats can go
    let cands: Cand[] = lots.map((l) => ({ slot, gid: l.gid ?? gid, uid: l.uid, price: lotUnit(l)!, stats: lotStats(l) }));
    cands.sort((x, y) => x.price - y.price);
    const kept: Cand[] = [];
    for (const c of cands) if (!kept.some((k) => k.gid === c.gid && stats.every((s) => (k.stats.get(s) ?? 0) >= (c.stats.get(s) ?? 0)))) kept.push(c);
    free.push({ slot, cands: kept });
  }
  void fixedSet;
  free.sort((x, y) => x.cands.length - y.cands.length);

  const need = tgt.map(([s, v]) => v - (base.get(s) ?? 0));
  const setOf = (g: number) => info.items[g]?.set_id ?? null;
  // set bonus table: sets reachable by a free slot or by a fixed piece
  const sets = info.sets;
  const baseCounts = setCounts(info, baseGids);
  const bonusAt = (sid: number, n: number): Totals => {
    const m: Totals = new Map();
    for (const e of sets[sid]?.effects[n - 1] ?? []) if (countable(e.group)) add(m, e.stat, negative(e) ? -e.max : e.max);
    return m;
  };
  // fixed pieces already earn their set bonus
  const startTotals = stats.map(() => 0);
  const bonusDelta = (sid: number, from: number, to: number) => stats.map((s) => (bonusAt(sid, to).get(s) ?? 0) - (from > 0 ? bonusAt(sid, from).get(s) ?? 0 : 0));
  for (const [sid, n] of baseCounts) bonusDelta(sid, 0, n).forEach((d, i) => (startTotals[i] += d));

  const maxStat = free.map((f) => stats.map((s) => Math.max(...f.cands.map((c) => c.stats.get(s) ?? 0))));
  const suffixMax = stats.map((_, i) => { const a = new Array(free.length + 1).fill(0); for (let k = free.length - 1; k >= 0; k--) a[k] = a[k + 1] + maxStat[k][i]; return a; });
  const maxBonus = stats.map((s) => { let t = 0; for (const sid of Object.keys(sets)) { const eff = sets[sid].effects; let m = 0; for (let n = 1; n <= eff.length; n++) m = Math.max(m, bonusAt(+sid, n).get(s) ?? 0); t += m; } return t; });
  const minCost = new Array(free.length + 1).fill(0);
  for (let k = free.length - 1; k >= 0; k--) minCost[k] = minCost[k + 1] + free[k].cands[0].price;

  // Fractional-knapsack lower bound: relative to each slot's cheapest lot, an upgrade costs dc for +ds on a stat; taking upgrades independently (and
  // fractionally, best ds/dc first) relaxes "one lot per slot", so the cost it needs to cover the missing stat can only underestimate the real one.
  const s0 = stats.map((s) => { const a = new Array(free.length + 1).fill(0); for (let k = free.length - 1; k >= 0; k--) a[k] = a[k + 1] + (free[k].cands[0].stats.get(s) ?? 0); return a; });
  const ups = stats.map((s) => {
    const byK: { dc: number; ds: number }[][] = new Array(free.length + 1).fill(0).map(() => []);
    for (let k = free.length - 1; k >= 0; k--) {
      const c0 = free[k].cands[0], b0 = c0.stats.get(s) ?? 0;
      const own = free[k].cands.slice(1).map((c) => ({ dc: c.price - c0.price, ds: (c.stats.get(s) ?? 0) - b0 })).filter((u) => u.ds > 0);
      byK[k] = [...byK[k + 1], ...own].sort((x, y) => y.ds / Math.max(y.dc, 1e-9) - x.ds / Math.max(x.dc, 1e-9));
    }
    return byK;
  });
  const extraCost = (i: number, k: number, deficit: number) => {
    let left = deficit - s0[i][k] - maxBonus[i], cost = 0;
    if (left <= 0) return 0;
    for (const u of ups[i][k]) {
      if (u.ds >= left) return cost + (u.dc * left) / u.ds;
      cost += u.dc; left -= u.ds;
    }
    return Infinity;
  };

  let best: { cost: number; picks: Cand[] } | null = null;
  let nodes = 0, approx = false;
  const counts = new Map(baseCounts);
  const cur = startTotals.slice();
  const chosen: Cand[] = [];
  let rootDone = 0; // share of the first slot's lots already explored
  const dfs = (k: number, cost: number) => {
    if (nodes++ > nodeLimit) { approx = true; return; }
    if (progress && nodes % 20000 === 0) progress(Math.min(1, Math.max(nodes / nodeLimit, rootDone)));
    if (best && cost + minCost[k] >= best.cost) return;
    for (let i = 0; i < stats.length; i++) {
      if (cur[i] + suffixMax[i][k] + maxBonus[i] < need[i]) return;
      if (best && cost + minCost[k] + extraCost(i, k, need[i] - cur[i]) >= best.cost) return;
    }
    if (k === free.length) {
      if (cur.every((v, i) => v >= need[i])) best = { cost, picks: chosen.slice() };
      return;
    }
    for (const [ci, c] of free[k].cands.entries()) {
      if (k === 0) rootDone = ci / free[0].cands.length;
      if (best && cost + c.price + minCost[k + 1] >= best.cost) break; // sorted by price
      const sid = setOf(c.gid);
      const n0 = sid != null ? counts.get(sid) ?? 0 : 0;
      const dup = chosen.some((x) => x.gid === c.gid && x.uid === c.uid); // the same lot can't be bought twice
      if (dup) continue;
      const delta = sid != null ? bonusDelta(sid, n0, n0 + 1) : null;
      stats.forEach((s, i) => { cur[i] += (c.stats.get(s) ?? 0) + (delta ? delta[i] : 0); });
      if (sid != null) counts.set(sid, n0 + 1);
      chosen.push(c);
      dfs(k + 1, cost + c.price);
      chosen.pop();
      if (sid != null) counts.set(sid, n0);
      stats.forEach((s, i) => { cur[i] -= (c.stats.get(s) ?? 0) + (delta ? delta[i] : 0); });
    }
  };
  dfs(0, 0);
  if (!best) return null;
  const found = best as { cost: number; picks: Cand[] };
  const picks: Record<string, number> = {};
  for (const c of found.picks) picks[c.slot] = c.uid;
  return { picks, cost: found.cost, totals: new Map(), approx, nodes, fixedSlots };
}
