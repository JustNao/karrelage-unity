//! Combat: the monsters of the player's current fight, with the author's notes for each (warning / invulnerability / tips,
//! data pack `monsters.json`). Fighters come from the fight messages (shown at placement, synchronised every turn, summoned
//! during the fight); each one is an ActorPositionInformation whose MonsterFighter carries the creature gid, grade and level
//! as three unordered ints. Which int is which is learnt (`Layout`): a message whose fighters fit only one arrangement (gid =
//! a monster, grade = one of its grades, level = a plausible level) votes for it; the winner is kept in `layout.json` (per
//! field names, so a game update starts over) and then used for every fighter, even an unknown gid. Levels are often scaled
//! (dungeons, dimensions...), so they are not required to match the grade. Strictly passive, like everything else.
use crate::{load_json, save_json, Ctx, Interest, Message, Module};
use anyhow::{anyhow, Result};
use loutil_data::Data;
use loutil_proto::util::{as_i64, int};
use loutil_proto::{DynamicMessage, PbValue, ReflectMessage};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
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
    /// GameActionFightEvent's `death` member (absent in older packs)
    #[serde(default)]
    death: Option<String>,
}

/// A monster fighter: the three unordered ints of its MonsterFighter (gid / grade / level, see `Layout`) and whether it was
/// summoned during the fight.
#[derive(Clone, Debug, PartialEq)]
struct Fighter {
    ints: [i64; 3],
    summoned: bool,
}

/// The six ways the three ints can be (gid, grade, level).
const PERMS: [[usize; 3]; 6] = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];

/// Learnt arrangement of the MonsterFighter ints: votes per `PERMS` entry, for the build whose field names are `fields`.
#[derive(Default)]
struct Layout {
    fields: Vec<String>,
    votes: [u32; 6],
}

impl Layout {
    fn best(&self) -> Option<usize> {
        (0..6).filter(|&i| self.votes[i] > 0).max_by_key(|&i| self.votes[i])
    }
}

/// Highest level a monster can plausibly have in a fight (scaled ones included): the 99th percentile of the grade levels
/// + margin (a few special entities go up to 2400, which would let most gids pass for a level).
fn max_level(data: &Data) -> i64 {
    static TOP: OnceLock<i64> = OnceLock::new();
    *TOP.get_or_init(|| {
        let mut l: Vec<i64> = data.monsters.values().flat_map(|m| m.levels.iter().copied()).collect();
        l.sort_unstable();
        l.get(l.len() * 99 / 100).copied().unwrap_or(200) + 100
    })
}

/// `p` makes sense for these ints: the gid is a monster, the grade one of its grades and the level plausible.
fn fits(data: &Data, ints: &[i64; 3], p: usize, top: i64) -> bool {
    let [g, gr, l] = PERMS[p].map(|i| ints[i]);
    data.monsters.get(&g).is_some_and(|m| gr >= 1 && gr as usize <= m.levels.len()) && (1..=top).contains(&l)
}

/// The arrangements that fit every one of these fighters, the most exact first (level = the grade's level).
fn fitting(data: &Data, all: &[[i64; 3]]) -> Vec<usize> {
    let top = max_level(data);
    let exact = |p: usize| {
        all.iter()
            .filter(|x| {
                let [g, gr, l] = PERMS[p].map(|i| x[i]);
                data.monsters.get(&g).and_then(|m| m.levels.get((gr - 1) as usize)) == Some(&l)
            })
            .count()
    };
    let mut ok: Vec<usize> = (0..6).filter(|&p| !all.is_empty() && all.iter().all(|x| fits(data, x, p, top))).collect();
    ok.sort_by_key(|&p| std::cmp::Reverse(exact(p)));
    ok
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
    /// actor ids of the monsters that died (death action, or left out of a synchronisation)
    dead: BTreeSet<i64>,
}

pub struct Combat {
    bind: OnceLock<Option<Bindings>>,
    enabled: AtomicBool,
    fight: Mutex<Fight>,
    layout: Mutex<Layout>,
}

impl Default for Combat {
    fn default() -> Self {
        Self::new()
    }
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
fn fighters(b: &Bindings, m: &DynamicMessage, out: &mut Vec<(i64, Fighter)>) {
    if m.descriptor().get_field_by_name(&b.actor_id).is_none() {
        for c in children(m) {
            fighters(b, &c, out);
        }
        return;
    }
    let Some(mon) = b.monster.first().and_then(|f| find(m, f)) else { return };
    let [Some(x), Some(y), Some(z)] = [0, 1, 2].map(|i| b.monster.get(i).map(|f| int(&mon, f))) else { return };
    let summoned = find(m, &b.summoned).and_then(|s| s.get_field_by_name(&b.summoned).and_then(|v| as_i64(&v))).unwrap_or(0) != 0;
    out.push((int(m, &b.actor_id), Fighter { ints: [x, y, z], summoned }));
}

impl Combat {
    pub fn new() -> Self {
        Self { bind: OnceLock::new(), enabled: AtomicBool::new(true), fight: Mutex::new(Fight::default()), layout: Default::default() }
    }

