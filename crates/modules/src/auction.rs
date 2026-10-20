//! Auction house: renders the AH item-list message (prices per lot size, effects vs the item's defaults)
//! and keeps the latest listing per item. Port of formatters.format_ah_list + webapp.update_store.
use crate::{Ctx, Interest, Message, Module, Persist};
use anyhow::{anyhow, Result};
use loutil_data::items::{info_key, Effect, ELEMENT_CHANGE_EFFECT, LOCK_EFFECT, LOT_SIZES};
use loutil_data::Data;
use loutil_proto::util::{as_i64, int, int_list, messages, oneof_value};
use loutil_proto::{DynamicMessage, Kind, PbValue, ReflectMessage};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

/// Clear name pinned in pipeline overrides (work/overrides.json) for the AH listing event.
pub const AH_LIST: &str = "ExchangeTypesItemsExchangerDescriptionForUserEvent";
/// Answer to an AH purchase: the lot bought leaves the stored listing.
pub const AH_BUY: &str = "ExchangeBidHouseBuyResultEvent";
/// A row of the open AH listing changed (new prices after a purchase or a new offer) or left it.
pub const AH_ROW_UPDATED: &str = "ExchangeBidHouseInListUpdatedEvent";
pub const AH_ROW_REMOVED: &str = "ExchangeBidHouseInListRemovedEvent";

// build-specific obfuscated field names, read from the data pack's bindings.json
#[derive(Deserialize, Clone)]
struct Bindings {
    ah_list: AhList,
    ah_item: AhItem,
    object_effect: ObjectEffect,
}
#[derive(Deserialize, Clone)]
struct AhList {
    items: String,
    ints: Vec<String>,
}
#[derive(Deserialize, Clone)]
struct AhItem {
    prices: String,
    effects: String,
    ints: Vec<String>,
}
#[derive(Deserialize, Clone)]
pub(crate) struct ObjectEffect {
    action: Vec<String>,
    oneof: String,
}
/// bindings.json `ah_buy` / `ah_row` / `ah_row_removed` (each optional, so that an older pack still loads the listing bindings).
#[derive(Deserialize, Clone)]
struct AhBuy {
    uid: String,
    bought: String,
}
#[derive(Deserialize, Clone)]
struct AhRow {
    prices: String,
    effects: String,
    ints: Vec<String>,
}
#[derive(Deserialize, Clone)]
struct AhRowRemoved {
    ints: Vec<String>,
}

pub struct Auction {
    bind: OnceLock<Option<Bindings>>,
    buy_bind: OnceLock<Option<AhBuy>>,
    row_bind: OnceLock<Option<AhRow>>,
    removed_bind: OnceLock<Option<AhRowRemoved>>,
    store: Arc<Mutex<BTreeMap<String, Value>>>,
    persist: OnceLock<Persist>,
    /// pack-derived answers, constant once the pack is loaded
    effects: OnceLock<Value>,
    recipes: OnceLock<Value>,
    effect_meta: OnceLock<Value>,
}

impl Default for Auction {
    fn default() -> Self {
        Self::new()
    }
}

impl Auction {
    pub fn new() -> Self {
        Self {
            bind: OnceLock::new(),
            buy_bind: OnceLock::new(),
            row_bind: OnceLock::new(),
            removed_bind: OnceLock::new(),
            store: Default::default(),
            persist: OnceLock::new(),
            effects: OnceLock::new(),
            recipes: OnceLock::new(),
            effect_meta: OnceLock::new(),
        }
    }

    fn bindings(&self, ctx: &Ctx) -> Option<&Bindings> {
        self.bind.get_or_init(|| serde_json::from_value(ctx.data.bindings.clone()).ok()).as_ref()
    }

