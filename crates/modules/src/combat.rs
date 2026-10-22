//! Combat: the monsters of the player's current fight, with the author's notes for each (warning / invulnerability / tips,
//! data pack `monsters.json`). Fighters come from the fight messages (shown at placement, synchronised every turn, summoned
//! during the fight); each one is an ActorPositionInformation whose MonsterFighter carries the creature gid, grade and level
//! as three unordered ints, resolved with the game's grade levels. Strictly passive, like everything else.
use crate::{Ctx, Interest, Message, Module};
use anyhow::{anyhow, Result};
use loutil_data::Data;
use loutil_proto::util::{as_i64, int};
use loutil_proto::{DynamicMessage, PbValue, ReflectMessage};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// The player enters a fight (placement starts): a new fight.
pub const JOIN: &str = "FightJoinEvent";
/// Reconnection to a running fight: a new fight too (the fighters follow).
pub const JOIN_RUNNING: &str = "FightJoinRunningEvent";
/// One fighter shown / refreshed (placement).
pub const SHOW: &str = "FightFighterShowEvent";
pub const REFRESH: &str = "FightFighterRefreshEvent";
/// Every fighter of the fight (start of each turn).
pub const SYNC: &str = "FightSynchronizeEvent";
/// Fight actions: only the summons (new fighters) are read.
pub const ACTION: &str = "GameActionFightEvent";
pub const END: &str = "FightEndEvent";

/// Obfuscated field names of the fighter messages (bindings.json `fight`).
#[derive(Deserialize, Clone)]
struct Bindings {
    actor_id: String,
    monster: Vec<String>,
    summoned: String,
}

/// A monster fighter: creature gid, grade (1-based), level, and whether it was summoned during the fight.
#[derive(Clone, Debug, PartialEq)]
struct Fighter {
    gid: i64,
    grade: i64,
    level: i64,
    summoned: bool,
}

#[derive(Default)]
struct Fight {
    /// a fight was seen (it may be over)
    seen: bool,
    started: f64,
    /// end time, None while it runs
    ended: Option<f64>,
    /// actor id -> monster
    fighters: BTreeMap<i64, Fighter>,
}

pub struct Combat {
    bind: OnceLock<Option<Bindings>>,
    enabled: AtomicBool,
    fight: Mutex<Fight>,
}

impl Default for Combat {
    fn default() -> Self {
        Self::new()
    }
}

/// (gid, grade, level) from the three unordered ints of a MonsterFighter: the permutation whose gid is a monster and whose
/// grade has that level, else a monster gid with a valid grade (scaled level), else the int that is a monster (grade / level
/// unknown, 0).
fn resolve(data: &Data, ints: &[i64]) -> Option<(i64, i64, i64)> {
    const PERMS: [[usize; 3]; 6] = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
    if ints.len() != 3 {
        return None;
    }
    let grade_ok = |g: i64, gr: i64| data.monsters.get(&g).is_some_and(|m| gr >= 1 && gr as usize <= m.levels.len());
    let exact = PERMS.iter().map(|p| (ints[p[0]], ints[p[1]], ints[p[2]])).find(|&(g, gr, l)| grade_ok(g, gr) && data.monsters[&g].levels[gr as usize - 1] == l);
    exact
        .or_else(|| PERMS.iter().map(|p| (ints[p[0]], ints[p[1]], ints[p[2]])).filter(|&(g, gr, _)| grade_ok(g, gr)).max_by_key(|&(g, _, _)| g))
        .or_else(|| ints.iter().filter(|g| data.monsters.contains_key(g)).max().map(|&g| (g, 0, 0)))
}

/// Child messages of `m` (message fields, lists and map values).
fn children(m: &DynamicMessage) -> Vec<DynamicMessage> {
    let mut out = vec![];
    for (_, v) in m.fields() {
        match v {
            PbValue::Message(x) => out.push(x.clone()),
            PbValue::List(l) => out.extend(l.iter().filter_map(|x| x.as_message().cloned())),
            PbValue::Map(mp) => out.extend(mp.values().filter_map(|x| x.as_message().cloned())),
            _ => {}
        }
    }
    out
}

/// First message of the subtree (depth first, `m` included) that has the field `name` in its schema.
fn find(m: &DynamicMessage, name: &str) -> Option<DynamicMessage> {
    if m.descriptor().get_field_by_name(name).is_some() {
        return Some(m.clone());
    }
    children(m).iter().find_map(|c| find(c, name))
}

/// Every monster fighter of a message: (actor id, fighter). Actors that are not monsters (players, tax collectors) are left out.
fn fighters(data: &Data, b: &Bindings, m: &DynamicMessage, out: &mut Vec<(i64, Fighter)>) {
    if m.descriptor().get_field_by_name(&b.actor_id).is_none() {
        for c in children(m) {
            fighters(data, b, &c, out);
        }
        return;
    }
    let Some(mon) = b.monster.first().and_then(|f| find(m, f)) else { return };
    let ints: Vec<i64> = b.monster.iter().map(|f| int(&mon, f)).collect();
    let Some((gid, grade, level)) = resolve(data, &ints) else { return };
    let summoned = find(m, &b.summoned).and_then(|s| s.get_field_by_name(&b.summoned).and_then(|v| as_i64(&v))).unwrap_or(0) != 0;
    out.push((int(m, &b.actor_id), Fighter { gid, grade, level, summoned }));
}