    fn bindings(&self, ctx: &Ctx) -> Option<&Bindings> {
        self.bind.get_or_init(|| serde_json::from_value(ctx.data.bindings.get("fight")?.clone()).ok()).as_ref()
    }

    /// A message's fighters fit a single arrangement: one vote for it (saved).
    fn vote(&self, ctx: &Ctx, found: &[(i64, Fighter)]) {
        let ok = fitting(&ctx.data, &found.iter().map(|(_, x)| x.ints).collect::<Vec<_>>());
        if ok.len() != 1 {
            return;
        }
        let mut l = self.layout.lock();
        let before = l.best();
        l.votes[ok[0]] += 1;
        if before != l.best() && l.fields.len() == 3 {
            ctx.log(format!("combat: monster fields (gid, grade, level) = {:?}", PERMS[ok[0]].map(|i| l.fields[i].clone())));
        }
        let _ = save_json(&ctx.module_dir("combat").join("layout.json"), &json!({"fields": l.fields, "votes": l.votes}));
    }

    /// The arrangement to read the fighters with: the learnt one, else the best fitting them all.
    fn arrangement(&self, data: &Data, f: &Fight) -> Option<usize> {
        self.layout.lock().best().or_else(|| fitting(data, &f.fighters.values().map(|x| x.ints).collect::<Vec<_>>()).first().copied())
    }