    fn role<'a, T: serde::de::DeserializeOwned>(cell: &'a OnceLock<Option<T>>, ctx: &Ctx, role: &str) -> Option<&'a T> {
        cell.get_or_init(|| serde_json::from_value(ctx.data.bindings.get(role)?.clone()).ok()).as_ref()
    }

    fn store_path(ctx: &Ctx) -> std::path::PathBuf {
        ctx.module_dir("auction").join("items.json")
    }

    /// Written in the background (see `Persist`).
    fn save(&self) {
        if let Some(p) = self.persist.get() {
            p.touch();
        }
    }

    /// Keep the latest listing per item; a new packet for the same item replaces the old one.
    fn update_store(&self, m: &Message, ctx: &Ctx) {
        let Some(f) = &m.fmt else { return };
        if f["kind"] != "auction_items" {
            return;
        }
        let Some(rows) = f["rows"].as_array().filter(|r| !r.is_empty()) else { return };
        let Some(gid) = f["gid"].as_i64().or_else(|| rows[0]["gid"].as_i64()) else { return };
        if !ctx.data.is_equipment(gid) {
            return; // resources / consumables only feed the stored prices
        }
        let mins: Vec<i64> = rows.iter().filter_map(|r| r["prices"].get(0).and_then(|p| p["price"].as_i64())).filter(|p| *p > 0).collect();
        let ent = json!({
            "gid": gid,
            "name": if f["title"].is_null() { rows[0]["name"].clone() } else { f["title"].clone() },
            "type": if f["type"].is_null() { rows[0]["type"].clone() } else { f["type"].clone() },
            "companion": ctx.data.is_companion(Some(gid)), "feedable": ctx.data.is_feedable(Some(gid)), "level": rows[0]["level"], "icon": rows[0]["icon"], "updated": m.raw.t, "lots": rows.len(),
            "min_price": mins.iter().min(), "rows": rows, "defaults": f["defaults"], "demo": m.raw.srv == "demo",
        });
        self.store.lock().insert(gid.to_string(), ent.clone());
        self.save();
        ctx.emit("auction", "item", ent);
    }

    /// A change of the open listing (`apply_change`): applied to the stored listing holding that lot, sent again with
    /// its scan time kept.
    fn on_change(&self, m: &Message, ctx: &Ctx) {
        let Some(f) = &m.fmt else { return };
        let mut s = self.store.lock();
        let Some(ent) = s.values_mut().find(|e| e["rows"].as_array().is_some_and(|r| r.iter().any(|x| !x["uid"].is_null() && x["uid"] == f["uid"]))) else { return };
        let rows = ent["rows"].as_array_mut().unwrap();
        if apply_change(rows, f).is_none() {
            return;
        }
        let min = rows.iter().filter_map(|r| r["prices"].get(0).and_then(|p| p["price"].as_i64())).filter(|p| *p > 0).min();
        ent["lots"] = json!(rows.len());
        ent["min_price"] = json!(min);
        let ent = ent.clone();
        drop(s);
        self.save();
        ctx.emit("auction", "item", ent);
    }
}

fn raw_json(v: &PbValue) -> Value {
    match v {
        PbValue::String(s) => json!(s),
        PbValue::Bool(b) => json!(b),
        PbValue::F32(x) => json!(x),
        PbValue::F64(x) => json!(x),
        PbValue::Bytes(b) => json!(format!("0x{}", loutil_core::wire::hex(b))),
        other => as_i64(other).map_or(Value::Null, |n| json!(n)),
    }
}

/// The effects that are stats of the item (info effects, the lock and the element-change marker left out).
pub(crate) fn stat_effects(effs: Vec<Effect>) -> Vec<Effect> {
    effs.into_iter().filter(|e| info_key(e.action).is_none() && e.action != LOCK_EFFECT && e.action != ELEMENT_CHANGE_EFFECT).collect()
}

