/**
 * The character's equipment, worn and in the bag (crates/modules/src/inventory.rs): read from the inventory the game sends at login
 * and kept up to date by the object added / modified / deleted events. Items are shaped like auction lots (`effects`, empty `prices`).
 *
 *   inventory.ensure();          // in a view's onMount (idempotent)
 *   inventory.ofGid(gid)         // the copies of an item the character owns, worn ones first
 */
import { call, on } from "../api";
import type { Lot } from "../stuff";

export interface InvItem extends Lot { name: string; level: number | null; position: number; equipped: boolean; quantity: number; updated: number }
interface Listing { updated: number; items: InvItem[] }

class InventoryStore {
  items = $state.raw<InvItem[]>([]);
  /** When the inventory was last sent by the game (login), 0 = never seen. */
  updated = $state(0);
  private ready: Promise<void> | null = null;

  ensure(): Promise<void> {
    return (this.ready ??= (async () => {
      await on<Listing>("inventory:changed", (l) => this.set(l));
      try { this.set(await call<Listing>("inventory", "list")); } catch (e) { this.ready = null; throw e; }
    })());
  }

  private set(l: Listing) {
    this.items = l.items;
    this.updated = l.updated;
  }

  ofGid(gid: number): InvItem[] {
    return this.items.filter((i) => i.gid === gid).sort((a, b) => Number(b.equipped) - Number(a.equipped) || a.uid - b.uid);
  }

  byUid(uid: number | null | undefined): InvItem | undefined {
    return uid == null ? undefined : this.items.find((i) => i.uid === uid);
  }
}

export const inventory = new InventoryStore();
