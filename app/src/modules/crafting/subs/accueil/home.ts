/**
 * Crafting « Accueil »: shared data of its cards. `homeItems` = prices.rs `home_items` (consumables restoring life / energy,
 * characteristic scrolls), loaded once per session.
 */
import { cached } from "../../../../lib/api";

export interface Consumable {
  gid: number;
  name: string;
  level: number;
  type: string;
  /** life / energy restored (mean of the roll) */
  life: number;
  energy: number;
}
export interface HomeItems {
  consumables: Consumable[];
  scrolls: { gid: number; name: string }[];
}
export const homeItems = () => cached<HomeItems>("prices", "home_items");

/** Copy an item's name (to paste in the auction house search). */
export const copyName = (name: string) => navigator.clipboard?.writeText(name).catch(() => {});