/// ObjectEffect message -> annotated-later Effect.
pub(crate) fn parse_effect(data: &Data, b: &ObjectEffect, e: &DynamicMessage) -> Effect {
    let acts: Vec<i64> = b.action.iter().map(|n| int(e, n)).collect();
    let action = acts.iter().find(|a| data.effects.contains_key(a)).or_else(|| acts.iter().find(|a| **a != 0)).or(acts.first()).copied().unwrap_or(0);
    let (mut params, mut raw): (Vec<Value>, Value) = (vec![], Value::Null);
    if let Some((_, v, _)) = oneof_value(e, &b.oneof) {
        match &v {
            PbValue::Message(sub) => {
                let mut obj = Map::new();
                let mut vals = vec![];
                for fd in sub.descriptor().fields() {
                    let val = sub.get_field(&fd);
                    match fd.kind() {
                        Kind::Message(_) => {}
                        k => {
                            obj.insert(fd.name().to_string(), raw_json(&val));
                            if matches!(k, Kind::Int64 | Kind::Int32 | Kind::Uint32 | Kind::Uint64) {
                                vals.push(as_i64(&val).unwrap_or(0));
                            }
                        }
                    }
                }
                raw = Value::Object(obj);
                params = match vals.as_slice() {
                    [a, b] => vec![(*a).into(), (*b).into()],
                    [n, s, c] if *n != 0 && *s != 0 => vec![(c + n).into(), (c + n * s).into()], // dice num, side, const
                    [_, _, c] => vec![(*c).into()],
                    _ => vec![],
                };
            }
            PbValue::String(s) => {
                params = vec![json!(s)];
                raw = json!(s);
            }
            other => {
                raw = raw_json(other);
                params = vec![raw.clone()];
            }
        }
    }
    let (text, row) = data.render_effect(action, &params);
    let hi = params.iter().filter_map(Value::as_i64).max();
    Effect {
        action,
        text,
        known: row.is_some(),
        hi,
        str: raw.as_str().map(str::to_owned),
        raw: if raw.is_array() { Value::Null } else { raw },
        params,
        group: String::new(),
        order: 0,
        density: 0.0,
        stat: 0,
        status: None,
        range: None,
    }
}

/// One AH row (a lot: prices per stack size, effects) from its ints / prices / effects fields; `fallback` = the
/// listing's (gid, type) when the row's ints do not resolve.
fn format_row(data: &Data, oe: &ObjectEffect, it: &DynamicMessage, ints: Vec<i64>, prices_f: &str, effects_f: &str, fallback: (Option<i64>, Option<i64>)) -> Value {
    let (mut gid, mut typ, mut uid) = data.resolve_gid_type(&ints);
    if gid.is_none() {
        (gid, typ, uid) = (fallback.0, fallback.1, None);
    }
    let prices = int_list(it, prices_f);
    let info = gid.and_then(|g| data.items.get(&g));
    let effs: Vec<Effect> = messages(it, effects_f).iter().map(|e| parse_effect(data, oe, e)).collect();
    let meta: Map<String, Value> =
        effs.iter().filter_map(|e| info_key(e.action).map(|k| (k.to_string(), e.str.clone().map_or(Value::Null, Value::from)))).collect();
    let locked = effs.iter().any(|e| e.action == LOCK_EFFECT);
    let stats = stat_effects(effs);
    let density = (data.item_density(&stats) * 1000.0).round() / 1000.0;
    json!({
        "uid": uid, "gid": gid, "ints": ints,
        "name": gid.map_or_else(|| "item#None".to_string(), |g| data.item_name(g)),
        "level": info.map(|i| i.level), "type": data.type_name(typ.or(info.map(|i| i.type_id))),
        "icon": info.map(|i| i.icon_id),
        "prices": prices.iter().enumerate().map(|(i, p)| json!({"qty": LOT_SIZES.get(i).copied().unwrap_or(i as i64), "price": p})).collect::<Vec<_>>(),
        "meta": meta, "locked": locked, "density": density,
        "effects": data.compare_effects(stats, gid),
    })
}

/// Apply a listing change (fmt `auction_bought` / `auction_row` / `auction_row_removed`) to the rows of a listing.
/// None when it changes nothing (no row with that uid...). A purchase only removes a row holding a single offer (an
/// equipment lot): a resource row lists the cheapest stack of every size, the server's `auction_row` updates it.
pub(crate) fn apply_change(rows: &mut Vec<Value>, f: &Value) -> Option<()> {
    let uid = f["uid"].as_i64()?;
    let i = rows.iter().position(|r| r["uid"].as_i64() == Some(uid))?;
    match f["kind"].as_str()? {
        "auction_row" => rows[i] = f["row"].clone(),
        "auction_row_removed" => {
            rows.remove(i);
        }
        "auction_bought" => {
            let offers = rows[i]["prices"].as_array().map_or(0, |p| p.iter().filter(|p| p["price"].as_i64().unwrap_or(0) > 0).count());
            if f["bought"] != true || offers > 1 {
                return None;
            }
            rows.remove(i);
        }
        _ => return None,
    }
    Some(())
}

