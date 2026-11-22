import { openFloating, savedPos } from "./floating";
import { writeJSON } from "./persist.svelte";

/** The floating "to buy" window (shoplist.html, src/ShopList.svelte), fed through this shared localStorage key (Crafting « XP métiers »). */
export const SHOP_LIST_KEY = "karrelage.shoplist";
/** Where the user left the window, reused when it opens again. */
export const SHOP_LIST_POS_KEY = "karrelage.shoplist.pos";
export interface ShopItem {
  gid: number;
  name: string;
  qty: number;
  /** unit price used (null: none known) */
  unit: number | null;
}
export interface ShopList {
  title: string;
  buy: ShopItem[];
  /** intermediate items to craft on the way (unit null) */
  craft: ShopItem[];
  /** gids ticked off in the window (by hand, or once `got` reaches the quantity) */
  done: number[];
  /** bought at the auction house since the list opened (gid -> quantity; `purchases:bought`, crates/modules/src/purchases.rs) */
  got?: Record<string, number>;
}

/** Show this list in the floating window (an open window picks it up through the storage event). */
export async function openShopList(list: Omit<ShopList, "done" | "got">) {
  writeJSON(SHOP_LIST_KEY, { ...list, done: [], got: {} } satisfies ShopList);
  await openFloating("shoplist", { url: "shoplist.html", title: "À acheter", width: 340, height: 520, pos: savedPos(SHOP_LIST_POS_KEY) });
}