    /// The fight for the UI: {active, started, ended, fighters: [{actor, gid, name, grade, level, summoned, notes, tags}]}.
    /// Without any arrangement (nothing fits them all), a fighter's gid is its int that is a monster (grade / level 0).
    fn state(&self, data: &Data) -> Value {
        let f = self.fight.lock();
        let p = self.arrangement(data, &f);
        let list: Vec<Value> = f
            .fighters
            .iter()
            .map(|(actor, x)| {
                let [gid, grade, level] = match p {
                    Some(p) => PERMS[p].map(|i| x.ints[i]),
                    None => [x.ints.iter().copied().filter(|g| data.monsters.contains_key(g)).max().unwrap_or(x.ints[0]), 0, 0],
                };
                let m = data.monsters.get(&gid);
                json!({
                    "actor": actor, "gid": gid, "grade": grade, "level": level, "summoned": x.summoned, "dead": f.dead.contains(actor),
                    "name": m.and_then(|m| data.text(m.name_id)).map(str::to_owned).unwrap_or_else(|| format!("monstre #{gid}")),
                    "notes": m.and_then(|m| m.notes.as_ref()),
                    "tags": m.map(|m| m.tags.as_slice()).unwrap_or_default(),
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

    fn init(&self, ctx: &Ctx) {
        let Some(b) = self.bindings(ctx) else { return };
        let mut l = self.layout.lock();
        l.fields = b.monster.clone();
        // the votes of another build (other field names) are dropped
        if let Some(saved) = load_json(&ctx.module_dir("combat").join("layout.json")) {
            if saved["fields"] == json!(b.monster) {
                if let Ok(v) = serde_json::from_value::<[u32; 6]>(saved["votes"].clone()) {
                    l.votes = v;
                }
            }
        }
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
                    // a death action: its ids (target, and an unnamed one) that are fighters of this fight
                    let died: Vec<i64> = match (&b.death, m.name == ACTION) {
                        (Some(death), true) if d.has_field_by_name(death) => match d.get_field_by_name(death).as_deref() {
                            Some(PbValue::Message(x)) => x.fields().filter_map(|(_, v)| as_i64(v)).filter(|id| f.fighters.contains_key(id)).collect(),
                            _ => vec![],
                        },
                        _ => vec![],
                    };
                    if !died.is_empty() {
                        let before = f.dead.len();
                        f.dead.extend(died);
                        f.dead.len() != before
                    } else {
                        let mut found = vec![];
                        fighters(b, d, &mut found);
                        if found.is_empty() {
                            return;
                        }
                        self.vote(ctx, &found);
                        // fighters without a fight start (app opened mid-fight) or after its end: a new fight
                        if !f.seen || f.ended.is_some() {
                            *f = Fight { seen: true, started: t, ..Default::default() };
                        }
                        // the synchronisation lists the fighters still in: the monsters it leaves out are dead (kept, flagged)
                        let mut dead = f.dead.clone();
                        if m.name == SYNC {
                            let listed: BTreeSet<i64> = found.iter().map(|(a, _)| *a).collect();
                            dead.extend(f.fighters.keys().filter(|a| !listed.contains(a)));
                        }
                        let mut next = f.fighters.clone();
                        next.extend(found);
                        let changed = next != f.fighters || dead != f.dead;
                        (f.fighters, f.dead) = (next, dead);
                        changed
                    }
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

    /// A real fight (Pissenlit Diabolique 79, Minotoror 121, Wobot Kiafin 3462, Meupette 1046) with every monster scaled to
    /// level 200: no grade level matches, and taking the largest monster int as the gid read 79 and 121 as Dragodinde rousse
    /// sauvage (200 = their level). Only one arrangement fits the four; it is learnt and then reads a fight of the small ids alone.
    #[test]
    fn learns_the_monster_fields_from_a_scaled_fight() {
        let pack = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../datapack");
        if !pack.join("manifest.json").exists() {
            return;
        }
        let data = Arc::new(Data::load(&pack, "fr").unwrap());
        if [79, 121, 3462, 1046, 200].iter().any(|g| !data.monsters.contains_key(g)) {
            eprintln!("skipped: pack without these monsters");
            return;
        }
        let dir = std::env::temp_dir().join("loutil-combat-layout");
        let _ = std::fs::remove_dir_all(&dir);
        let proto = Arc::new(Proto::load(&pack, dir.join("o.json")).unwrap());
        let hub = crate::Hub::new(data.clone(), proto, dir, Arc::new(|_, _| {}), Arc::new(|_| {}));
        // field order of this imaginary build: (grade, level, gid)
        let fighter = |gid: i64, grade: i64| Fighter { ints: [grade, 200, gid], summoned: false };
        let c = Combat::new();
        let found: Vec<(i64, Fighter)> = [(79, 3), (121, 2), (3462, 5), (1046, 1)].iter().enumerate().map(|(i, &(g, gr))| (-(i as i64) - 1, fighter(g, gr))).collect();
        c.vote(&hub.ctx, &found);
        *c.fight.lock() = Fight { seen: true, fighters: found.into_iter().collect(), ..Default::default() };
        let s = c.call("state", Value::Null, &hub.ctx).unwrap();
        let gids: Vec<i64> = s["fighters"].as_array().unwrap().iter().map(|f| f["gid"].as_i64().unwrap()).collect();
        assert_eq!(gids, vec![1046, 3462, 121, 79], "actors -4..-1");
        assert!(s["fighters"].as_array().unwrap().iter().all(|f| f["level"] == json!(200)));

        // alone, 79 and 121 fit two arrangements (200 is a monster too): the learnt one decides
        let small = vec![(-1, fighter(79, 3)), (-2, fighter(121, 2))];
        c.vote(&hub.ctx, &small);
        *c.fight.lock() = Fight { seen: true, fighters: small.into_iter().collect(), ..Default::default() };
        let s = c.call("state", Value::Null, &hub.ctx).unwrap();
        assert_eq!(s["fighters"][0]["gid"], json!(121));
        assert_eq!(s["fighters"][1]["gid"], json!(79));
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
        let proto_action = proto.message(&proto.obf_for_clear(ACTION).unwrap()).unwrap();
        let mut sync = DynamicMessage::new(sync_md);
        sync.set_field(&list_field, PbValue::List(vec![PbValue::Message(actor(-1, gid, false)), PbValue::Message(actor(-2, gid, true))]));
        let mut sync_one = DynamicMessage::new(sync.descriptor());
        sync_one.set_field(&list_field, PbValue::List(vec![PbValue::Message(actor(-2, gid, true))]));
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

        // the summon dies (death action: its id in one of the two ints), then a synchronisation leaves the other one out
        let action_md = proto_action.clone();
        let death_field = action_md.get_field_by_name(b.death.as_deref().unwrap()).unwrap();
        let death_md = death_field.kind().as_message().unwrap().clone();
        let mut death = DynamicMessage::new(death_md.clone());
        for (f, v) in death_md.fields().zip([-2i64, 7]) {
            death.set_field(&f, PbValue::I64(v));
        }
        let mut action = DynamicMessage::new(action_md);
        action.set_field(&death_field, PbValue::Message(death));
        c.on_message(&message(ACTION, action, 2.5), &hub.ctx);
        let s = c.call("state", Value::Null, &hub.ctx).unwrap();
        assert_eq!((s["fighters"][0]["dead"].clone(), s["fighters"][1]["dead"].clone()), (json!(true), json!(false)), "actor -2 died");
        c.on_message(&message(SYNC, sync_one, 2.6), &hub.ctx);
        let s = c.call("state", Value::Null, &hub.ctx).unwrap();
        assert_eq!(s["fighters"].as_array().unwrap().len(), 2, "dead monsters stay listed (flagged)");
        assert_eq!(s["fighters"][1]["dead"], json!(true), "actor -1, left out of the synchronisation = dead");

        c.on_message(&message(END, end, 3.0), &hub.ctx);
        let s = c.call("state", Value::Null, &hub.ctx).unwrap();
        assert_eq!((s["active"].clone(), s["ended"].clone()), (json!(false), json!(3.0)), "the last fight stays shown, ended");
    }
}
