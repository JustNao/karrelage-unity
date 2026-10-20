//! Brisage (breaking items into forgemagie runes): the pack data the Brisage tab simulates with. Every breakable equipment
//! piece with its default effects that yield a rune, and for each such effect the base rune it yields ("Rune Fo", not the
//! Pa / Ra ones) with its weight. The rune quantities and the profit are computed in the UI (`app/src/lib/brisage.ts`).
use crate::{Ctx, Interest, Message, Module};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::OnceLock;

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

pub struct Brisage {
    items: OnceLock<Value>,
}

impl Default for Brisage {
    fn default() -> Self {
        Self::new()
    }
}

impl Brisage {
    pub fn new() -> Self {
        Self { items: OnceLock::new() }
    }

    /// Effect id -> its base rune {gid, name, points (what the rune adds), unit (weight per point = effectPowerRate),
    /// weight (= unit x points: rune Fo 1, Sa 3, Vi 5 x 0.2 = 1, Pod 10 x 0.25 = 2.5, Ga Pa 100)}.
    fn runes(ctx: &Ctx) -> HashMap<i64, Value> {
        let mut best: HashMap<i64, (i64, i64)> = HashMap::new(); // effect -> (points, gid): the weakest rune of the effect
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
                (unit > 0.0).then(|| {
                    (a, json!({"gid": gid, "name": ctx.data.item_name(gid), "points": points, "unit": unit, "weight": unit * points as f64, "label": ctx.data.effect_label(a)}))
                })
            })
            .collect()
    }

    fn build(ctx: &Ctx) -> Value {
        let runes = Self::runes(ctx);
        let mut items: Vec<Value> = ctx
            .data
            .items
            .iter()
            .filter(|(_, i)| breakable(i.type_id))
            .filter_map(|(g, i)| {
                let effects: Vec<Value> = i.effects.iter().filter(|e| e.max > 0 && runes.contains_key(&e.a)).map(|e| json!({"a": e.a, "min": e.min, "max": e.max})).collect();
                (!effects.is_empty()).then(|| {
                    json!({
                        "gid": g, "name": ctx.data.item_name(*g), "level": i.level, "type_id": i.type_id, "type": ctx.data.type_name(Some(i.type_id)),
                        "drop": i.drop, "craft": ctx.data.recipes.contains_key(g), "effects": effects,
                    })
                })
            })
            .collect();
        items.sort_by_key(|x| x["gid"].as_i64());
        let runes: serde_json::Map<String, Value> = runes.into_iter().map(|(a, r)| (a.to_string(), r)).collect();
        json!({"items": items, "runes": runes})
    }
}

impl Module for Brisage {
    fn id(&self) -> &'static str {
        "brisage"
    }

    fn interest(&self) -> Interest {
        Interest::Types(&[])
    }

    fn wants(&self) -> bool {
        false
    }

    fn on_message(&self, _m: &Message, _ctx: &Ctx) {}

    fn call(&self, method: &str, _args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            // {items: [{gid, name, level, type_id, type, drop, craft, effects: [{a, min, max}]}], runes: {effect id: rune}}
            "items" => Ok(self.items.get_or_init(|| Self::build(ctx)).clone()),
            _ => Err(anyhow!("brisage: unknown method {method}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loutil_data::Data;
    use loutil_proto::Proto;
    use std::path::Path;
    use std::sync::Arc;

    /// Needs the generated data pack; skipped otherwise.
    #[test]
    fn base_runes_and_breakable_items() {
        let pack = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../datapack");
        if !pack.join("manifest.json").exists() {
            return;
        }
        let dir = std::env::temp_dir().join("loutil-brisage");
        let data = Arc::new(Data::load(&pack, "fr").unwrap());
        let proto = Arc::new(Proto::load(&pack, dir.join("o.json")).unwrap());
        let hub = crate::Hub::new(data, proto, dir, Arc::new(|_, _| {}), Arc::new(|_| {}));
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
        let by = |g: i64| items.iter().find(|i| i["gid"] == g);
        if let Some(c) = by(31789) {
            assert_eq!(c["level"], 200);
            assert_eq!(c["drop"], true);
        }
        assert!(items.iter().all(|i| breakable(i["type_id"].as_i64().unwrap())));
        assert!(!items.iter().any(|i| i["type_id"] == 23 || i["type_id"] == 151 || i["type_id"] == 18), "no Dofus / trophée / familier");
    }
}
