/**
 * Scanned auction-house listings (crates/modules/src/auction.rs), one per item, shared by the HDV, Stuff and Montures tabs:
 * fetched once, kept up to date by one set of listeners for the whole session.
 *
 *   auction.ensure();                // in a view's onMount (idempotent)
 *   auction.items                    // gid -> latest listing (reactive SvelteMap; a scan replaces the entry whole)
 *   auction.subscribe((c) => ...)    // { kind: "item", item, live } for each listing set, { kind: "clear" }
 */
import { SvelteMap } from "svelte/reactivity";
import { call, on } from "../api";

/** `live` = just captured from the game (not part of the startup list). */
export type AuctionChange = { kind: "item"; item: any; live: boolean } | { kind: "clear" };

class AuctionStore {
  readonly items = new SvelteMap<number, any>();
  /** Resolves once the first list is in. */
  ready: Promise<void> | null = null;
  private subs = new Set<(c: AuctionChange) => void>();

  /** Starts the store the first time it is called (listeners first, then the list, so nothing falls between the two). */
  ensure(): Promise<void> {
    return (this.ready ??= (async () => {
      await (this.listening ??= this.listen());
      const e = this.epoch;
      try {
        const l = await call<any[]>("auction", "items");
        if (e !== this.epoch) return; // cleared while loading
        for (const i of l) {
          const old = this.items.get(i.gid);
          if (!old || old.updated <= i.updated) this.put(i, false); // an item scanned while the list was loading stays
        }
      } catch (err) {
        this.ready = null;
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
      on<any>("auction:item", (i) => this.put(i, true)),
      on("auction:cleared", () => {
        this.epoch++;
        this.items.clear();
        this.emit({ kind: "clear" });
      }),
    ]);
  }

  private put(item: any, live: boolean) {
    this.items.set(item.gid, item);
    this.emit({ kind: "item", item, live });
  }
  private emit(c: AuctionChange) {
    for (const f of this.subs) f(c);
  }

  /** Be told about every listing set and every clear. Returns the unsubscribe function. */
  subscribe(f: (c: AuctionChange) => void) {
    this.subs.add(f);
    return () => void this.subs.delete(f);
  }
}

export const auction = new AuctionStore();
