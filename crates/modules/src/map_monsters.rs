//! Map monsters: the monster groups of the map the player is on, sent to the UI for the Automation tab (Monstres: a sound
//! when a searched monster is on the map). `monsters:map` {t, map, gids} on every map change (MapComplementaryInformationEvent,
//! every actor of the new map), `monsters:spawn` {t, gids} when groups appear on the current map (GameRolePlayShowActorsEvent).
//! Both also carry `npcs`: the NPC actors (NpcStaticInformation) of the message, by pack NPC id (Songes goblins: King Gob...).
//!
//! A group is an ActorPositionInformation -> RolePlayActor -> MonsterGroupActor -> MonsterGroupStaticInformation {main_creature,
//! underlings, alternatives (other compositions by party size)}, each creature a MonsterInGroupInformation (named by the script
//! table) whose gid / grade / level are three unnamed ints reshuffled per build: the gid is read from the arrangement that
//! makes sense (a monster, one of its grades, a plausible level; the grade's exact level first), like the combat module.
use crate::combat::{fits, max_level, PERMS};
use crate::kit::unknown;
use crate::{Ctx, Interest, Message, Module};
use anyhow::Result;
use loutil_data::Data;
use loutil_proto::util::scalar_ints;
use loutil_proto::{DynamicMessage, PbValue, ReflectMessage};
use serde_json::{json, Value};
use std::collections::BTreeSet;

/// The player arrives on a map: every actor of it.
pub const MAP: &str = "MapComplementaryInformationEvent";
/// Actors appearing on the current map (a group spawning, a player arriving).
pub const SHOW: &str = "GameRolePlayShowActorsEvent";
/// Script-table name of a creature of a group (stable across builds).
const CREATURE: &str = "MonsterInGroupInformation";
/// Script-table name of an NPC actor: npc id / special artwork id are unnamed ints, the npc id is the one that is a pack NPC.
const NPC: &str = "NpcStaticInformation";

/// Monster gids and NPC ids found in a message.
#[derive(Default)]
struct Found {
    gids: BTreeSet<i64>,
    npcs: BTreeSet<i64>,
}

#[derive(Default)]
pub struct MapMonsters;

impl MapMonsters {
    pub fn new() -> Self {
        Self
    }

    /// The gid of a creature's three ints: the most exact arrangement that fits, else the largest int that is a monster.
    fn gid(data: &Data, ints: &[i64]) -> Option<i64> {
        let ints: [i64; 3] = ints.try_into().ok()?;
        let top = max_level(data);
        let exact = |p: usize| {
            let [g, gr, l] = PERMS[p].map(|i| ints[i]);
            data.monsters.get(&g).and_then(|m| m.levels.get((gr - 1) as usize)) == Some(&l)
        };
        let ok: Vec<usize> = (0..6).filter(|&p| fits(data, &ints, p, top)).collect();
        match ok.iter().find(|&&p| exact(p)).or(ok.first()) {
            Some(&p) => Some(ints[PERMS[p][0]]),
            None => ints.iter().copied().filter(|g| data.monsters.contains_key(g)).max(),
        }
    }

    /// Every creature gid under `m` (each group's main creature, underlings and alternatives) and every NPC, whatever the
    /// fields leading there.
    fn walk(ctx: &Ctx, m: &DynamicMessage, out: &mut Found) {
        let name = ctx.proto.message_name(m.descriptor().full_name()).0;
        if name == CREATURE {
            let ints: Vec<i64> = scalar_ints(m).into_iter().map(|(_, v)| v).collect();
            out.gids.extend(Self::gid(&ctx.data, &ints));
            return;
        }
        if name == NPC {
            out.npcs.extend(scalar_ints(m).into_iter().map(|(_, v)| v).find(|v| *v != 0 && ctx.data.npcs.contains_key(v)));
            return;
        }
        for (_, v) in m.fields() {
            match v {
                PbValue::Message(x) => Self::walk(ctx, x, out),
                PbValue::List(l) => l.iter().filter_map(PbValue::as_message).for_each(|x| Self::walk(ctx, x, out)),
                _ => {}
            }
        }
    }

