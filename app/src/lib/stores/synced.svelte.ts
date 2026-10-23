/**
 * A backend list kept in sync for the whole session: one `SvelteMap` filled by a list call and kept up to date by events,
 * shared by every view (fetched once, one set of listeners). The base of the auction and prices stores.
 *
 *   class MyStore extends SyncedMap<number, Row> {
 *     constructor() { super({ list: ["module", "list"], key: (r) => r.gid, put: ["module:row"], bulk: ["module:rows"], cleared: "module:cleared" }); }
 *   }
 *   store.ensure();               // in a view's onMount (idempotent)
 *   store.map                     // reactive SvelteMap (a value is replaced whole, never mutated)
 *   store.subscribe((c) => ...)   // { kind: "put", values, live } for each change, { kind: "clear" }
 */
import { SvelteMap } from "svelte/reactivity";
import { call, on } from "../api";

export type SyncChange<V> = { kind: "put"; values: V[]; live: boolean } | { kind: "clear" };

export interface SyncSpec<K, V> {
  /** [module, method] returning the whole list. */
  list: [string, string];
  key: (v: V) => K;
  /** Events carrying one value. */
  put?: string[];
  /** Events carrying an array of values. */
  bulk?: string[];
  /** Event clearing everything. */
  cleared?: string;
  /** A value of the list loaded after `old` was received from an event: keep it? (default: the event's is newer, kept) */
  fromList?: (old: V, listed: V) => boolean;
}

export class SyncedMap<K, V> {
  readonly map = new SvelteMap<K, V>();
  /** Resolves once the first list is in. */
  ready: Promise<void> | null = null;
  private subs = new Set<(c: SyncChange<V>) => void>();
  /** Listeners for the whole session, registered once (a failed list is retried, never listened to twice). */
  private listening: Promise<unknown> | null = null;
  /** Bumped by each clear: a list requested before it is out of date. */
  private epoch = 0;

  constructor(private spec: SyncSpec<K, V>) {}

  /** Starts the store the first time it is called (listeners first, then the list, so nothing falls between the two). */
  ensure(): Promise<void> {
    return (this.ready ??= (async () => {
      await (this.listening ??= this.listen());
      const e = this.epoch;
      try {
        const l = await call<V[]>(...this.spec.list);
        if (e !== this.epoch) return; // cleared while loading
        const keep = this.spec.fromList ?? (() => false);
        this.put(
          l.filter((v) => {
            const old = this.map.get(this.spec.key(v));
            return old === undefined || keep(old, v);
          }),
          false,
        );
      } catch (err) {
        this.ready = null; // try again next time
        throw err;
      }
    })());
  }

  private listen() {
    const s = this.spec;
    return Promise.all([
      ...(s.put ?? []).map((ev) => on<V>(ev, (v) => this.put([v], true))),
      ...(s.bulk ?? []).map((ev) => on<V[]>(ev, (l) => this.put(l, true))),
      ...(s.cleared
        ? [
            on(s.cleared, () => {
              this.epoch++;
              this.map.clear();
              this.emit({ kind: "clear" });
            }),
          ]
        : []),
    ]);
  }

  private put(values: V[], live: boolean) {
    for (const v of values) this.map.set(this.spec.key(v), v);
    if (values.length) this.emit({ kind: "put", values, live });
  }
  private emit(c: SyncChange<V>) {
    for (const f of this.subs) f(c);
  }

  /** Be told about every change. Returns the unsubscribe function. */
  subscribe(f: (c: SyncChange<V>) => void) {
    this.subs.add(f);
    return () => void this.subs.delete(f);
  }
}
