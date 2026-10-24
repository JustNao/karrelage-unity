//! Map monsters: the monster groups of the map the player is on, sent to the UI for the Automation tab (Monstres: a sound
//! when a searched monster is on the map). `monsters:map` {t, map, gids} on every map change (MapComplementaryInformationEvent,
//! every actor of the new map), `monsters:spawn` {t, gids} when groups appear on the current map (GameRolePlayShowActorsEvent).
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

    /// Every creature gid under `m` (each group's main creature, underlings and alternatives), whatever the fields leading there.
    fn walk(ctx: &Ctx, m: &DynamicMessage, out: &mut BTreeSet<i64>) {
        if ctx.proto.message_name(m.descriptor().full_name()).0 == CREATURE {
            let ints: Vec<i64> = scalar_ints(m).into_iter().map(|(_, v)| v).collect();
            out.extend(Self::gid(&ctx.data, &ints));
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

    /// The monster gids of a message, sorted, each once. Only the `actors` field when it is named (map: skips the
    /// interactive elements, houses...).
    fn monsters(ctx: &Ctx, msg: &DynamicMessage) -> Vec<i64> {
        let mut out = BTreeSet::new();
        match ctx.proto.field_named(msg.descriptor().full_name(), "actors").and_then(|f| msg.get_field_by_name(&f)) {
            Some(v) => match v.as_ref() {
                PbValue::List(l) => l.iter().filter_map(PbValue::as_message).for_each(|x| Self::walk(ctx, x, &mut out)),
                _ => Self::walk(ctx, msg, &mut out),
            },
            None => Self::walk(ctx, msg, &mut out),
        }
        out.into_iter().collect()
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
        let gids = Self::monsters(ctx, msg);
        if m.name == MAP {
            let map = ctx.proto.field_named(msg.descriptor().full_name(), "map_id").map(|f| loutil_proto::util::int(msg, &f));
            ctx.emit("monsters", "map", json!({"t": m.raw.t, "map": map, "gids": gids}));
        } else if !gids.is_empty() {
            ctx.emit("monsters", "spawn", json!({"t": m.raw.t, "gids": gids}));
        }
    }

    /// `list`: every monster of the pack [{gid, name}] (Automation search suggestions).
    fn call(&self, method: &str, _args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            "list" => {
                let d = &ctx.data;
                let l: Vec<Value> = d.monsters.iter().filter_map(|(g, m)| Some(json!({"gid": g, "name": d.text(m.name_id)?}))).collect();
                Ok(Value::Array(l))
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
