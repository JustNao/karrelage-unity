/**
 * Scanned auction-house listings (crates/modules/src/auction.rs), one per item, shared by the HDV, Stuff, Brisage and
 * Montures tabs: fetched once, kept up to date by one set of listeners for the whole session (`SyncedMap`).
 *
 *   auction.ensure();                // in a view's onMount (idempotent)
 *   auction.items                    // gid -> latest listing (reactive SvelteMap; a scan replaces the entry whole)
 *   auction.subscribe((c) => ...)    // { kind: "put", values: [listing], live } for each listing set, { kind: "clear" }
 */
import { SvelteSet } from "svelte/reactivity";
import { call, on } from "../api";
import { SyncedMap } from "./synced.svelte";

export type { SyncChange as AuctionChange } from "./synced.svelte";

class AuctionStore extends SyncedMap<number, any> {
  constructor() {
    super({
      list: ["auction", "items"],
      key: (i) => i.gid,
      put: ["auction:item"],
      cleared: "auction:cleared",
      fromList: (old, listed) => old.updated <= listed.updated, // an item scanned while the list was loading stays
    });
  }
  /** gid -> latest listing. */
  get items() {
    return this.map;
  }
}

export const auction = new AuctionStore();

/** uids of the player's own lots on sale (auction.rs `mine`: the AH sell window's list + the lots put on sale since). */
class MineStore {
  readonly uids = new SvelteSet<number>();
  private ready: Promise<void> | null = null;
  private set(lots: { uid: number }[]) {
    this.uids.clear();
    for (const l of lots) this.uids.add(l.uid);
  }
  /** Listener first, then the list (unless an event already gave a newer one). */
  ensure(): Promise<void> {
    return (this.ready ??= (async () => {
      let live = false;
      await on<{ uid: number }[]>("auction:mine", (l) => ((live = true), this.set(l)));
      const l = await call<{ uid: number }[]>("auction", "mine");
      if (!live) this.set(l);
    })().catch(() => void (this.ready = null)));
  }
  has(uid: any): boolean {
    return uid != null && this.uids.has(uid);
  }
}

export const mine = new MineStore();
