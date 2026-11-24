import { openFloating, savedPos } from "./floating";
import { writeJSON } from "./persist.svelte";
import type { Costs } from "./jobXp";

/** The floating "to buy" window (shoplist.html, src/ShopList.svelte), fed through this shared localStorage key (Crafting
 *  « XP métiers », Brisage « Crafts »). */
export const SHOP_LIST_KEY = "karrelage.shoplist";
/** Where the user left the window, reused when it opens again. */
export const SHOP_LIST_POS_KEY = "karrelage.shoplist.pos";
export interface ShopItem {
  gid: number;
  name: string;
  /** quantity needed (with `stock`: in full, the window deducts what the inventory and the bank hold) */
  qty: number;
  /** unit price used (null: none known) */
  unit: number | null;
}
export interface ShopList {
  title: string;
  buy: ShopItem[];
  /** intermediate items to craft on the way (unit null) */
  craft: ShopItem[];
  /** gids ticked off in the window (by hand, or once the quantity is owned / bought) */
  done: number[];
  /** bought at the auction house since the list opened (gid -> quantity; `purchases:bought`, crates/modules/src/purchases.rs) */
  got?: Record<string, number>;
  /** the inventory + bank (inventory.rs `owned`) counts towards each row, read live by the window */
  stock?: boolean;
  /** with `stock`: what was owned when the list opened (gid -> quantity; purchases count on top of it when the inventory is
   *  not followed) */
  had?: Record<string, number>;
}

/** Inventory + bank as `inventory.owned` gives it. */
export interface Owned {
  items: Record<string, number>;
  inventory: boolean;
  /** `t` = when the bank was last seen (unix s, null = never), `source` = "hdv" (merged at an auction house) / "bank" (its
   *  window), `items` = its own gid -> quantity */
  bank: { t: number | null; count: number; source?: "hdv" | "bank" | null; items?: Record<string, number> | null };
}

/** Show this list in the floating window (an open window picks it up through the storage event). */
export async function openShopList(list: Omit<ShopList, "done" | "got">) {
  writeJSON(SHOP_LIST_KEY, { ...list, done: [], got: {} } satisfies ShopList);
  await openFloating("shoplist", { url: "shoplist.html", title: "À acheter", width: 360, height: 520, pos: savedPos(SHOP_LIST_POS_KEY) });
}

/** The list's rows for these needs (gid -> quantity): resources to buy in full (the window deducts the stock), intermediate
 *  crafts on the way (`stock`'s own intermediates used first: `Costs.shop`), by name. */
export function shopRows(costs: Costs, needs: Iterable<[number, number]>, name: (gid: number) => string, stock?: Map<number, number>): Pick<ShopList, "buy" | "craft"> {
  const { buy, craft } = costs.shop(needs, stock);
  const rows = (m: Map<number, number>, priced: boolean) =>
    [...m].map(([gid, qty]) => ({ gid, name: name(gid), qty, unit: priced ? (costs.best(gid).unit ?? null) : null })).sort((a, b) => a.name.localeCompare(b.name, "fr"));
  return { buy: rows(buy, true), craft: rows(craft, false) };
}
