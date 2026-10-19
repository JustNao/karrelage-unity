//! Stored prices: the cheapest per-unit price seen for every item opened in the auction house, grouped by item
//! category / type, with a manual override that wins over the captured price.
use crate::auction::AH_LIST;
use crate::{Ctx, Interest, Message, Module, Persist};
use anyhow::{anyhow, Result};
use loutil_data::items::RUNE_MARKER_EFFECTS;
use loutil_proto::util::{int, messages};
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock};

/// `itemtypes.categoryId` -> label shown in the UI.
fn category_name(id: Option<i64>) -> &'static str {
    match id {
        Some(0) => "Équipement",
        Some(1) => "Consommable",
        Some(2) => "Ressource",
        _ => "Autre",
    }
}

/// Clear name of the message carrying the average prices of the last week.
pub const AVG_PRICES: &str = "ObjectAveragePricesEvent";

/// itemtypes id of "Rune de transcendance".
const RUNE_TYPE: i64 = 211;

/// A price read in the auction house is forgotten after this long (seconds); the weekly average and manual overrides stay.
const UNIT_MAX_AGE: f64 = 2.0 * 86400.0;

fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// Drop the AH-read price of every entry older than `UNIT_MAX_AGE`, then the entries left with no price at all.
fn expire(s: &mut BTreeMap<i64, Value>, now: f64) -> bool {
    let mut changed = false;
    for e in s.values_mut() {
        if !e["unit"].is_null() && e["updated"].as_f64().is_some_and(|t| now - t > UNIT_MAX_AGE) {
            e["unit"] = Value::Null;
            e["updated"] = Value::Null;
            changed = true;
        }
    }
    let before = s.len();
    s.retain(|_, e| !e["unit"].is_null() || !e["override"].is_null() || !e["avg"].is_null());
    changed || s.len() != before
}

pub struct Prices {
    /// gid -> {gid, name, type_id, type, category, unit, updated, override}
    store: Arc<Mutex<BTreeMap<i64, Value>>>,
    persist: OnceLock<Persist>,
    /// pack-derived answers, constant once the pack is loaded
    runes: OnceLock<Value>,
    craft_runes: OnceLock<Value>,
    by_name: OnceLock<HashMap<String, i64>>,
}

/// Case / nbsp / apostrophe / accent insensitive item name, for `gids_by_name`.
fn norm_name(s: &str) -> String {
    s.replace('\u{a0}', " ")
        .replace('\u{2019}', "'")
        .trim()
        .to_lowercase()
        .chars()
        .flat_map(|c| -> Vec<char> {
            match c {
                'é' | 'è' | 'ê' | 'ë' => vec!['e'],
                'à' | 'â' | 'ä' => vec!['a'],
                'î' | 'ï' => vec!['i'],
                'ô' | 'ö' => vec!['o'],
                'ù' | 'û' | 'ü' => vec!['u'],
                'ç' => vec!['c'],
                'œ' => vec!['o', 'e'],
                c => vec![c],
            }
        })
        .collect()
}

impl Default for Prices {
    fn default() -> Self {
        Self::new()
    }
}

impl Prices {
    pub fn new() -> Self {
        Self { store: Default::default(), persist: OnceLock::new(), runes: OnceLock::new(), craft_runes: OnceLock::new(), by_name: OnceLock::new() }
    }

    fn path(ctx: &Ctx) -> std::path::PathBuf {
        ctx.module_dir("prices").join("prices.json")
    }

