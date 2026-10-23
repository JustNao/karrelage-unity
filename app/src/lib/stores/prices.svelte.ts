/**
 * Stored prices (crates/modules/src/prices.rs), shared by every tab: fetched once, kept up to date by one set of listeners
 * for the whole session (tabs used to refetch the whole list and subscribe again each time they opened).
 *
 *   prices.ensure();                 // in a view's onMount (idempotent)
 *   prices.map                       // gid -> StoredPrice (reactive SvelteMap)
 *   prices.effective(gid)            // { value, source } with the priority override > unit > avg
 *   prices.effective(gid, { avg: false })   // real prices only (Stuff)
 *   prices.subscribe((c) => ...)     // { kind: "put", values, live } / { kind: "clear" } (see `SyncedMap`)
 */
import { effectivePrice, type StoredPrice } from "../price";
import { SyncedMap } from "./synced.svelte";

export type { StoredPrice } from "../price";
export { effectivePrice } from "../price";
export type { SyncChange as PriceChange } from "./synced.svelte";

class PriceStore<P extends StoredPrice = StoredPrice> extends SyncedMap<number, P> {
  constructor() {
    // a price event received while the list loads is newer: the listed one is dropped (default `fromList`)
    super({ list: ["prices", "list"], key: (p) => p.gid, put: ["prices:price"], bulk: ["prices:bulk"], cleared: "prices:cleared" });
  }

  /** The price used for an item and its source (see `effectivePrice`). */
  effective(gid: number, opts?: { avg?: boolean }) {
    return effectivePrice(this.map.get(gid), opts);
  }
  /** Just the value of `effective`. */
  price(gid: number, opts?: { avg?: boolean }): number | null {
    return this.effective(gid, opts)?.value ?? null;
  }
}

/** Shape of the entries `prices.list` returns (the Prices tab shows the extra fields). */
export interface PriceRow extends StoredPrice { name: string; type_id: number | null; type: string; category: string }

export const prices = new PriceStore<PriceRow>();