    /// The monster gids and NPC ids of a message, sorted, each once. Only the `actors` field when it is named (map: skips the
    /// interactive elements, houses...).
    fn actors(ctx: &Ctx, msg: &DynamicMessage) -> (Vec<i64>, Vec<i64>) {
        let mut out = Found::default();
        match ctx.proto.field_named(msg.descriptor().full_name(), "actors").and_then(|f| msg.get_field_by_name(&f)) {
            Some(v) => match v.as_ref() {
                PbValue::List(l) => l.iter().filter_map(PbValue::as_message).for_each(|x| Self::walk(ctx, x, &mut out)),
                _ => Self::walk(ctx, msg, &mut out),
            },
            None => Self::walk(ctx, msg, &mut out),
        }
        (out.gids.into_iter().collect(), out.npcs.into_iter().collect())
    }

    #[cfg(test)]
    fn monsters(ctx: &Ctx, msg: &DynamicMessage) -> Vec<i64> {
        Self::actors(ctx, msg).0
    }
}

impl Module for MapMonsters {
    fn id(&self) -> &'static str {
        "monsters"
    }

    fn interest(&self) -> Interest {
        Interest::Types(&[MAP, SHOW])
    }

    fn on_message(&self, m: &Message, ctx: &Ctx) {
        let Some(msg) = m.decoded.as_ref() else { return };
        let (gids, npcs) = Self::actors(ctx, msg);
        if m.name == MAP {
            let map = ctx.proto.field_named(msg.descriptor().full_name(), "map_id").map(|f| loutil_proto::util::int(msg, &f));
            ctx.emit("monsters", "map", json!({"t": m.raw.t, "map": map, "gids": gids, "npcs": npcs}));
        } else if !gids.is_empty() || !npcs.is_empty() {
            ctx.emit("monsters", "spawn", json!({"t": m.raw.t, "gids": gids, "npcs": npcs}));
        }
    }

    /// `list`: every monster of the pack [{gid, name}] (Automation search suggestions); `npcs`: every NPC [{id, name}].
    fn call(&self, method: &str, _args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            "list" => {
                let d = &ctx.data;
                let l: Vec<Value> = d.monsters.iter().filter_map(|(g, m)| Some(json!({"gid": g, "name": d.text(m.name_id)?}))).collect();
                Ok(Value::Array(l))
            }
            "npcs" => {
                let d = &ctx.data;
                Ok(Value::Array(d.npcs.iter().filter_map(|(id, n)| Some(json!({"id": id, "name": d.text(n.name_id)?}))).collect()))
            }
            _ => Err(unknown("monsters", method)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::testkit;
    use loutil_proto::{FieldDescriptor, MessageDescriptor, Proto};

    /// A real Songe room (Paquets tab export of MapComplementaryInformationEvent, work/capture_kinggob_map.json: local only, it
    /// holds players' names) replayed through a hub: King Gob (NPC 7850) and the room's group are found. Skipped without it.
    #[test]
    fn finds_king_gob_in_a_recorded_songe_room() {
        let Some(pack) = testkit::pack() else { return };
        let rec = pack.join("../work/capture_kinggob_map.json");
        let Ok(text) = std::fs::read_to_string(&rec) else { return };
        let r: Value = serde_json::from_str(&text).unwrap();
        let h = r["hex"].as_str().unwrap();
        let payload: Vec<u8> = (0..h.len() / 2).map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap()).collect();
        let data = std::sync::Arc::new(Data::load(&pack, "fr").unwrap());
        let dir = testkit::temp_dir("loutil-map-npcs");
        let proto = std::sync::Arc::new(Proto::load(&pack, dir.join("o.json")).unwrap());
        let seen = std::sync::Arc::new(parking_lot::Mutex::new(vec![]));
        let s2 = seen.clone();
        let hub =
            crate::Hub::new(data, proto, dir, std::sync::Arc::new(move |ev: &str, v: Value| s2.lock().push((ev.to_string(), v))), std::sync::Arc::new(|_| {}));
        hub.handle(loutil_core::RawMessage { t: 1.0, srv: "x".into(), dir: "S->C", payload });
        let seen = seen.lock();
        let (_, v) = seen.iter().find(|(e, _)| e == "monsters:map").expect("a monsters:map event");
        assert_eq!(v["npcs"], json!([7850]), "King Gob");
        assert_eq!(v["gids"].as_array().unwrap().len(), 8, "Grand Pa Wabbit and its 7 underlings");
    }

    /// The field path from `m` down to a message named `target` (script table), at most 6 levels deep.
    fn path(proto: &Proto, m: &MessageDescriptor, target: &str, depth: usize) -> Option<Vec<FieldDescriptor>> {
        if depth > 6 {
            return None;
        }
        for f in m.fields() {
            let Some(k) = f.kind().as_message().cloned() else { continue };
            if proto.message_name(k.full_name()).0 == target {
                return Some(vec![f]);
            }
            if let Some(mut rest) = path(proto, &k, target, depth + 1) {
                rest.insert(0, f);
                return Some(rest);
            }
        }
        None
    }

    /// A creature message with this gid / grade / level put in scrambled order.
    fn creature(data: &Data, md: &MessageDescriptor, gid: i64) -> DynamicMessage {
        let ints: Vec<String> = scalar_ints(&DynamicMessage::new(md.clone())).into_iter().map(|(f, _)| f).collect();
        let level = data.monsters[&gid].levels[1];
        let mut c = DynamicMessage::new(md.clone());
        for (f, v) in ints.iter().zip([level, gid, 2]) {
            c.set_field_by_name(f, PbValue::I32(v as i32));
        }
        c
    }

    /// A synthetic map: one group (main creature + an underling) and one actor that is not a group; then a group spawning.
    #[test]
    fn reads_the_monster_groups_of_a_map() {
        let Some((hub, proto)) = testkit::hub("loutil-map-monsters") else { return };
        let data = hub.ctx.data.clone();
        let two: Vec<i64> = {
            let mut g: Vec<i64> = data.monsters.iter().filter(|(_, m)| m.levels.len() >= 2).map(|(g, _)| *g).collect();
            g.sort_unstable();
            g.into_iter().filter(|g| *g > 300).take(2).collect()
        };
        if two.len() < 2 {
            eprintln!("skipped: pack without monsters");
            return;
        }
        let md = proto.message(&proto.obf_for_clear(MAP).unwrap()).unwrap();
        let actors_f = md.get_field_by_name(&proto.field_named(md.full_name(), "actors").unwrap()).unwrap();
        let actor_md = actors_f.kind().as_message().unwrap().clone();
        let static_md = proto.message(&proto.obf_for_clear("MonsterGroupStaticInformation").unwrap()).unwrap();
        let named = |m: &MessageDescriptor, clear: &str| m.get_field_by_name(&proto.field_named(m.full_name(), clear).unwrap()).unwrap();
        let (main_f, under_f) = (named(&static_md, "main_creature"), named(&static_md, "underlings"));
        let creature_md = main_f.kind().as_message().unwrap().clone();
        let mut group = DynamicMessage::new(static_md.clone());
        group.set_field(&main_f, PbValue::Message(creature(&data, &creature_md, two[0])));
        group.set_field(&under_f, PbValue::List(vec![PbValue::Message(creature(&data, &creature_md, two[1]))]));
        // actor -> ... -> MonsterGroupStaticInformation, built from the innermost message out
        let p = path(&proto, &actor_md, "MonsterGroupStaticInformation", 0).expect("a group under ActorPositionInformation");
        let mut value = PbValue::Message(group);
        for f in p.iter().rev() {
            let v = if f.is_list() { PbValue::List(vec![value]) } else { value };
            let mut parent = DynamicMessage::new(f.parent_message().clone());
            parent.set_field(f, v);
            value = PbValue::Message(parent);
        }
        let mut msg = DynamicMessage::new(md);
        msg.set_field(&actors_f, PbValue::List(vec![value.clone(), PbValue::Message(DynamicMessage::new(actor_md.clone()))]));
        assert_eq!(MapMonsters::monsters(&hub.ctx, &msg), two);

        let show_md = proto.message(&proto.obf_for_clear(SHOW).unwrap()).unwrap();
        let list_f = show_md.fields().find(|f| f.is_list() && f.kind().as_message().is_some_and(|k| k.full_name() == actor_md.full_name())).unwrap();
        let mut show = DynamicMessage::new(show_md);
        show.set_field(&list_f, PbValue::List(vec![value]));
        assert_eq!(MapMonsters::monsters(&hub.ctx, &show), two);
    }
}
