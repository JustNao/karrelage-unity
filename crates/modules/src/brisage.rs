//! Brisage (breaking items into forgemagie runes): the pack data the Brisage tab simulates with, and the item coefficients.
//! Pack data = every breakable equipment piece with its default effects that yield a rune, and for each such effect the base
//! rune it yields ("Rune Fo", not the Pa / Ra ones) with its weight. The rune quantities and the profit are computed in the
//! UI (`app/src/lib/brisage.ts`). Coefficients (gid -> %) come from the break results the game sends (`DecraftResultEvent`)
//! or are typed in the tab; the latest one wins.
use crate::inventory::UidMap;
use crate::{Ctx, Interest, Message, Module, Persist};
use anyhow::{anyhow, Result};
use loutil_proto::util::{int, messages};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock};

/// Clear name of the break result (one entry per item broken: its coefficient, its uid and the runes received).
pub const DECRAFT: &str = "DecraftResultEvent";

/// itemtypes id of the forgemagie runes (Rune Fo, Rune Pa Fo, Rune Ga Pa...).
const FM_RUNE_TYPE: i64 = 78;
/// Effects whose rune is left out: the hunting rune (795, "Arme de chasse") carries no value to break into.
const NO_RUNE_EFFECTS: [i64; 1] = [795];
/// Equipment types that can be broken: weapons, rings, amulets, belts, boots, hats, cloaks, shields. Familiers, montures,
/// Dofus, trophées, prysmaradites, compagnons and perceptor gear cannot.
const BREAKABLE_TYPES: [i64; 20] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 16, 17, 19, 20, 21, 22, 82, 114, 271];

fn breakable(type_id: i64) -> bool {
    BREAKABLE_TYPES.contains(&type_id)
}

fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// Obfuscated field names of the break result (bindings.json `decraft`).
#[derive(Deserialize, Clone)]
struct Bindings {
    results: String,
    coef_min: String,
    coef_max: String,
    uid: String,
    runes: String,
    rune_gid: String,
    rune_qty: String,
}

/// The base rune of an effect.
#[derive(Clone)]
struct Rune {
    gid: i64,
    /// What the rune adds ("Rune Vi" = 5).
    points: i64,
    /// Weight per point of the effect (effectPowerRate).
    unit: f64,
}

impl Rune {
    fn weight(&self) -> f64 {
        self.unit * self.points as f64
    }
}

/// Pack-derived, constant once the pack is loaded.
struct Pack {
    /// effect id -> its base rune
    runes: HashMap<i64, Rune>,
    /// breakable gid -> (level, rune-yielding default effects (a, min, max))
    items: BTreeMap<i64, (i64, Vec<(i64, i64, i64)>)>,
    json: Value,
}

pub struct Brisage {
    pack: OnceLock<Pack>,
    bind: OnceLock<Option<Bindings>>,
    /// gid -> {gid, coef (%), t, source: "brisage" | "manuel", uid?, guessed?, runes?: [[rune gid, qty]]}
    coefs: Arc<Mutex<BTreeMap<i64, Value>>>,
    persist: OnceLock<Persist>,
    uids: UidMap,
    /// Break results whose item could not be told (latest last): {uid, coef, t, runes}; the user can assign them to an item.
    pending: Mutex<Vec<Value>>,
}

/// Unassigned break results kept for the session.
const MAX_PENDING: usize = 10;

impl Default for Brisage {
    fn default() -> Self {
        Self::new(Default::default())
    }
}

impl Brisage {
    /// `uids`: object uid -> gid of the equipment seen in the inventory (shared with the inventory module).
    pub fn new(uids: UidMap) -> Self {
        Self { pack: OnceLock::new(), bind: OnceLock::new(), coefs: Default::default(), persist: OnceLock::new(), uids, pending: Default::default() }
    }

    fn bindings(&self, ctx: &Ctx) -> Option<&Bindings> {
        self.bind.get_or_init(|| serde_json::from_value(ctx.data.bindings.get("decraft")?.clone()).ok()).as_ref()
    }

