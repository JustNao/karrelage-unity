/**
 * Scanned auction-house listings (crates/modules/src/auction.rs), one per item, shared by the HDV, Stuff, Brisage and
 * Montures tabs: fetched once, kept up to date by one set of listeners for the whole session (`SyncedMap`).
 *
 *   auction.ensure();                // in a view's onMount (idempotent)
 *   auction.items                    // gid -> latest listing (reactive SvelteMap; a scan replaces the entry whole)
 *   auction.subscribe((c) => ...)    // { kind: "put", values: [listing], live } for each listing set, { kind: "clear" }
 */
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