fn format_ah_list(data: &Data, b: &Bindings, msg: &DynamicMessage) -> Value {
    let names = |v: &[String]| -> Vec<i64> { v.iter().map(|n| int(msg, n)).collect() };
    let (gid0, type0, _) = data.resolve_gid_type(&names(&b.ah_list.ints));
    let mut rows = vec![];
    for it in messages(msg, &b.ah_list.items) {
        let ints: Vec<i64> = b.ah_item.ints.iter().map(|n| int(&it, n)).collect();
        rows.push(format_row(data, &b.object_effect, &it, ints, &b.ah_item.prices, &b.ah_item.effects, (gid0, type0)));
    }
    json!({
        "kind": "auction_items", "defaults": data.defaults_for(gid0),
        "title": gid0.map(|g| data.item_name(g)), "type": type0.map(|t| data.type_name(Some(t))),
        "gid": gid0, "rows": rows,
    })
}

impl Module for Auction {
    fn id(&self) -> &'static str {
        "auction"
    }

    fn interest(&self) -> Interest {
        Interest::Types(&[AH_LIST, AH_BUY, AH_ROW_UPDATED, AH_ROW_REMOVED])
    }

    fn init(&self, ctx: &Ctx) {
        let store = self.store.clone();
        let (persist, saved) = Persist::open(Self::store_path(ctx), ctx.logger(), move || {
            let s = store.lock();
            Value::Object(s.iter().filter(|(_, v)| v["demo"] != json!(true)).map(|(k, v)| (k.clone(), v.clone())).collect())
        });
        let _ = self.persist.set(persist);
        if let Some(Value::Object(saved)) = saved {
            let mut s = self.store.lock();
            for (k, mut v) in saved {
                if !v["gid"].as_i64().is_some_and(|g| ctx.data.is_equipment(g)) {
                    continue; // stored before the HDV tab was limited to equipment
                }
                v["companion"] = json!(ctx.data.is_companion(v["gid"].as_i64()));
                v["feedable"] = json!(ctx.data.is_feedable(v["gid"].as_i64()));
                if v.get("defaults").is_none() {
                    v["defaults"] = Value::Array(ctx.data.defaults_for(v["gid"].as_i64()));
                }
                // Stored lots may predate (or disagree with) the current data pack: re-tag their effects with the pack's
                // group/order/density, and recompute the lot density from them (the formula may have changed).
                if let Some(rows) = v.get_mut("rows").and_then(Value::as_array_mut) {
                    for e in rows.iter_mut().flat_map(|r| r["effects"].as_array_mut()).flatten() {
                        // the stored text was rendered with the pack of the time (e.g. a Dofus passive shown as a spell id)
                        if let (Some(action), Some(params)) = (e["action"].as_i64(), e["params"].as_array()) {
                            e["text"] = json!(ctx.data.render_effect(action, params).0);
                        }
                        if let Some(row) = e["action"].as_i64().and_then(|a| ctx.data.effects.get(&a)) {
                            e["group"] = json!(row.group);
                            e["order"] = json!(row.order);
                            e["density"] = json!(row.density);
                            e["stat"] = json!(ctx.data.stat_of(e["action"].as_i64().unwrap_or(0)));
                        }
                    }
                    for r in rows.iter_mut() {
                        let sum: f64 = r["effects"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|e| e["status"] != "removed")
                            .filter_map(|e| ctx.data.effect_density(e["action"].as_i64()?, e["params"].as_array()?))
                            .sum();
                        r["density"] = json!((sum * 1000.0).round() / 1000.0);
                    }
                }
                s.insert(k, v);
            }
        }
    }

    fn format(&self, m: &Message, ctx: &Ctx) -> Option<Value> {
        let msg = m.decoded.as_ref()?;
        match m.name.as_str() {
            AH_LIST => Some(format_ah_list(&ctx.data, self.bindings(ctx)?, msg)),
            AH_BUY => {
                let b = Self::role(&self.buy_bind, ctx, "ah_buy")?;
                let bought = msg.get_field_by_name(&b.bought).and_then(|v| v.as_bool()).unwrap_or(false);
                Some(json!({"kind": "auction_bought", "uid": int(msg, &b.uid), "bought": bought}))
            }
            AH_ROW_UPDATED => {
                let (b, oe) = (Self::role(&self.row_bind, ctx, "ah_row")?, &self.bindings(ctx)?.object_effect);
                let ints: Vec<i64> = b.ints.iter().map(|n| int(msg, n)).collect();
                let row = format_row(&ctx.data, oe, msg, ints, &b.prices, &b.effects, (None, None));
                Some(json!({"kind": "auction_row", "uid": row["uid"], "gid": row["gid"], "row": row}))
            }
            AH_ROW_REMOVED => {
                let b = Self::role(&self.removed_bind, ctx, "ah_row_removed")?;
                let ints: Vec<i64> = b.ints.iter().map(|n| int(msg, n)).collect();
                let (gid, _, uid) = ctx.data.resolve_gid_type(&ints);
                Some(json!({"kind": "auction_row_removed", "uid": uid, "gid": gid, "ints": ints}))
            }
            _ => None,
        }
    }

    fn on_message(&self, m: &Message, ctx: &Ctx) {
        if m.name != AH_LIST {
            return self.on_change(m, ctx);
        }
        self.update_store(m, ctx);
    }

    fn call(&self, method: &str, _args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            "items" => Ok(Value::Array(self.store.lock().values().cloned().collect())),
            "effects" => Ok(self.effects.get_or_init(|| ctx.data.filterable_effects()).clone()),
            "recipes" => Ok(self.recipes.get_or_init(|| ctx.data.recipes_json()).clone()),
            "effect_meta" => Ok(self.effect_meta.get_or_init(|| ctx.data.effect_meta()).clone()),
            "clear" => {
                self.store.lock().clear();
                self.save();
                ctx.emit("auction", "cleared", json!({}));
                Ok(json!({"ok": true}))
            }
            _ => Err(anyhow!("auction: unknown method {method}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loutil_core::RawMessage;
    use loutil_proto::Proto;
    use std::sync::Arc;

    fn strip_keys(v: &mut Value, keys: &[&str]) {
        match v {
            Value::Object(m) => {
                m.retain(|k, _| !keys.contains(&k.as_str()));
                m.values_mut().for_each(|x| strip_keys(x, keys));
            }
            Value::Array(a) => a.iter_mut().for_each(|x| strip_keys(x, keys)),
            _ => {}
        }
    }

    fn strip_nulls(v: &mut Value) {
        match v {
            Value::Object(m) => {
                m.retain(|_, x| !x.is_null());
                m.values_mut().for_each(strip_nulls);
            }
            Value::Array(a) => a.iter_mut().for_each(strip_nulls),
            _ => {}
        }
    }

    #[test]
    fn listing_changes() {
        let lot = |uid: i64, prices: &[i64]| json!({"uid": uid, "prices": prices.iter().map(|p| json!({"price": p})).collect::<Vec<_>>()});
        let mut rows = vec![lot(1, &[100]), lot(2, &[5, 40, 300, 0])];
        // an equipment lot bought leaves the listing; a failed purchase or an unknown uid changes nothing
        assert!(apply_change(&mut rows, &json!({"kind": "auction_bought", "uid": 1, "bought": false})).is_none());
        assert!(apply_change(&mut rows, &json!({"kind": "auction_bought", "uid": 9, "bought": true})).is_none());
        assert!(apply_change(&mut rows, &json!({"kind": "auction_bought", "uid": 1, "bought": true})).is_some());
        assert_eq!(rows.len(), 1);
        // a resource row (several stack sizes) waits for the server's updated row
        assert!(apply_change(&mut rows, &json!({"kind": "auction_bought", "uid": 2, "bought": true})).is_none());
        apply_change(&mut rows, &json!({"kind": "auction_row", "uid": 2, "row": lot(2, &[6, 45, 0, 0])})).unwrap();
        assert_eq!(rows[0]["prices"][1]["price"], json!(45));
        apply_change(&mut rows, &json!({"kind": "auction_row_removed", "uid": 2})).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn backfills_density_of_stored_lots() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let pack = root.join("datapack");
        if !pack.join("manifest.json").exists() {
            return;
        }
        let dir = std::env::temp_dir().join("loutil-backfill");
        let _ = std::fs::remove_dir_all(&dir);
        let store = json!({"25222": {"gid": 25222, "updated": 0, "rows": [{"effects": [
            {"action": 125, "params": [5], "status": "normal"},
            {"action": 118, "params": [10], "status": "removed"}, // default effect missing on the lot: not counted
            {"action": 153, "params": [10], "status": "normal"},
            {"action": 176, "params": [30], "status": "normal"}]}], "density": 99.0}}); // prospection ignored; stale density replaced
        crate::save_json(&dir.join("auction/items.json"), &store).unwrap();
        let data = Arc::new(Data::load(&pack, "fr").unwrap());
        let proto = Arc::new(Proto::load(&pack, dir.join("o.json")).unwrap());
        let hub = crate::Hub::new(data, proto, dir, Arc::new(|_, _| {}), Arc::new(|_| {}));
        let items = hub.call("auction", "items", Value::Null).unwrap();
        assert_eq!(items[0]["rows"][0]["density"], json!(0.0)); // +5 vitalité (1.0) and -10 vitalité (-1.0)

        // effects stored without a group get the pack's (the UI's perfect-roll flags depend on it)
        assert_eq!(items[0]["rows"][0]["effects"][0]["group"], json!("stats"));
        assert_eq!(items[0]["rows"][0]["effects"][0]["density"], json!(0.2f32 as f64));
    }

    /// Golden test: the python formatter's output for the synthetic AH packet must equal ours.
    /// Needs the generated data pack (`python pipeline/export_pack.py`), otherwise it is skipped.
    #[test]
    fn matches_python_formatter() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let pack = root.join("datapack");
        if !pack.join("manifest.json").exists() {
            eprintln!("skipped: no datapack/");
            return;
        }
        let data = Arc::new(Data::load(&pack, "fr").unwrap());
        let tmp = std::env::temp_dir().join("loutil-golden-overrides.json");
        let proto = Arc::new(Proto::load(&pack, tmp).unwrap());
        let events = Arc::new(Mutex::new(vec![]));
        let ev = events.clone();
        let emit: Arc<dyn Fn(&str, Value) + Send + Sync> = Arc::new(move |n, v| ev.lock().push((n.to_string(), v)));
        let dir = std::env::temp_dir().join("loutil-golden");
        let _ = std::fs::remove_dir_all(&dir);
        let hub = crate::Hub::new(data, proto, dir, emit, Arc::new(|_| {}));
        hub.call("packets", "set_active", json!({"on": true})).unwrap(); // the inspector only records while its tab is open

        let payload = std::fs::read(root.join("fixtures/ah_demo.bin")).unwrap();
        hub.handle(RawMessage { t: 1.0, srv: "demo".into(), dir: "S->C", payload });

        let mut expected: Value = serde_json::from_slice(&std::fs::read(root.join("fixtures/ah_demo.expected.json")).unwrap()).unwrap();
        let events = events.lock();
        let list = hub.call("packets", "list", Value::Null).unwrap();
        let pkt = &list[0];
        assert_eq!(pkt["name"], expected["name"]);
        let mut got = pkt["fmt"].clone();
        strip_keys(&mut got, &["group", "order", "density", "stat"]); // added after the python reference was written
        let mut want = expected["fmt"].take();
        strip_nulls(&mut got);
        strip_nulls(&mut want);
        assert_eq!(got, want);
        assert!(events.iter().any(|(n, _)| n == "auction:item"));
    }
}