    fn pack(&self, ctx: &Ctx) -> &Pack {
        self.pack.get_or_init(|| Self::build(ctx))
    }

    /// Effect id -> its base rune: the weakest forgemagie rune of the effect. Weight = unit x points (rune Fo 1, Sa 3,
    /// Vi 5 x 0.2 = 1, Pod 10 x 0.25 = 2.5, Ga Pa 100).
    fn runes(ctx: &Ctx) -> HashMap<i64, Rune> {
        let mut best: HashMap<i64, (i64, i64)> = HashMap::new(); // effect -> (points, gid)
        for (g, i) in &ctx.data.items {
            if i.type_id != FM_RUNE_TYPE {
                continue;
            }
            let [e] = i.effects.as_slice() else { continue };
            if e.max <= 0 || NO_RUNE_EFFECTS.contains(&e.a) {
                continue;
            }
            let cur = best.entry(e.a).or_insert((e.max, *g));
            if (e.max, *g) < *cur {
                *cur = (e.max, *g);
            }
        }
        best.into_iter()
            .filter_map(|(a, (points, gid))| {
                let unit = ctx.data.effects.get(&a)?.density;
                (unit > 0.0).then_some((a, Rune { gid, points, unit }))
            })
            .collect()
    }

    fn build(ctx: &Ctx) -> Pack {
        let runes = Self::runes(ctx);
        let mut items = BTreeMap::new();
        let mut list = vec![];
        for (g, i) in &ctx.data.items {
            if !breakable(i.type_id) {
                continue;
            }
            let effects: Vec<(i64, i64, i64)> = i.effects.iter().filter(|e| e.max > 0 && runes.contains_key(&e.a)).map(|e| (e.a, e.min, e.max)).collect();
            if effects.is_empty() {
                continue;
            }
            list.push(json!({
                "gid": g, "name": ctx.data.item_name(*g), "level": i.level, "type_id": i.type_id, "type": ctx.data.type_name(Some(i.type_id)),
                "drop": i.drop, "craft": ctx.data.recipes.contains_key(g),
                "effects": effects.iter().map(|(a, min, max)| json!({"a": a, "min": min, "max": max})).collect::<Vec<_>>(),
            }));
            items.insert(*g, (i.level, effects));
        }
        list.sort_by_key(|x| x["gid"].as_i64());
        let rj: serde_json::Map<String, Value> = runes
            .iter()
            .map(|(a, r)| {
                let v = json!({"gid": r.gid, "name": ctx.data.item_name(r.gid), "points": r.points, "unit": r.unit, "weight": r.weight(), "label": ctx.data.effect_label(*a)});
                (a.to_string(), v)
            })
            .collect();
        Pack { runes, items, json: json!({"items": list, "runes": rj}) }
    }

