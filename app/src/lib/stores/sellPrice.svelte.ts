/**
 * HDV « Prix de vente copié » (Automation tab): the backend (crates/modules/src/sell_price.rs) follows the rule's switch and
 * writes the clipboard itself (the game has the focus while selling: the webview's clipboard needs ours); each copy is sent as
 * `sellprice:copied`, shown under the rule (`last`) and, if asked, heard.
 */
import { call, on } from "../api";
import { playSound } from "../sounds";
import { rules } from "./automation.svelte";

/** A copy: the resource, how many the inventory holds, the lot size the game selects, its cheapest lot and the price copied
 *  (null = nothing on sale at that size, clipboard left as it was). */
export interface SellCopy { gid: number; name: string; quantity: number; size: number; min: number; price: number | null }

class SellPriceState {
  last = $state<SellCopy | null>(null);
}
export const sellPrice = new SellPriceState();

let started = false;
export function startSellPrice() {
  if (started) return;
  started = true;
  $effect.root(() => {
    $effect(() => {
      const on = rules.sellPrice.enabled;
      call("sellprice", "set_enabled", { on }, true).catch(() => {});
    });
  });
  void on<SellCopy>("sellprice:copied", (c) => {
    sellPrice.last = c;
    const r = rules.sellPrice;
    if (r.enabled && r.withSound && c.price != null) playSound(r.sound, r.volume);
  });
}