impl Combat {
    pub fn new() -> Self {
        Self { bind: OnceLock::new(), enabled: AtomicBool::new(true), fight: Mutex::new(Fight::default()) }
    }

    fn bindings(&self, ctx: &Ctx) -> Option<&Bindings> {
        self.bind.get_or_init(|| serde_json::from_value(ctx.data.bindings.get("fight")?.clone()).ok()).as_ref()
    }

    /// The fight for the UI: {active, started, ended, fighters: [{actor, gid, name, grade, level, summoned, notes}]}.
    fn state(&self, data: &Data) -> Value {
        let f = self.fight.lock();
        let list: Vec<Value> = f
            .fighters
            .iter()
            .map(|(actor, x)| {
                let m = data.monsters.get(&x.gid);
                json!({
                    "actor": actor, "gid": x.gid, "grade": x.grade, "level": x.level, "summoned": x.summoned,
                    "name": m.and_then(|m| data.text(m.name_id)).map(str::to_owned).unwrap_or_else(|| format!("monstre#{}", x.gid)),
                    "notes": m.and_then(|m| m.notes.as_ref()),
                })
            })
            .collect();
        json!({"seen": f.seen, "active": f.seen && f.ended.is_none(), "started": f.started, "ended": f.ended, "fighters": list})
    }
}