    /// The item a break result is about when its uid is unknown (bought and broken without the inventory being read):
    /// the breakable item whose rune-yielding effects match the runes received. A result may cover a stack of identical
    /// items, so quantities are compared through their ratios to what one item's mean roll gives (one ratio for all the
    /// effects of the right item, about the stack size). No guess from a single kind of rune (a focus break) or when two
    /// items fit about as well.
    fn guess_gid(pack: &Pack, received: &[(i64, i64)]) -> Option<i64> {
        let by_rune: HashMap<i64, i64> = pack.runes.iter().map(|(a, r)| (r.gid, *a)).collect();
        let got: HashMap<i64, i64> = received.iter().filter(|(_, q)| *q > 0).filter_map(|(g, q)| Some((*by_rune.get(g)?, *q))).collect();
        if got.len() < 2 {
            return None;
        }
        let mut scored: Vec<(f64, i64)> = vec![];
        for (gid, (level, effects)) in &pack.items {
            let shared = effects.iter().filter(|e| got.contains_key(&e.0)).count();
            let (exo, missing) = (got.len() - shared, effects.len() - shared);
            if shared < 2 || exo > 1 || missing > 0 {
                continue;
            }
            let logs: Vec<f64> = effects
                .iter()
                .map(|(a, min, max)| {
                    let r = &pack.runes[a];
                    let want = 3.0 * r.unit * (*min + *max) as f64 / 2.0 * *level as f64 / 200.0 + 1.0;
                    (got[a] as f64 * r.weight() / want).ln()
                })
                .collect();
            let mean = logs.iter().sum::<f64>() / logs.len() as f64;
            let spread = (logs.iter().map(|l| (l - mean).powi(2)).sum::<f64>() / logs.len() as f64).sqrt();
            scored.push((spread + exo as f64, *gid));
        }
        scored.sort_by(|a, b| a.0.total_cmp(&b.0));
        match scored.as_slice() {
            [(s, g), rest @ ..] if *s < 0.5 && rest.first().is_none_or(|(s2, _)| s2 - s > 0.15) => Some(*g),
            _ => None,
        }
    }

    fn changed(&self, ctx: &Ctx, e: Value) {
        if let Some(p) = self.persist.get() {
            p.touch();
        }
        ctx.emit("brisage", "coef", e);
    }
}

