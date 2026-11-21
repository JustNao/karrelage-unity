//! HDV sell price helper (Automation « Prix de vente copié »): a resource picked in the auction house's sell window
//! (ExchangeBidPriceEvent: the cheapest price per lot size) puts the price to type in the clipboard: the cheapest lot of the
//! largest size the inventory holds enough for (what the game selects: 346 owned -> 100) minus 1 kamas. Each sale makes the
//! stack smaller (ObjectQuantityEvent...); when it falls below that size (100 -> 10) the price of the next size down is copied.
//! The listing's changes (ExchangeBidHouseInListUpdatedEvent) keep the prices current; a size whose cheapest lot is at the
//! price copied before (the player's own lot, put on sale at it) copies that price again rather than undercutting it.
use crate::auction::{AH_ROW_UPDATED, AH_SELLER};
use crate::inventory::{quantity_of, Stacks, ADDED, ADDED_LIST, CONTENT, DELETED, DELETED_LIST, MODIFIED, QUANTITY, QUANTITY_LIST};
use crate::kit::{unknown, Role};
use crate::{Ctx, Interest, Message, Module};
use anyhow::Result;
use loutil_data::items::LOT_SIZES;
use loutil_proto::util::{int, int_list};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

const ID: &str = "sellprice";
pub const PRICE: &str = "ExchangeBidPriceEvent";
const LEAVE: &[&str] = &["ExchangeLeaveEvent", "DialogLeaveEvent"];

/// bindings.json `ah_sell_price`.
#[derive(Deserialize, Clone)]
struct PriceBindings {
    gid: String,
    seller: String,
    prices: String,
}
/// bindings.json `ah_row` (the fields read here).
#[derive(Deserialize, Clone)]
struct RowBindings {
    prices: String,
    ints: Vec<String>,
}

/// The resource open in the sell window.
struct Open {
    gid: i64,
    /// cheapest price per lot size (`LOT_SIZES`), 0 = none on sale
    prices: Vec<i64>,
    /// the lot size last copied for (None = nothing copied yet)
    size: Option<i64>,
}

/// The lot size the game selects for `qty` owned (the largest it can fill) and its index.
fn size_for(qty: i64) -> Option<(usize, i64)> {
    LOT_SIZES.iter().copied().enumerate().rfind(|(_, s)| *s <= qty)
}

/// The price to type for a size whose cheapest lot is `min`: 1 kamas under it, or `min` itself when it is the price copied
/// before for that size (the player's own lot). None when nothing is on sale at that size.
fn price_for(min: i64, copied: Option<i64>) -> Option<i64> {
    match min {
        m if m <= 0 => None,
        m if copied == Some(m) => Some(m),
        1 => Some(1),
        m => Some(m - 1),
    }
}

#[derive(Default)]
pub struct SellPrice {
    enabled: AtomicBool,
    price_bind: Role<PriceBindings>,
    row_bind: Role<RowBindings>,
    stacks: Stacks,
    open: Mutex<Option<Open>>,
    /// (gid, lot size) -> price last copied this session
    copied: Mutex<HashMap<(i64, i64), i64>>,
}

impl SellPrice {
    /// `stacks`: every object's stack size, kept by the inventory module (which sees the messages first).
    pub fn new(stacks: Stacks) -> Self {
        Self { stacks, ..Default::default() }
    }

    /// Copies the price for the size the inventory now fills, when that size changed (or `force`: the resource was just picked).
    fn update(&self, ctx: &Ctx, force: bool) {
        let mut open = self.open.lock();
        let Some(o) = open.as_mut() else { return };
        let qty = quantity_of(&self.stacks, o.gid);
        let Some((i, size)) = size_for(qty) else {
            o.size = None; // none left
            return;
        };
        if !force && o.size == Some(size) {
            return;
        }
        o.size = Some(size);
        let min = o.prices.get(i).copied().unwrap_or(0);
        let mut copied = self.copied.lock();
        let price = price_for(min, copied.get(&(o.gid, size)).copied());
        if let Some(p) = price {
            copied.insert((o.gid, size), p);
            // the clipboard can be held by another program for a moment: retried off the capture thread (left alone by tests)
            if !cfg!(test) {
                let (text, log) = (p.to_string(), ctx.logger());
                std::thread::spawn(move || {
                    if !crate::sys::set_clipboard(&text) {
                        log(format!("{ID}: clipboard not available"));
                    }
                });
            }
        }
        ctx.emit(ID, "copied", json!({"gid": o.gid, "name": ctx.data.item_name(o.gid), "quantity": qty, "size": size, "min": min, "price": price}));
    }
}