impl Module for Combat {
    fn id(&self) -> &'static str {
        "combat"
    }

    fn interest(&self) -> Interest {
        Interest::Types(&[JOIN, JOIN_RUNNING, SHOW, REFRESH, SYNC, ACTION, END])
    }

    fn wants(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    fn on_message(&self, m: &Message, ctx: &Ctx) {
        let t = m.raw.t;
        let changed = {
            let mut f = self.fight.lock();
            match m.name.as_str() {
                JOIN | JOIN_RUNNING => {
                    *f = Fight { seen: true, started: t, ..Default::default() };
                    true
                }
                END => {
                    let open = f.seen && f.ended.is_none();
                    if open {
                        f.ended = Some(t);
                    }
                    open
                }
                _ => {
                    let (Some(b), Some(d)) = (self.bindings(ctx), &m.decoded) else { return };
                    let mut found = vec![];
                    fighters(&ctx.data, b, d, &mut found);
                    if found.is_empty() {
                        return;
                    }
                    // fighters without a fight start (app opened mid-fight) or after its end: a new fight
                    if !f.seen || f.ended.is_some() {
                        *f = Fight { seen: true, started: t, ..Default::default() };
                    }
                    // the synchronisation lists every fighter; the other messages add or update some
                    let mut next = if m.name == SYNC { BTreeMap::new() } else { f.fighters.clone() };
                    next.extend(found);
                    let changed = next != f.fighters;
                    f.fighters = next;
                    changed
                }
            }
        };
        if changed {
            ctx.emit("combat", "fight", self.state(&ctx.data));
        }
    }

    fn call(&self, method: &str, args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            "state" => Ok(self.state(&ctx.data)),
            // the feature switch (Combat tab / Paramètres): off = the fight messages are not even decoded
            "set_enabled" => {
                let on = args["on"].as_bool().unwrap_or(true);
                self.enabled.store(on, Ordering::Relaxed);
                if !on {
                    *self.fight.lock() = Fight::default();
                }
                Ok(json!({"enabled": on}))
            }
            "clear" => {
                *self.fight.lock() = Fight::default();
                let s = self.state(&ctx.data);
                ctx.emit("combat", "fight", s.clone());
                Ok(s)
            }
            _ => Err(anyhow!("unknown method {method}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loutil_core::RawMessage;
    use loutil_proto::{FieldDescriptor, MessageDescriptor, Proto};
    use std::sync::Arc;

    /// Field path from `md` down to a message that has the field `target` (depth first, each message type once).
    fn path(md: &MessageDescriptor, target: &str, seen: &mut Vec<String>) -> Option<Vec<FieldDescriptor>> {
        if md.get_field_by_name(target).is_some() {
            return Some(vec![]);
        }
        if seen.contains(&md.full_name().to_string()) {
            return None;
        }
        seen.push(md.full_name().to_string());
        md.fields().find_map(|f| {
            let sub = f.kind().as_message()?.clone();
            let mut p = path(&sub, target, seen)?;
            p.insert(0, f);
            Some(p)
        })
    }

    /// Set `field` = `v` on the message at the end of `p` inside `m`, creating the messages on the way.
    fn set(m: &mut DynamicMessage, p: &[FieldDescriptor], field: &str, v: PbValue) {
        match p.split_first() {
            None => m.set_field_by_name(field, v),
            Some((f, rest)) => {
                let mut sub = match m.get_field(f).into_owned() {
                    PbValue::Message(x) if m.has_field(f) => x,
                    _ => DynamicMessage::new(f.kind().as_message().unwrap().clone()),
                };
                set(&mut sub, rest, field, v);
                m.set_field(f, PbValue::Message(sub));
            }
        }
    }

    /// A synthetic fight: placement shows a monster, the turn synchronisation lists it plus a summon, then the fight ends.
    #[test]
    fn follows_the_fight_monsters() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let pack = root.join("datapack");
        if !pack.join("manifest.json").exists() {
            return;
        }
        let dir = std::env::temp_dir().join("loutil-combat");
        let _ = std::fs::remove_dir_all(&dir);
        let data = Arc::new(Data::load(&pack, "fr").unwrap());
        if data.monsters.is_empty() {
            eprintln!("skipped: pack without monsters.json");
            return;
        }
        let proto = Arc::new(Proto::load(&pack, dir.join("o.json")).unwrap());
        let b: Bindings = serde_json::from_value(data.bindings["fight"].clone()).unwrap();
        // a noted monster with several grades, and its grade 2 (ints given in a scrambled order)
        let (&gid, mon) = data.monsters.iter().filter(|(_, m)| m.notes.is_some() && m.levels.len() >= 2).min_by_key(|(g, _)| **g).unwrap();
        let level = mon.levels[1];

        let sync_md = proto.message(&proto.obf_for_clear(SYNC).unwrap()).unwrap();
        let list_field = sync_md.fields().next().unwrap();
        let actor_md = list_field.kind().as_message().unwrap().clone();
        assert!(actor_md.get_field_by_name(&b.actor_id).is_some(), "fighters are ActorPositionInformation");
        let to_monster = path(&actor_md, &b.monster[0], &mut vec![]).unwrap();
        let to_summoned = path(&actor_md, &b.summoned, &mut vec![]).unwrap();
        let actor = |id: i64, g: i64, summoned: bool| {
            let mut a = DynamicMessage::new(actor_md.clone());
            a.set_field_by_name(&b.actor_id, PbValue::I64(id));
            for (f, v) in b.monster.iter().zip([level, g, 2]) {
                set(&mut a, &to_monster, f, PbValue::I32(v as i32));
            }
            set(&mut a, &to_summoned, &b.summoned, PbValue::Bool(summoned));
            a
        };
        let show_md = proto.message(&proto.obf_for_clear(SHOW).unwrap()).unwrap();
        let mut show = DynamicMessage::new(show_md.clone());
        let info = show_md.fields().find(|f| f.kind().as_message().is_some_and(|k| k.full_name() == actor_md.full_name())).unwrap();
        show.set_field(&info, PbValue::Message(actor(-1, gid, false)));
        let mut sync = DynamicMessage::new(sync_md);
        sync.set_field(&list_field, PbValue::List(vec![PbValue::Message(actor(-1, gid, false)), PbValue::Message(actor(-2, gid, true))]));
        let end = DynamicMessage::new(proto.message(&proto.obf_for_clear(END).unwrap()).unwrap());

        let hub = crate::Hub::new(data, proto, dir, Arc::new(|_, _| {}), Arc::new(|_| {}));
        let message = |name: &str, decoded: DynamicMessage, t: f64| Message {
            id: 1,
            raw: RawMessage { t, srv: "x".into(), dir: "S->C", payload: vec![] },
            env: None,
            type_name: None,
            name: name.into(),
            conf: None,
            decoded: Some(decoded),
            fmt: None,
        };
        let c = Combat::new();
        c.on_message(&message(SHOW, show, 1.0), &hub.ctx);
        let s = c.call("state", Value::Null, &hub.ctx).unwrap();
        assert_eq!(s["active"], json!(true), "a fighter starts a fight even without its join message");
        assert_eq!(s["fighters"].as_array().unwrap().len(), 1);
        let f = &s["fighters"][0];
        assert_eq!((f["gid"].as_i64(), f["grade"].as_i64(), f["level"].as_i64()), (Some(gid), Some(2), Some(level)));
        assert!(f["notes"].is_object(), "the monster's notes come along");

        c.on_message(&message(SYNC, sync, 2.0), &hub.ctx);
        let s = c.call("state", Value::Null, &hub.ctx).unwrap();
        assert_eq!(s["fighters"].as_array().unwrap().len(), 2);
        assert_eq!(s["fighters"][0]["summoned"], json!(true), "actor -2 is the summon");

        c.on_message(&message(END, end, 3.0), &hub.ctx);
        let s = c.call("state", Value::Null, &hub.ctx).unwrap();
        assert_eq!((s["active"].clone(), s["ended"].clone()), (json!(false), json!(3.0)), "the last fight stays shown, ended");
    }
}
