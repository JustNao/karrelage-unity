/**
 * Scanned auction-house listings (crates/modules/src/auction.rs), one per item, shared by the HDV, Stuff, Brisage and
 * Montures tabs: fetched once, kept up to date by one set of listeners for the whole session (`SyncedMap`).
 *
 *   auction.ensure();                // in a view's onMount (idempotent)
 *   auction.items                    // gid -> latest listing (reactive SvelteMap; a scan replaces the entry whole)
 *   auction.previous                 // gid -> the last listing with other lots than the latest (this session: the HDV "Modifications")
 *   auction.subscribe((c) => ...)    // { kind: "put", values: [listing], live } for each listing set, { kind: "clear" }
 */
import { SvelteMap, SvelteSet } from "svelte/reactivity";
import { call, on } from "../api";
import { lotDiff } from "../lotDiff";
import { SyncedMap } from "./synced.svelte";

export type { SyncChange as AuctionChange } from "./synced.svelte";

const previous = new SvelteMap<number, any>();
/** Some lot added, removed or repriced between two listings of an item. */
const changed = (old: any, now: any) => {
  const d = lotDiff(old.rows ?? [], now.rows ?? []);
  return d.added.size + d.removed.length + d.was.size > 0;
};

class AuctionStore extends SyncedMap<number, any> {
  constructor() {
    super({
      list: ["auction", "items"],
      key: (i) => i.gid,
      put: ["auction:item"],
      cleared: "auction:cleared",
      fromList: (old, listed) => old.updated <= listed.updated, // an item scanned while the list was loading stays
      replaced: (old, now) => changed(old, now) && previous.set(old.gid, old), // a scan finding the same lots keeps the earlier one
    });
    this.subscribe((c) => c.kind === "clear" && previous.clear());
  }
  /** gid -> latest listing. */
  get items() {
    return this.map;
  }
  /** gid -> the last listing whose lots differ from the latest one's. */
  get previous() {
    return previous;
  }
}

export const auction = new AuctionStore();

interface MineLot { uid: number; sell_id?: number | null; gid: number; prices?: number[]; effects?: [number, number | null][] }
/** Sorted [action, highest value] of a lot's stats, as text. */
const effectSig = (effects: [number, number | null][]) => effects.map(([a, hi]) => `${a}:${hi ?? ""}`).join(",");
const rowSig = (r: any) =>
  effectSig(
    (r.effects ?? [])
      .filter((e: any) => e.status !== "removed")
      .map((e: any): [number, number | null] => [e.action, e.hi ?? null])
      .sort((a: [number, number | null], b: [number, number | null]) => a[0] - b[0] || (a[1] ?? -Infinity) - (b[1] ?? -Infinity)),
  );

/**
 * The player's own lots on sale (auction.rs `mine`: the AH sell window's list + the lots put on sale since). Their object
 * uid is not the listing's: a listing row is one of them by its sell id, else by gid + price + identical stats.
 */
class MineStore {
  private sellIds = new SvelteSet<number>();
  /** "gid|price|stats" */
  private keys = new SvelteSet<string>();
  private ready: Promise<void> | null = null;
  private set(lots: MineLot[]) {
    this.sellIds.clear();
    this.keys.clear();
    for (const l of lots) {
      if (l.sell_id) this.sellIds.add(l.sell_id);
      for (const p of l.prices ?? []) if (p > 0) this.keys.add(`${l.gid}|${p}|${effectSig(l.effects ?? [])}`);
    }
  }
  /** Listener first, then the list (unless an event already gave a newer one). */
  ensure(): Promise<void> {
    return (this.ready ??= (async () => {
      let live = false;
      await on<MineLot[]>("auction:mine", (l) => ((live = true), this.set(l)));
      const l = await call<MineLot[]>("auction", "mine");
      if (!live) this.set(l);
    })().catch(() => void (this.ready = null)));
  }
  /** A row of an AH listing (`gid`: the listing's, when the row has none). */
  has(row: any, gid?: number): boolean {
    if (row.uid != null && this.sellIds.has(row.uid)) return true;
    const price = row.prices?.[0]?.price;
    return !!price && this.keys.has(`${row.gid ?? gid}|${price}|${rowSig(row)}`);
  }
}

export const mine = new MineStore();
