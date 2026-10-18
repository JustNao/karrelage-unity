//! Stored prices: the cheapest per-unit price seen for every item opened in the auction house, grouped by item
//! category / type, with a manual override that wins over the captured price.
use crate::auction::AH_LIST;
use crate::{load_json, save_json, Ctx, Interest, Message, Module};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// `itemtypes.categoryId` -> label shown in the UI.
fn category_name(id: Option<i64>) -> &'static str {
    match id {
        Some(0) => "Équipement",
        Some(1) => "Consommable",
        Some(2) => "Ressource",
        _ => "Autre",
    }
}

/// itemtypes id of "Rune de transcendance".
const RUNE_TYPE: i64 = 211;

pub struct Prices {
    /// gid -> {gid, name, type_id, type, category, unit, updated, override}
    store: Mutex<BTreeMap<i64, Value>>,
}

impl Prices {
    pub fn new() -> Self {
        Self { store: Mutex::new(BTreeMap::new()) }
    }

    fn path(ctx: &Ctx) -> std::path::PathBuf {
        ctx.module_dir("prices").join("prices.json")
    }

    fn save(&self, ctx: &Ctx) {
        let s = self.store.lock().unwrap();
        let obj: serde_json::Map<String, Value> = s.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
        if let Err(e) = save_json(&Self::path(ctx), &Value::Object(obj)) {
            ctx.log(format!("prices: cannot save: {e}"));
        }
    }

    /// Fill the display fields from the data pack (names may change with the pack, so they are never trusted from disk).
    fn describe(ctx: &Ctx, gid: i64, e: &mut Value) {
        let type_id = ctx.data.items.get(&gid).map(|i| i.type_id);
        e["name"] = json!(ctx.data.item_name(gid));
        e["type_id"] = json!(type_id);
        e["type"] = json!(ctx.data.type_name(type_id));
        e["category"] = json!(category_name(type_id.and_then(|t| ctx.data.itemtypes.get(&t)).and_then(|t| t.category_id)));
    }
}

impl Module for Prices {
    fn id(&self) -> &'static str {
        "prices"
    }

    fn interest(&self) -> Interest {
        Interest::Types(&[AH_LIST])
    }

    fn init(&self, ctx: &Ctx) {
        if let Some(Value::Object(saved)) = load_json(&Self::path(ctx)) {
            let mut s = self.store.lock().unwrap();
            for (k, mut v) in saved {
                if let Ok(gid) = k.parse::<i64>() {
                    Self::describe(ctx, gid, &mut v);
                    s.insert(gid, v);
                }
            }
        }
    }

    fn on_message(&self, m: &Message, ctx: &Ctx) {
        let Some(f) = m.fmt.as_ref().filter(|f| f["kind"] == "auction_items") else { return };
        if m.raw.srv == "demo" {
            return;
        }
        let Some(rows) = f["rows"].as_array() else { return };
        let Some(gid) = f["gid"].as_i64().or_else(|| rows.first().and_then(|r| r["gid"].as_i64())) else { return };
        // Per-unit price of the highest stack size on sale (x1000 over x100 over x10 over x1): the mean price per unit
        // in that stack, taking the cheapest such stack.
        let offers: Vec<(f64, f64)> = rows
            .iter()
            .flat_map(|r| r["prices"].as_array().into_iter().flatten())
            .filter_map(|p| Some((p["qty"].as_f64().filter(|q| *q > 0.0)?, p["price"].as_f64().filter(|x| *x > 0.0)?)))
            .collect();
        let top = offers.iter().map(|(q, _)| *q).fold(0.0, f64::max);
        let unit = offers.iter().filter(|(q, _)| *q == top).map(|(q, p)| p / q).fold(f64::INFINITY, f64::min);
        if !unit.is_finite() {
            return; // nobody is selling it: keep the previous price
        }
        let mut s = self.store.lock().unwrap();
        let ent = s.entry(gid).or_insert_with(|| json!({"gid": gid, "override": null}));
        ent["unit"] = json!(unit.round() as i64);
        ent["updated"] = json!(m.raw.t);
        Self::describe(ctx, gid, ent);
        let ent = ent.clone();
        drop(s);
        self.save(ctx);
        ctx.emit("prices", "price", ent);
    }

    fn call(&self, method: &str, args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            "list" => Ok(Value::Array(self.store.lock().unwrap().values().cloned().collect())),
            // Rune de transcendance items: [{gid, name, a, value}] (the effect a rune adds to an item)
            "runes" => {
                let mut v: Vec<Value> = ctx
                    .data
                    .items
                    .iter()
                    .filter(|(_, i)| i.type_id == RUNE_TYPE)
                    .filter_map(|(g, i)| {
                        let e = i.effects.iter().find(|e| ![2825, 2826, 2827].contains(&e.a))?;
                        Some(json!({"gid": g, "name": ctx.data.item_name(*g), "a": e.a, "value": e.max}))
                    })
                    .collect();
                v.sort_by_key(|x| x["gid"].as_i64());
                Ok(Value::Array(v))
            }
            // args: {gid, value: number | null}; null removes the override
            "set_override" => {
                let gid = args["gid"].as_i64().ok_or_else(|| anyhow!("prices: gid required"))?;
                let value = args["value"].as_f64().filter(|v| *v >= 0.0).map(|v| v.round() as i64);
                let mut s = self.store.lock().unwrap();
                let ent = s.get_mut(&gid).ok_or_else(|| anyhow!("prices: unknown item {gid}"))?;
                ent["override"] = json!(value);
                let ent = ent.clone();
                drop(s);
                self.save(ctx);
                ctx.emit("prices", "price", ent.clone());
                Ok(ent)
            }
            "forget" => {
                self.store.lock().unwrap().clear();
                self.save(ctx);
                ctx.emit("prices", "cleared", json!({}));
                Ok(json!({"ok": true}))
            }
            _ => Err(anyhow!("prices: unknown method {method}")),
        }
    }
}
