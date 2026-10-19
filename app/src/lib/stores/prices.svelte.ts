/**
 * Stored prices (crates/modules/src/prices.rs), shared by every tab: fetched once, kept up to date by one set of listeners
 * for the whole session (tabs used to refetch the whole list and subscribe again each time they opened).
 *
 *   prices.ensure();                 // in a view's onMount (idempotent)
 *   prices.map                       // gid -> StoredPrice (reactive SvelteMap)
 *   prices.effective(gid)            // { value, source } with the priority override > unit > avg
 *   prices.effective(gid, { avg: false })   // real prices only (Stuff)
 */
import { SvelteMap } from "svelte/reactivity";
import { call, on } from "../api";
import { effectivePrice, type StoredPrice } from "../price";

export type { StoredPrice } from "../price";
export { effectivePrice } from "../price";

/** What a change listener is told: these prices were set (in arrival order), or everything was forgotten. */
export type PriceChange<P = StoredPrice> = { kind: "put"; prices: P[] } | { kind: "clear" };

class PriceStore<P extends StoredPrice = StoredPrice> {
  readonly map = new SvelteMap<number, P>();
  /** Resolves once the first list is in. */
  ready: Promise<void> | null = null;
  private subs = new Set<(c: PriceChange<P>) => void>();

  /** Starts the store the first time it is called (listeners first, then the list, so nothing falls between the two). */
  ensure(): Promise<void> {
    return (this.ready ??= (async () => {
      await (this.listening ??= this.listen());
      const e = this.epoch;
      try {
        const l = await call<P[]>("prices", "list");
        if (e !== this.epoch) return; // cleared while loading: the list is out of date
        this.put(l.filter((p) => !this.map.has(p.gid))); // a price event received meanwhile is newer
      } catch (err) {
        this.ready = null; // try again next time
        throw err;
      }
    })());
  }

  /** Listeners for the whole session, registered once. */
  private listening: Promise<unknown> | null = null;
  /** Bumped by each clear: a list requested before it is out of date. */
  private epoch = 0;
  private listen() {
    return Promise.all([
      on<P>("prices:price", (p) => this.put([p])),
      on<P[]>("prices:bulk", (l) => this.put(l)), // weekly averages preloaded by the game
      on("prices:cleared", () => {
        this.epoch++;
        this.map.clear();
        this.emit({ kind: "clear" });
      }),
    ]);
  }

  private put(l: P[]) {
    for (const p of l) this.map.set(p.gid, p);
    if (l.length) this.emit({ kind: "put", prices: l });
  }
  private emit(c: PriceChange<P>) {
    for (const f of this.subs) f(c);
  }

  /** Be told about every change (the Prices tab updates only the groups that changed). Returns the unsubscribe function. */
  subscribe(f: (c: PriceChange<P>) => void) {
    this.subs.add(f);
    return () => void this.subs.delete(f);
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