impl Module for SellPrice {
    fn id(&self) -> &'static str {
        ID
    }

    fn interest(&self) -> Interest {
        Interest::Types(&[
            PRICE,
            AH_ROW_UPDATED,
            AH_SELLER,
            "ExchangeLeaveEvent",
            "DialogLeaveEvent",
            CONTENT,
            ADDED,
            ADDED_LIST,
            MODIFIED,
            DELETED,
            DELETED_LIST,
            QUANTITY,
            QUANTITY_LIST,
        ])
    }

    /// Off (the Automation rule): nothing decoded for it.
    fn wants(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    fn on_message(&self, m: &Message, ctx: &Ctx) {
        let Some(d) = &m.decoded else { return };
        match m.name.as_str() {
            PRICE => {
                let Some(b) = self.price_bind.get(ctx, "ah_sell_price") else { return };
                let Some(seller) = d.get_field_by_name(&b.seller).and_then(|v| v.as_message().cloned()) else { return };
                let gid = int(d, &b.gid);
                *self.open.lock() = Some(Open { gid, prices: int_list(&seller, &b.prices), size: None });
                self.update(ctx, true);
            }
            AH_ROW_UPDATED => {
                let Some(b) = self.row_bind.get(ctx, "ah_row") else { return };
                let mut open = self.open.lock();
                // the row of the resource open (its gid is one of the row's three unordered ints)
                if let Some(o) = open.as_mut().filter(|o| b.ints.iter().any(|f| int(d, f) == o.gid)) {
                    let prices = int_list(d, &b.prices);
                    if !prices.is_empty() {
                        o.prices = prices;
                    }
                }
            }
            AH_SELLER => *self.open.lock() = None,
            n if LEAVE.contains(&n) => *self.open.lock() = None,
            _ => self.update(ctx, false),
        }
    }

    /// `set_enabled` {on}: the rule's switch (off: the open resource forgotten).
    fn call(&self, method: &str, args: Value, _ctx: &Ctx) -> Result<Value> {
        match method {
            "set_enabled" => {
                let on = args["on"].as_bool().unwrap_or(false);
                self.enabled.store(on, Ordering::Relaxed);
                if !on {
                    *self.open.lock() = None;
                }
                Ok(json!({"enabled": on}))
            }
            _ => Err(unknown(ID, method)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::testkit;
    use loutil_core::RawMessage;
    use std::sync::Arc;

    /// A recorded sale (work/capture_hdv_sell.jsonl: 876 « Bourgeon de Kaliptus »... sold as 8 lots of 100): picked with
    /// 876 owned = the 100 lot at 169,980 -> 169,979; the listing then shows the player's 169,979 (not undercut again) and
    /// at 76 left the 10 lot's 16,991 -> 16,990. Skipped without the file.
    #[test]
    fn follows_a_recorded_sale() {
        let Some(pack) = testkit::pack() else { return };
        let rec = pack.join("../work/capture_hdv_sell.jsonl");
        if !rec.exists() {
            return;
        }
        let (hub, proto) = testkit::hub("loutil-sellprice").unwrap();
        let copies: Arc<Mutex<Vec<Value>>> = Default::default();
        let sink = copies.clone();
        let emit = Arc::new(move |e: &str, v: Value| {
            if e == "sellprice:copied" {
                sink.lock().push(v);
            }
        });
        let hub = crate::Hub::new(hub.ctx.data.clone(), proto, testkit::temp_dir("loutil-sellprice2"), emit, Arc::new(|_| {}));
        hub.call(ID, "set_enabled", json!({"on": true})).unwrap();
        let hex = |s: &str| (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect::<Vec<u8>>();
        for line in std::fs::read_to_string(&rec).unwrap().lines() {
            let r: Value = serde_json::from_str(line).unwrap();
            let (Some(h), Some(dir)) = (r["hex"].as_str(), r["dir"].as_str()) else { continue };
            let dir = if dir == "C->S" { "C->S" } else { "S->C" };
            hub.handle(RawMessage { t: r["t"].as_f64().unwrap_or(0.0), srv: "x".into(), dir, payload: hex(h) });
        }
        let got: Vec<(i64, i64, i64)> =
            copies.lock().iter().map(|c| (c["quantity"].as_i64().unwrap(), c["size"].as_i64().unwrap(), c["price"].as_i64().unwrap())).collect();
        assert_eq!(got, vec![(876, 100, 169979), (76, 10, 16990)]);
    }

    #[test]
    fn picks_the_size_the_game_selects() {
        assert_eq!(size_for(0), None);
        assert_eq!(size_for(9), Some((0, 1)));
        assert_eq!(size_for(346), Some((2, 100)));
        assert_eq!(size_for(100), Some((2, 100)));
        assert_eq!(size_for(99), Some((1, 10)));
        assert_eq!(size_for(5000), Some((3, 1000)));
    }

    #[test]
    fn undercuts_others_but_not_the_players_own_lot() {
        assert_eq!(price_for(169980, None), Some(169979));
        assert_eq!(price_for(169979, Some(169979)), Some(169979));
        assert_eq!(price_for(169970, Some(169979)), Some(169969));
        assert_eq!(price_for(0, None), None);
        assert_eq!(price_for(1, None), Some(1));
    }
}
