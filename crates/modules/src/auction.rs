//! Auction house: renders the AH item-list message (prices per lot size, effects vs the item's defaults)
//! and keeps the latest listing per item. Port of formatters.format_ah_list + webapp.update_store.
use crate::{load_json, save_json, Ctx, Interest, Message, Module};
use anyhow::{anyhow, Result};
use loutil_data::items::{info_key, Effect, LOCK_EFFECT, LOT_SIZES};
use loutil_data::Data;
use loutil_proto::util::{as_i64, int, int_list, messages, oneof_value};
use loutil_proto::{DynamicMessage, Kind, PbValue, ReflectMessage};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

/// Clear name pinned in pipeline overrides (work/overrides.json) for the AH listing event.
pub const AH_LIST: &str = "ExchangeTypesItemsExchangerDescriptionForUserEvent";

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
struct ObjectEffect {
    action: Vec<String>,
    oneof: String,
}

pub struct Auction {
    bind: OnceLock<Option<Bindings>>,
    store: Mutex<BTreeMap<String, Value>>,
}

impl Auction {
    pub fn new() -> Self {
        Self { bind: OnceLock::new(), store: Mutex::new(BTreeMap::new()) }
    }

    fn bindings(&self, ctx: &Ctx) -> Option<&Bindings> {
        self.bind.get_or_init(|| serde_json::from_value(ctx.data.bindings.clone()).ok()).as_ref()
    }

    fn store_path(ctx: &Ctx) -> std::path::PathBuf {
        ctx.module_dir("auction").join("items.json")
    }

