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
/** The character's six characteristics (game ids: 11 vitality, 12 wisdom, 10 strength, 15 intelligence, 13 chance, 14 agility):
 *  points invested (`base`) and scrolls, from CharacterCharacteristicsEvent. */
export interface Character { t: number; carac: Record<string, { base: number; scroll: number }> }
interface Listing { updated: number; items: InvItem[]; character?: Character | null }

class InventoryStore {
  items = $state.raw<InvItem[]>([]);
  /** When the inventory was last sent by the game (login), 0 = never seen. */
  updated = $state(0);
  /** null = characteristics never seen (they are sent at login and after fights / level ups). */
  character = $state.raw<Character | null>(null);
  private ready: Promise<void> | null = null;
  /** Registered once: a failed list is retried without listening twice. */
  private listening: Promise<unknown> | null = null;

  ensure(): Promise<void> {
    return (this.ready ??= (async () => {
      await (this.listening ??= on<Listing>("inventory:changed", (l) => this.set(l)));
      try { this.set(await call<Listing>("inventory", "list")); } catch (e) { this.ready = null; throw e; }
    })());
  }

  private set(l: Listing) {
    this.items = l.items;
    this.updated = l.updated;
    this.character = l.character ?? null;
  }

  ofGid(gid: number): InvItem[] {
    return this.items.filter((i) => i.gid === gid).sort((a, b) => Number(b.equipped) - Number(a.equipped) || a.uid - b.uid);
  }

  byUid(uid: number | null | undefined): InvItem | undefined {
    return uid == null ? undefined : this.items.find((i) => i.uid === uid);
  }
}

export const inventory = new InventoryStore();
