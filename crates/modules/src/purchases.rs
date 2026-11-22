//! Auction house purchases as (gid, quantity) bought, for the floating « À acheter » list (Crafting « XP métiers ») to tick
//! off what is bought. The purchase messages name neither: ExchangeBidHouseBuyRequest = listing uid + lot size + price
//! (unnamed), ExchangeBidHouseBuyResultEvent = listing uid + `bought`. The inventory tells instead: between a request and its
//! result the bought lot lands in it (ObjectQuantityEvent on the stack, or ObjectAddedEvent for a new one: read by the
//! inventory module first), so a successful result sends every gid whose quantity grew since the request.
use crate::inventory::Stacks;
use crate::kit::{unknown, Role};
use crate::{Ctx, Interest, Message, Module};
use anyhow::Result;
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

const ID: &str = "purchases";
const BUY_REQUEST: &str = "ExchangeBidHouseBuyRequest";
const BUY_RESULT: &str = crate::auction::AH_BUY;

/// bindings.json `ah_buy` (only `bought` read here).
#[derive(Deserialize, Clone)]
struct BuyBindings {
    bought: String,
}

/// gid -> quantity held, every stack summed.
fn totals(stacks: &Stacks) -> HashMap<i64, i64> {
    let mut out = HashMap::new();
    for (gid, q) in stacks.lock().values() {
        *out.entry(*gid).or_insert(0) += q;
    }
    out
}

#[derive(Default)]
struct Pending {
    /// requests not answered yet (several can be sent before the first result)
    count: usize,
    /// the inventory when the first of them was sent, or at the last result
    before: HashMap<i64, i64>,
}

#[derive(Default)]
pub struct Purchases {
    bind: Role<BuyBindings>,
    stacks: Stacks,
    pending: Mutex<Pending>,
}

impl Purchases {
    /// `stacks`: every object's stack size, kept by the inventory module (which sees the messages first).
    pub fn new(stacks: Stacks) -> Self {
        Self { stacks, ..Default::default() }
    }
}

impl Module for Purchases {
    fn id(&self) -> &'static str {
        ID
    }

    fn interest(&self) -> Interest {
        Interest::Types(&[BUY_REQUEST, BUY_RESULT])
    }

    fn on_message(&self, m: &Message, ctx: &Ctx) {
        let mut p = self.pending.lock();
        if m.name == BUY_REQUEST {
            if p.count == 0 {
                p.before = totals(&self.stacks);
            }
            p.count += 1;
            return;
        }
        if p.count == 0 {
            return; // a result whose request was not seen (app started in between)
        }
        p.count -= 1;
        let now = totals(&self.stacks);
        let bought = match (self.bind.get(ctx, "ah_buy"), &m.decoded) {
            (Some(b), Some(d)) => d.get_field_by_name(&b.bought).and_then(|v| v.as_bool()).unwrap_or(false),
            _ => false,
        };
        if bought {
            let mut got: Vec<(i64, i64)> =
                now.iter().filter_map(|(g, q)| Some((*g, q - p.before.get(g).copied().unwrap_or(0))).filter(|(_, d)| *d > 0)).collect();
            got.sort_unstable();
            for (gid, quantity) in got {
                ctx.emit(ID, "bought", json!({"gid": gid, "quantity": quantity, "t": m.raw.t}));
            }
        }
        p.before = now;
    }

    fn call(&self, method: &str, _args: Value, _ctx: &Ctx) -> Result<Value> {
        Err(unknown(ID, method))
    }
}

#[cfg(test)]
mod tests {
    use crate::kit::testkit;
    use loutil_core::RawMessage;
    use parking_lot::Mutex;
    use serde_json::Value;
    use std::sync::Arc;

    /// work/capture_hdv_buy.jsonl: 10 purchases of one resource (gid 20819) one by one, each answered bought: ten times
    /// (20819, 1). Skipped without the file.
    #[test]
    fn reads_recorded_purchases() {
        let Some(pack) = testkit::pack() else { return };
        let rec = pack.join("../work/capture_hdv_buy.jsonl");
        if !rec.exists() {
            return;
        }
        let (hub, proto) = testkit::hub("loutil-purchases").unwrap();
        let got: Arc<Mutex<Vec<(i64, i64)>>> = Default::default();
        let sink = got.clone();
        let emit = Arc::new(move |e: &str, v: Value| {
            if e == "purchases:bought" {
                sink.lock().push((v["gid"].as_i64().unwrap(), v["quantity"].as_i64().unwrap()));
            }
        });
        let hub = crate::Hub::new(hub.ctx.data.clone(), proto, testkit::temp_dir("loutil-purchases2"), emit, Arc::new(|_| {}));
        let hex = |s: &str| (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect::<Vec<u8>>();
        for line in std::fs::read_to_string(&rec).unwrap().lines() {
            let r: Value = serde_json::from_str(line).unwrap();
            let (Some(h), Some(dir)) = (r["hex"].as_str(), r["dir"].as_str()) else { continue };
            let dir = if dir == "C->S" { "C->S" } else { "S->C" };
            hub.handle(RawMessage { t: r["t"].as_f64().unwrap_or(0.0), srv: "x".into(), dir, payload: hex(h) });
        }
        assert_eq!(*got.lock(), vec![(20819, 1); 10]);
    }
}
