//! Jobs: the player's job levels and xp (Crafting « XP métiers »), and the pack's job / craft data for its planner.
//!
//! JobExperiencesUpdateEvent (script-named) {experiences: [JobExperience]} comes at login (every job) and after each craft
//! or harvest (that job). JobExperience's fields are only half named: two int32 = job id / level, three int64 = xp / level
//! floor / next level floor, told apart by value: floor ≤ xp < next floor, and a level's floor is L × (L − 1) × 10 (job 11
//! at level 103: floor 105,060, xp 106,817, next 107,120 on work/capture_bosses.jsonl; at level 200 the next floor is
//! absent). Kept per character (CharacterSelectionRequest `character_id`; 0 before any login is seen) in
//! `<app data>/jobs/levels.json`, event `jobs:levels` on each change.
use crate::kit::{unknown, Store};
use crate::{Ctx, Interest, Message, Module};
use anyhow::Result;
use loutil_proto::util::messages;
use loutil_proto::{DynamicMessage, Kind, PbValue, ReflectMessage};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const XP: &str = "JobExperiencesUpdateEvent";
const SELECT: &str = crate::combat::SELECT;
const FIRST_SELECT: &str = crate::combat::FIRST_SELECT;
pub const MAX_LEVEL: i64 = 200;

/// Total xp at which a job reaches `level`.
pub fn floor(level: i64) -> i64 {
    level * (level - 1) * 10
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct JobXp {
    pub level: i64,
    /// Total xp.
    pub xp: i64,
}

/// One JobExperience: (job id, its level and xp), None when its ints don't fit a job (level and floor not matching).
fn experience(m: &DynamicMessage, known: &dyn Fn(i64) -> bool) -> Option<(i64, JobXp)> {
    let (mut small, mut big) = (vec![], vec![]);
    for f in m.descriptor().fields().filter(|f| !f.is_list()) {
        let v = match m.get_field(&f).as_ref() {
            PbValue::I32(v) => *v as i64,
            PbValue::U32(v) => *v as i64,
            PbValue::I64(v) => *v,
            PbValue::U64(v) => *v as i64,
            _ => continue,
        };
        match f.kind() {
            Kind::Int32 | Kind::Uint32 | Kind::Sint32 => small.push(v),
            Kind::Int64 | Kind::Uint64 | Kind::Sint64 => big.push(v),
            _ => {}
        }
    }
    let [a, b] = small[..] else { return None };
    // the level is the int whose floor is among the xp values (an unset floor reads 0: level 1); a known job id breaks a tie
    let fits = |level: i64| (1..=MAX_LEVEL).contains(&level) && big.contains(&floor(level));
    let (job, level) = match (fits(b), fits(a)) {
        (true, false) => (a, b),
        (false, true) => (b, a),
        (true, true) if known(a) && !known(b) => (a, b),
        (true, true) => (b, a),
        _ => return None,
    };
    let next = (level < MAX_LEVEL).then(|| floor(level + 1));
    let xp = big.iter().copied().filter(|&v| v >= floor(level) && next.is_none_or(|n| v < n)).max().unwrap_or(floor(level));
    Some((job, JobXp { level, xp }))
}

#[derive(Serialize, Deserialize, Default)]
struct Levels {
    /// The character logged in last (0: none seen).
    current: i64,
    /// character id -> job id -> level / xp
    characters: BTreeMap<i64, BTreeMap<i64, JobXp>>,
}

#[derive(Default)]
pub struct Jobs {
    levels: Store<Levels>,
    /// Catalogue sent to the UI, built once.
    catalog: Mutex<Option<Value>>,
}

impl Jobs {
    pub fn new() -> Self {
        Self::default()
    }

    fn levels_json(&self) -> Value {
        json!(*self.levels.lock())
    }

    /// The pack's crafting jobs and crafts: {jobs: {id: name}, crafts: {gid: [job, level, ratio, item category, item type]},
    /// types: {type: name}} (category 0 equipment, 1 consumable, 2 resource; -1 / 0 when the item is unknown).
    fn catalog(&self, ctx: &Ctx) -> Value {
        self.catalog
            .lock()
            .get_or_insert_with(|| {
                let jobs: BTreeMap<String, String> = ctx
                    .data
                    .jobs
                    .iter()
                    .filter(|(id, _)| ctx.data.crafts.values().any(|c| c.0 == **id))
                    .map(|(id, j)| (id.to_string(), ctx.data.text(j.name_id).unwrap_or("?").to_string()))
                    .collect();
                let mut types = BTreeMap::new();
                let crafts: BTreeMap<String, [i64; 5]> = ctx
                    .data
                    .crafts
                    .iter()
                    .map(|(g, c)| {
                        let t = ctx.data.items.get(g).map(|i| i.type_id).unwrap_or(0);
                        let category = ctx.data.itemtypes.get(&t).and_then(|t| t.category_id).unwrap_or(-1);
                        types.entry(t.to_string()).or_insert_with(|| ctx.data.type_name(Some(t)));
                        (g.to_string(), [c.0, c.1, c.2, category, t])
                    })
                    .collect();
                json!({"jobs": jobs, "crafts": crafts, "types": types})
            })
            .clone()
    }
}

impl Module for Jobs {
    fn id(&self) -> &'static str {
        "jobs"
    }

    fn interest(&self) -> Interest {
        Interest::Types(&[XP, SELECT, FIRST_SELECT])
    }

    fn init(&self, ctx: &Ctx) {
        let saved = self.levels.open(ctx, "jobs", "levels.json", |l| json!(l));
        if let Some(l) = saved.and_then(|v| serde_json::from_value(v).ok()) {
            *self.levels.lock() = l;
        }
    }

    fn on_message(&self, m: &Message, ctx: &Ctx) {
        let Some(d) = m.decoded.as_ref() else { return };
        let field = |clear: &str| ctx.proto.field_named(d.descriptor().full_name(), clear);
        if m.name == SELECT || m.name == FIRST_SELECT {
            let Some(id) = field("character_id").and_then(|f| d.get_field_by_name(&f).and_then(|v| loutil_proto::util::as_i64(&v))).filter(|id| *id != 0)
            else {
                return;
            };
            if self.levels.lock().current != id {
                self.levels.update(|l| l.current = id);
                ctx.emit("jobs", "levels", self.levels_json());
            }
            return;
        }
        let Some(list) = field("experiences") else { return };
        let known = |id: i64| ctx.data.jobs.contains_key(&id);
        let got: Vec<(i64, JobXp)> = messages(d, &list).iter().filter_map(|e| experience(e, &known)).collect();
        if got.is_empty() {
            return;
        }
        self.levels.update(|l| {
            let mine = l.characters.entry(l.current).or_default();
            mine.extend(got);
        });
        ctx.emit("jobs", "levels", self.levels_json());
    }

    /// `levels`: {current, characters: {id: {job: {level, xp}}}}; `catalog`: the pack's crafting jobs and crafts.
    fn call(&self, method: &str, _args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            "levels" => Ok(self.levels_json()),
            "catalog" => Ok(self.catalog(ctx)),
            _ => Err(unknown("jobs", method)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::testkit;
    use loutil_core::RawMessage;

    #[test]
    fn floors_follow_the_game() {
        assert_eq!((floor(1), floor(4), floor(103), floor(104), floor(200)), (0, 120, 105_060, 107_120, 398_000));
    }

    /// The login of work/capture_bosses.jsonl: every job with its level and xp (job 11 Forgeron at 103).
    #[test]
    fn levels_read_at_login() {
        let Some(pack) = testkit::pack() else { return };
        let rec = pack.join("../work/capture_bosses.jsonl");
        if !rec.exists() {
            return;
        }
        let (hub, _) = testkit::hub("loutil-jobs-replay").unwrap();
        let hex = |s: &str| (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect::<Vec<u8>>();
        for line in std::fs::read_to_string(&rec).unwrap().lines() {
            let r: Value = serde_json::from_str(line).unwrap();
            if r["name"] != XP {
                continue;
            }
            let (Some(h), Some(len)) = (r["hex"].as_str(), r["len"].as_u64()) else { continue };
            if h.len() / 2 >= len as usize {
                hub.handle(RawMessage { t: 0.0, srv: "x".into(), dir: "S->C", payload: hex(h) });
            }
        }
        let l = hub.call("jobs", "levels", Value::Null).unwrap();
        let mine = l["characters"].as_object().unwrap().values().next().expect("a character's jobs").clone();
        assert_eq!(mine["11"], json!({"level": 103, "xp": 106_817}));
        assert_eq!(mine["2"]["level"], 4);
        for (job, x) in mine.as_object().unwrap() {
            let (level, xp) = (x["level"].as_i64().unwrap(), x["xp"].as_i64().unwrap());
            assert!(xp >= floor(level) && (level == MAX_LEVEL || xp < floor(level + 1)), "job {job}: {x}");
        }
        let cat = hub.call("jobs", "catalog", Value::Null).unwrap();
        assert_eq!(cat["jobs"]["11"], "Forgeron");
        assert!(cat["crafts"].as_object().unwrap().len() > 1000);
        let crafts = cat["crafts"].as_object().unwrap();
        assert!(crafts.values().any(|c| c[0] == 11 && c[3] == 0), "Forgeron makes equipment");
        assert!(crafts.values().any(|c| c[3] == 2), "resources are crafted too");
    }
}