impl Module for Brisage {
    fn id(&self) -> &'static str {
        "brisage"
    }

    fn interest(&self) -> Interest {
        Interest::Types(&[DECRAFT])
    }

    fn init(&self, ctx: &Ctx) {
        let coefs = self.coefs.clone();
        let (persist, saved) = Persist::open(ctx.module_dir("brisage").join("coefs.json"), ctx.logger(), move || json!(coefs.lock().values().cloned().collect::<Vec<_>>()));
        let _ = self.persist.set(persist);
        let mut s = self.coefs.lock();
        for e in saved.as_ref().and_then(Value::as_array).into_iter().flatten() {
            if let Some(g) = e["gid"].as_i64() {
                s.insert(g, e.clone());
            }
        }
    }

    fn on_message(&self, m: &Message, ctx: &Ctx) {
        let (Some(msg), Some(b)) = (m.decoded.as_ref(), self.bindings(ctx)) else { return };
        let float = |x: &loutil_proto::DynamicMessage, f: &str| x.get_field_by_name(f).and_then(|v| v.as_f32()).unwrap_or(0.0) as f64;
        for r in messages(msg, &b.results) {
            // a stack of identical items: coef_max for the first one broken, coef_min for the last (= the latest coefficient)
            let (lo, hi) = (float(&r, &b.coef_min), float(&r, &b.coef_max));
            let coef = (lo.min(hi) * 1000.0).round() / 10.0;
            let uid = int(&r, &b.uid);
            let runes: Vec<(i64, i64)> = messages(&r, &b.runes).iter().map(|x| (int(x, &b.rune_gid), int(x, &b.rune_qty))).collect();
            let known = self.uids.lock().get(&uid).copied();
            let Some(gid) = known.or_else(|| Self::guess_gid(self.pack(ctx), &runes)) else {
                ctx.log(format!("brisage: objet {uid} non reconnu, coefficient {coef:.1} % à attribuer dans l'onglet Brisage"));
                let mut l = self.pending.lock();
                l.push(json!({"uid": uid, "coef": coef, "t": m.raw.t, "runes": runes}));
                let extra = l.len().saturating_sub(MAX_PENDING);
                l.drain(..extra);
                ctx.emit("brisage", "pending", Value::Array(l.clone()));
                continue;
            };
            let e = json!({"gid": gid, "coef": coef, "t": m.raw.t, "source": "brisage", "uid": uid, "guessed": known.is_none(), "runes": runes});
            self.coefs.lock().insert(gid, e.clone());
            self.changed(ctx, e);
        }
    }

    fn call(&self, method: &str, args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            // {items: [{gid, name, level, type_id, type, drop, craft, effects: [{a, min, max}]}], runes: {effect id: rune}}
            "items" => Ok(self.pack(ctx).json.clone()),
            // {gid: {gid, coef, t, source, ...}}
            "coefs" => Ok(Value::Object(self.coefs.lock().iter().map(|(g, e)| (g.to_string(), e.clone())).collect())),
            // args: {gid, coef: number | null} (%); null forgets the item's coefficient
            "set_coef" => {
                let gid = args["gid"].as_i64().ok_or_else(|| anyhow!("brisage: gid required"))?;
                let e = match args["coef"].as_f64().filter(|c| *c >= 0.0) {
                    Some(c) => {
                        let e = json!({"gid": gid, "coef": c, "t": now(), "source": "manuel"});
                        self.coefs.lock().insert(gid, e.clone());
                        e
                    }
                    None => {
                        self.coefs.lock().remove(&gid);
                        json!({"gid": gid, "coef": null})
                    }
                };
                self.changed(ctx, e.clone());
                Ok(e)
            }
            // unassigned break results: [{uid, coef, t, runes}]
            "pending" => Ok(Value::Array(self.pending.lock().clone())),
            // args: {uid, t, gid}: that pending break result is about this item (gid null just dismisses it)
            "assign" => {
                let (uid, t) = (args["uid"].as_i64(), args["t"].as_f64());
                let mut l = self.pending.lock();
                let i = l.iter().position(|p| p["uid"].as_i64() == uid && p["t"].as_f64() == t).ok_or_else(|| anyhow!("brisage: unknown break result"))?;
                let p = l.remove(i);
                ctx.emit("brisage", "pending", Value::Array(l.clone()));
                drop(l);
                if let Some(gid) = args["gid"].as_i64() {
                    let e = json!({"gid": gid, "coef": p["coef"], "t": p["t"], "source": "brisage", "uid": p["uid"], "runes": p["runes"]});
                    self.coefs.lock().insert(gid, e.clone());
                    self.changed(ctx, e);
                }
                Ok(json!({"ok": true}))
            }
            _ => Err(anyhow!("brisage: unknown method {method}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loutil_core::RawMessage;
    use loutil_data::Data;
    use loutil_proto::Proto;
    use std::path::Path;

    fn hub(name: &str) -> Option<(crate::Hub, Arc<Proto>)> {
        let pack = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../datapack");
        if !pack.join("manifest.json").exists() {
            return None;
        }
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        let data = Arc::new(Data::load(&pack, "fr").unwrap());
        let proto = Arc::new(Proto::load(&pack, dir.join("o.json")).unwrap());
        Some((crate::Hub::new(data, proto.clone(), dir, Arc::new(|_, _| {}), Arc::new(|_| {})), proto))
    }

    /// Needs the generated data pack; skipped otherwise.
    #[test]
    fn base_runes_and_breakable_items() {
        let Some((hub, _)) = hub("loutil-brisage") else { return };
        let v = hub.call("brisage", "items", Value::Null).unwrap();
        let rune = |a: i64| v["runes"][a.to_string()].clone();
        let close = |x: &Value, y: f64| assert!((x.as_f64().unwrap() - y).abs() < 1e-3, "{x} vs {y}");
        assert_eq!(rune(118)["name"], "Rune Fo"); // the base rune, not Pa / Ra Fo
        close(&rune(118)["weight"], 1.0);
        close(&rune(125)["weight"], 1.0); // Rune Vi: 5 vitality x 0.2
        close(&rune(158)["weight"], 2.5); // Rune Pod: 10 pods x 0.25
        close(&rune(124)["weight"], 3.0); // Sa
        close(&rune(111)["weight"], 100.0); // Ga Pa
        assert!(rune(795).is_null(), "no hunting rune");
        let items = v["items"].as_array().unwrap();
        if let Some(c) = items.iter().find(|i| i["gid"] == 31789) {
            assert_eq!(c["level"], 200);
            assert_eq!(c["drop"], true);
        }
        assert!(items.iter().all(|i| breakable(i["type_id"].as_i64().unwrap())));
        assert!(!items.iter().any(|i| i["type_id"] == 23 || i["type_id"] == 151 || i["type_id"] == 18), "no Dofus / trophée / familier");
    }

    /// A real break result (Talisman Doline, level 137, coefficient 0.529): the uid is not in the inventory, so the item is
    /// recognised from the runes received; then a manual coefficient replaces it, and null forgets it.
    #[test]
    fn break_result_sets_the_coefficient() {
        let Some((hub, proto)) = hub("loutil-brisage-decraft") else { return };
        if proto.obf_for_clear(DECRAFT).as_deref() != Some("kag") {
            return; // payload recorded for another build
        }
        let hex = "0a630db971073f15b971073f189bdcc9022205080110950c2205080910915b2205082210ef0b2205080b10a33a2205080710895b22050808108f5b2205081d10f30b22050816109b3a2205080910875b2205081410f10b2205082c10f40b22050809109c3a";
        let payload: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
        let msg = Message {
            id: 1,
            raw: RawMessage { t: 1792528239.1, srv: "x".into(), dir: "S->C", payload: vec![] },
            env: None,
            type_name: Some("kag".into()),
            name: DECRAFT.into(),
            conf: None,
            decoded: proto.decode("kag", &payload),
            fmt: None,
        };
        let b = Brisage::new(Default::default());
        b.on_message(&msg, &hub.ctx);
        let c = b.call("coefs", Value::Null, &hub.ctx).unwrap();
        let e = &c["11609"];
        assert_eq!(e["coef"], json!(52.9), "{c}");
        assert_eq!(e["source"], "brisage");
        assert_eq!(e["uid"], 5402139);
        assert_eq!(e["guessed"], true);
        assert_eq!(e["runes"].as_array().unwrap().len(), 12);

        // with the uid known from the inventory, no guess
        let uids: UidMap = Default::default();
        uids.lock().insert(5402139, 11609);
        let b2 = Brisage::new(uids);
        b2.on_message(&msg, &hub.ctx);
        assert_eq!(b2.call("coefs", Value::Null, &hub.ctx).unwrap()["11609"]["guessed"], false);

        // a stack of three: three times the runes, still the Doline
        let pack = b.pack(&hub.ctx);
        let triple: Vec<(i64, i64)> = e["runes"].as_array().unwrap().iter().map(|r| (r[0].as_i64().unwrap(), r[1].as_i64().unwrap() * 3)).collect();
        assert_eq!(Brisage::guess_gid(pack, &triple), Some(11609));
        // a focus break (one kind of rune) is never guessed: it waits to be assigned
        assert_eq!(Brisage::guess_gid(pack, &[(7459, 225)]), None);
        let p = Brisage::new(Default::default());
        p.pending.lock().push(json!({"uid": 1, "coef": 302.7, "t": 2.0, "runes": [[7459, 225]]}));
        p.call("assign", json!({"uid": 1, "t": 2.0, "gid": 6748}), &hub.ctx).unwrap();
        assert_eq!(p.call("coefs", Value::Null, &hub.ctx).unwrap()["6748"]["coef"], json!(302.7));
        assert!(p.call("pending", Value::Null, &hub.ctx).unwrap().as_array().unwrap().is_empty());

        b.call("set_coef", json!({"gid": 11609, "coef": 40}), &hub.ctx).unwrap();
        let c = b.call("coefs", Value::Null, &hub.ctx).unwrap();
        assert_eq!((c["11609"]["coef"].as_f64(), c["11609"]["source"].as_str()), (Some(40.0), Some("manuel")));
        b.call("set_coef", json!({"gid": 11609, "coef": null}), &hub.ctx).unwrap();
        assert!(b.call("coefs", Value::Null, &hub.ctx).unwrap().get("11609").is_none());
    }
}