    fn save(&self, ctx: &Ctx) {
        let s = self.store.lock().unwrap();
        let live: Map<String, Value> = s.iter().filter(|(_, v)| v["demo"] != json!(true)).map(|(k, v)| (k.clone(), v.clone())).collect();
        if let Err(e) = save_json(&Self::store_path(ctx), &Value::Object(live)) {
            ctx.log(format!("auction: cannot save store: {e}"));
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
        let mins: Vec<i64> = rows
            .iter()
            .filter_map(|r| r["prices"].get(0).and_then(|p| p["price"].as_i64()))
            .filter(|p| *p > 0)
            .collect();
        let ent = json!({
            "gid": gid,
            "name": if f["title"].is_null() { rows[0]["name"].clone() } else { f["title"].clone() },
            "type": if f["type"].is_null() { rows[0]["type"].clone() } else { f["type"].clone() },
            "level": rows[0]["level"], "icon": rows[0]["icon"], "updated": m.raw.t, "lots": rows.len(),
            "min_price": mins.iter().min(), "rows": rows, "defaults": f["defaults"], "demo": m.raw.srv == "demo",
        });
        self.store.lock().unwrap().insert(gid.to_string(), ent.clone());
        self.save(ctx);
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

/// ObjectEffect message -> annotated-later Effect.
fn parse_effect(data: &Data, b: &ObjectEffect, e: &DynamicMessage) -> Effect {
    let acts: Vec<i64> = b.action.iter().map(|n| int(e, n)).collect();
    let action = acts
        .iter()
        .find(|a| data.effects.contains_key(a))
        .or_else(|| acts.iter().find(|a| **a != 0))
        .or(acts.first())
        .copied()
        .unwrap_or(0);
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
        status: None,
        range: None,
    }
}

fn format_ah_list(data: &Data, b: &Bindings, msg: &DynamicMessage) -> Value {
    let names = |v: &[String]| -> Vec<i64> { v.iter().map(|n| int(msg, n)).collect() };
    let (gid0, type0, _) = data.resolve_gid_type(&names(&b.ah_list.ints));
    let mut rows = vec![];
    for it in messages(msg, &b.ah_list.items) {
        let ints: Vec<i64> = b.ah_item.ints.iter().map(|n| int(&it, n)).collect();
        let (mut gid, mut typ, mut uid) = data.resolve_gid_type(&ints);
        if gid.is_none() {
            (gid, typ, uid) = (gid0, type0, None);
        }
        let prices = int_list(&it, &b.ah_item.prices);
        let info = gid.and_then(|g| data.items.get(&g));
        let effs: Vec<Effect> = messages(&it, &b.ah_item.effects).iter().map(|e| parse_effect(data, &b.object_effect, e)).collect();
        let meta: Map<String, Value> = effs
            .iter()
            .filter_map(|e| info_key(e.action).map(|k| (k.to_string(), e.str.clone().map_or(Value::Null, Value::from))))
            .collect();
        let locked = effs.iter().any(|e| e.action == LOCK_EFFECT);
        let stats: Vec<Effect> = effs.into_iter().filter(|e| info_key(e.action).is_none() && e.action != LOCK_EFFECT).collect();
        let density = (data.item_density(&stats) * 1000.0).round() / 1000.0;
        rows.push(json!({
            "uid": uid, "gid": gid, "ints": ints,
            "name": gid.map_or_else(|| "item#None".to_string(), |g| data.item_name(g)),
            "level": info.map(|i| i.level), "type": data.type_name(typ.or(info.map(|i| i.type_id))),
            "icon": info.map(|i| i.icon_id),
            "prices": prices.iter().enumerate().map(|(i, p)| json!({"qty": LOT_SIZES.get(i).copied().unwrap_or(i as i64), "price": p})).collect::<Vec<_>>(),
            "meta": meta, "locked": locked, "density": density,
            "effects": data.compare_effects(stats, gid),
        }));
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
        Interest::Types(&[AH_LIST])
    }

    fn init(&self, ctx: &Ctx) {
        if let Some(Value::Object(saved)) = load_json(&Self::store_path(ctx)) {
            let mut s = self.store.lock().unwrap();
            for (k, mut v) in saved {
                if v.get("defaults").is_none() {
                    v["defaults"] = Value::Array(ctx.data.defaults_for(v["gid"].as_i64()));
                }
                s.insert(k, v);
            }
        }
    }

    fn format(&self, m: &Message, ctx: &Ctx) -> Option<Value> {
        if m.name != AH_LIST {
            return None;
        }
        Some(format_ah_list(&ctx.data, self.bindings(ctx)?, m.decoded.as_ref()?))
    }

    fn on_message(&self, m: &Message, ctx: &Ctx) {
        self.update_store(m, ctx);
    }

    fn call(&self, method: &str, _args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            "items" => Ok(Value::Array(self.store.lock().unwrap().values().cloned().collect())),
            "effects" => Ok(ctx.data.filterable_effects()),
            "effect_meta" => Ok(ctx.data.effect_meta()),
            "clear" => {
                self.store.lock().unwrap().clear();
                self.save(ctx);
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
        let emit: Arc<dyn Fn(&str, Value) + Send + Sync> = Arc::new(move |n, v| ev.lock().unwrap().push((n.to_string(), v)));
        let dir = std::env::temp_dir().join("loutil-golden");
        let _ = std::fs::remove_dir_all(&dir);
        let hub = crate::Hub::new(data, proto, dir, emit, Arc::new(|_| {}));

        let payload = std::fs::read(root.join("fixtures/ah_demo.bin")).unwrap();
        hub.handle(RawMessage { t: 1.0, srv: "demo".into(), dir: "S->C", payload });

        let mut expected: Value = serde_json::from_slice(&std::fs::read(root.join("fixtures/ah_demo.expected.json")).unwrap()).unwrap();
        let events = events.lock().unwrap();
        let (_, pkt) = events.iter().find(|(n, _)| n == "packets:packet").expect("packet event");
        assert_eq!(pkt["name"], expected["name"]);
        let mut got = pkt["fmt"].clone();
        strip_keys(&mut got, &["group", "order", "density"]); // added after the python reference was written
        let mut want = expected["fmt"].take();
        strip_nulls(&mut got);
        strip_nulls(&mut want);
        assert_eq!(got, want);
        assert!(events.iter().any(|(n, _)| n == "auction:item"));
    }
}