    /// Written in the background (see `Persist`).
    fn save(&self) {
        if let Some(p) = self.persist.get() {
            p.touch();
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

impl Prices {
    /// Average per-unit prices of the last week: preloads the store. They only fill `avg`; prices read in the auction
    /// house (`unit`) and manual ones (`override`) always win over it.
    fn on_average_prices(&self, m: &Message, ctx: &Ctx) {
        let (Some(msg), Some(b)) = (m.decoded.as_ref(), ctx.data.bindings.get("avg_prices")) else { return };
        let field = |k: &str| b[k].as_str().unwrap_or_default();
        let mut changed = vec![];
        let mut s = self.store.lock();
        for e in messages(msg, field("items")) {
            let (gid, price) = (int(&e, field("gid")), int(&e, field("price")));
            if price <= 0 || !ctx.data.items.contains_key(&gid) {
                continue;
            }
            let ent = s.entry(gid).or_insert_with(|| json!({"gid": gid, "override": null, "unit": null}));
            ent["avg"] = json!(price);
            ent["avg_updated"] = json!(m.raw.t);
            Self::describe(ctx, gid, ent);
            changed.push(ent.clone());
        }
        drop(s);
        if changed.is_empty() {
            return;
        }
        self.save();
        ctx.emit("prices", "bulk", Value::Array(changed));
    }
}

impl Module for Prices {
    fn id(&self) -> &'static str {
        "prices"
    }

    fn interest(&self) -> Interest {
        Interest::Types(&[AH_LIST, AVG_PRICES])
    }

    fn init(&self, ctx: &Ctx) {
        let store = self.store.clone();
        let (persist, saved) =
            Persist::open(Self::path(ctx), ctx.logger(), move || Value::Object(store.lock().iter().map(|(k, v)| (k.to_string(), v.clone())).collect()));
        let _ = self.persist.set(persist);
        if let Some(Value::Object(saved)) = saved {
            let mut s = self.store.lock();
            for (k, mut v) in saved {
                if let Ok(gid) = k.parse::<i64>() {
                    Self::describe(ctx, gid, &mut v);
                    s.insert(gid, v);
                }
            }
            if expire(&mut s, now()) {
                drop(s);
                self.save();
            }
        }
    }

    fn on_message(&self, m: &Message, ctx: &Ctx) {
        if m.name == AVG_PRICES {
            return self.on_average_prices(m, ctx);
        }
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
        let mut s = self.store.lock();
        let ent = s.entry(gid).or_insert_with(|| json!({"gid": gid, "override": null}));
        ent["unit"] = json!(unit.round() as i64);
        ent["updated"] = json!(m.raw.t);
        Self::describe(ctx, gid, ent);
        let ent = ent.clone();
        drop(s);
        self.save();
        ctx.emit("prices", "price", ent);
    }

    fn call(&self, method: &str, args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            "list" => {
                let mut s = self.store.lock();
                let changed = expire(&mut s, now());
                let out = Value::Array(s.values().cloned().collect());
                drop(s);
                if changed {
                    self.save();
                }
                Ok(out)
            }
            // Rune de transcendance items: [{gid, name, a, value}] (the effect a rune adds to an item)
            "runes" => Ok(self
                .runes
                .get_or_init(|| {
                    let mut v: Vec<Value> = ctx
                        .data
                        .items
                        .iter()
                        .filter(|(_, i)| i.type_id == RUNE_TYPE)
                        .filter_map(|(g, i)| {
                            let e = i.effects.iter().find(|e| !RUNE_MARKER_EFFECTS.contains(&e.a))?;
                            Some(json!({"gid": g, "name": ctx.data.item_name(*g), "a": e.a, "value": e.max}))
                        })
                        .collect();
                    v.sort_by_key(|x| x["gid"].as_i64());
                    Value::Array(v)
                })
                .clone()),
            // Every craftable rune (forgemagie type 78 and transcendance type 211): [{gid, name, effect}]
            "craft_runes" => Ok(self
                .craft_runes
                .get_or_init(|| {
                    let mut v: Vec<Value> = ctx
                        .data
                        .items
                        .iter()
                        .filter(|(g, i)| (i.type_id == 78 || i.type_id == RUNE_TYPE) && ctx.data.recipes.contains_key(g))
                        .map(|(g, i)| {
                            // the effect the rune grants, rendered like in game ("+10 Force")
                            let effect = i.effects.iter().find(|e| !RUNE_MARKER_EFFECTS.contains(&e.a)).map(|e| ctx.data.render_effect(e.a, &[json!(e.max)]).0);
                            json!({"gid": g, "name": ctx.data.item_name(*g), "effect": effect})
                        })
                        .collect();
                    v.sort_by_key(|x| x["gid"].as_i64());
                    Value::Array(v)
                })
                .clone()),
            // args: {names: [string]} -> {name: gid} (case / nbsp / apostrophe insensitive; lowest gid wins on duplicates)
            "gids_by_name" => {
                let by = self.by_name.get_or_init(|| {
                    let mut by: HashMap<String, i64> = HashMap::new();
                    for g in ctx.data.items.keys() {
                        by.entry(norm_name(&ctx.data.item_name(*g))).and_modify(|x| *x = (*x).min(*g)).or_insert(*g);
                    }
                    by
                });
                let mut out = serde_json::Map::new();
                for n in args["names"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                    if let Some(g) = by.get(&norm_name(n)) {
                        out.insert(n.to_string(), json!(g));
                    }
                }
                Ok(Value::Object(out))
            }
            // args: {gid, value: number | null}; null removes the override
            "set_override" => {
                let gid = args["gid"].as_i64().ok_or_else(|| anyhow!("prices: gid required"))?;
                let value = args["value"].as_f64().filter(|v| *v >= 0.0).map(|v| v.round() as i64);
                let mut s = self.store.lock();
                let ent = s.get_mut(&gid).ok_or_else(|| anyhow!("prices: unknown item {gid}"))?;
                ent["override"] = json!(value);
                let ent = ent.clone();
                drop(s);
                self.save();
                ctx.emit("prices", "price", ent.clone());
                Ok(ent)
            }
            "forget" => {
                self.store.lock().clear();
                self.save();
                ctx.emit("prices", "cleared", json!({}));
                Ok(json!({"ok": true}))
            }
            _ => Err(anyhow!("prices: unknown method {method}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loutil_core::RawMessage;
    use loutil_data::Data;
    use loutil_proto::Proto;
    use std::sync::Arc;

    #[test]
    fn average_prices_preload_but_never_override() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let pack = root.join("datapack");
        if !pack.join("manifest.json").exists() {
            return;
        }
        let dir = std::env::temp_dir().join("loutil-avgprices");
        let _ = std::fs::remove_dir_all(&dir);
        let data = Arc::new(Data::load(&pack, "fr").unwrap());
        let proto = Arc::new(Proto::load(&pack, dir.join("o.json")).unwrap());
        let hub = crate::Hub::new(data, proto.clone(), dir, Arc::new(|_, _| {}), Arc::new(|_| {}));
        // two entries: gid 1711 -> 5000, gid 1728 -> 300 (third field unused)
        let payload = vec![0x1a, 0x06, 0x08, 0xaf, 0x0d, 0x10, 0x88, 0x27, 0x1a, 0x08, 0x08, 0xc0, 0x0d, 0x10, 0xac, 0x02, 0x18, 0x01];
        let decoded = proto.decode("isr", &payload);
        let msg = |t| Message {
            id: 1,
            raw: RawMessage { t, srv: "x".into(), dir: "S->C", payload: vec![] },
            env: None,
            type_name: Some("isr".into()),
            name: AVG_PRICES.into(),
            conf: None,
            decoded: decoded.clone(),
            fmt: None,
        };
        let p = Prices::new();
        p.on_message(&msg(1.0), &hub.ctx);
        let list = p.call("list", Value::Null, &hub.ctx).unwrap();
        let get = |l: &Value, gid: i64| l.as_array().unwrap().iter().find(|e| e["gid"] == gid).cloned().unwrap();
        assert_eq!(get(&list, 1711)["avg"], 5000);
        assert_eq!(get(&list, 1728)["avg"], 300);
        assert!(get(&list, 1711)["unit"].is_null());
        // a manual override survives a later average
        p.call("set_override", json!({"gid": 1711, "value": 42}), &hub.ctx).unwrap();
        p.on_message(&msg(2.0), &hub.ctx);
        let list = p.call("list", Value::Null, &hub.ctx).unwrap();
        assert_eq!(get(&list, 1711)["override"], 42);
    }

    #[test]
    fn gids_by_name_finds_foods() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let pack = root.join("datapack");
        if !pack.join("manifest.json").exists() {
            return;
        }
        let dir = std::env::temp_dir().join("loutil-gids");
        let _ = std::fs::remove_dir_all(&dir);
        let data = Arc::new(Data::load(&pack, "fr").unwrap());
        let proto = Arc::new(Proto::load(&pack, dir.join("o.json")).unwrap());
        let hub = crate::Hub::new(data, proto, dir, Arc::new(|_, _| {}), Arc::new(|_| {}));
        let r = Prices::new().call("gids_by_name", json!({"names": ["Croquette enrichie", "Queue d'Ecumouth", "Eau calme"]}), &hub.ctx).unwrap();
        println!("{r}");
        assert_eq!(r["Croquette enrichie"], 20125);
        assert!(r.as_object().unwrap().len() >= 2);
    }
}
