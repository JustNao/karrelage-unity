//! Loot: what each fighter got at the end of a fight (FightEndEvent), sent to the UI as `loot:fight` {t, items: [{fighter,
//! gid, name, type_id}]} for the Automation tab (Songes: a « Légende » looted by someone of the group).
//!
//! FightEndEvent -> results (FightResultListEntry: fighter_list_entry.fighter_id, rewards -> FightLoot -> [Loot]). `results`,
//! `fighter_list_entry` and `fighter_id` have exact clear names and `Loot` is named by the script table, but Loot's three
//! ints (item id, quantity, priority hint) are unnamed and reshuffled per build: the item is the largest of them that is a
//! known item gid (a quantity or a hint never reaches the gids of the items looked for).
use crate::kit::unknown;
use crate::{Ctx, Interest, Message, Module};
use anyhow::Result;
use loutil_proto::util::{int, messages, scalar_ints};
use loutil_proto::{DynamicMessage, PbValue, ReflectMessage};
use serde_json::{json, Value};

pub const END: &str = crate::combat::END;
/// Script-table name of a looted object (stable across builds).
const LOOT: &str = "Loot";

#[derive(Default)]
pub struct Loot;

impl Loot {
    pub fn new() -> Self {
        Self
    }

    /// Every `Loot` message under `m` (rewards -> FightLoot lists), whatever the unnamed fields leading to them.
    fn loots(ctx: &Ctx, m: &DynamicMessage, out: &mut Vec<DynamicMessage>) {
        if ctx.proto.message_name(m.descriptor().full_name()).0 == LOOT {
            out.push(m.clone());
            return;
        }
        for (_, v) in m.fields() {
            match v {
                PbValue::Message(x) => Self::loots(ctx, x, out),
                PbValue::List(l) => l.iter().filter_map(PbValue::as_message).for_each(|x| Self::loots(ctx, x, out)),
                _ => {}
            }
        }
    }

    /// FightEndEvent -> [{fighter, gid, name, type_id}].
    fn items(ctx: &Ctx, msg: &DynamicMessage) -> Vec<Value> {
        let field = |m: &DynamicMessage, clear: &str| ctx.proto.field_named(m.descriptor().full_name(), clear);
        let Some(results) = field(msg, "results") else { return vec![] };
        let mut out = vec![];
        for entry in messages(msg, &results) {
            let fighter = field(&entry, "fighter_list_entry")
                .and_then(|f| entry.get_field_by_name(&f)?.as_message().cloned())
                .and_then(|f| Some(int(&f, &field(&f, "fighter_id")?)));
            let mut loots = vec![];
            Self::loots(ctx, &entry, &mut loots);
            for l in loots {
                let Some(gid) = scalar_ints(&l).into_iter().map(|(_, v)| v).filter(|v| ctx.data.items.contains_key(v)).max() else { continue };
                let type_id = ctx.data.items.get(&gid).map(|i| i.type_id);
                out.push(json!({"fighter": fighter, "gid": gid, "name": ctx.data.item_name(gid), "type_id": type_id}));
            }
        }
        out
    }
}

impl Module for Loot {
    fn id(&self) -> &'static str {
        "loot"
    }

    fn interest(&self) -> Interest {
        Interest::Types(&[END])
    }

    fn on_message(&self, m: &Message, ctx: &Ctx) {
        let Some(msg) = m.decoded.as_ref() else { return };
        let items = Self::items(ctx, msg);
        if !items.is_empty() {
            ctx.emit("loot", "fight", json!({"t": m.raw.t, "items": items}));
        }
    }

    fn call(&self, method: &str, _args: Value, _ctx: &Ctx) -> Result<Value> {
        Err(unknown("loot", method))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::testkit;

    /// A synthetic FightEndEvent: a player looting 3 of a « Légende » (Ressource des Songes), the gid and the quantity put in
    /// whichever ints of the build's Loot.
    #[test]
    fn reads_the_loot_of_each_fighter() {
        let Some((hub, proto)) = testkit::hub("loutil-loot") else { return };
        let data = hub.ctx.data.clone();
        let legend = *data.items.keys().find(|g| data.items[g].type_id == 219 && data.item_name(**g).starts_with("Légende")).unwrap();
        let md = proto.message(&proto.obf_for_clear(END).unwrap()).unwrap();
        let named = |m: &loutil_proto::MessageDescriptor, clear: &str| proto.field_named(m.full_name(), clear).unwrap_or_else(|| panic!("{clear}"));
        let sub = |m: &loutil_proto::MessageDescriptor, f: &str| m.get_field_by_name(f).unwrap().kind().as_message().unwrap().clone();
        let results_f = named(&md, "results");
        let entry_md = sub(&md, &results_f);
        let fle_f = named(&entry_md, "fighter_list_entry");
        let fle_md = sub(&entry_md, &fle_f);
        let fid_f = named(&fle_md, "fighter_id");
        // the path to Loot is unnamed: find it in the descriptors (entry -> rewards -> FightLoot -> [Loot])
        fn path(proto: &loutil_proto::Proto, m: &loutil_proto::MessageDescriptor, depth: usize) -> Option<Vec<loutil_proto::FieldDescriptor>> {
            if depth > 4 {
                return None;
            }
            for f in m.fields() {
                let Some(k) = f.kind().as_message().cloned() else { continue };
                if proto.message_name(k.full_name()).0 == LOOT {
                    return Some(vec![f]);
                }
                if let Some(mut rest) = path(proto, &k, depth + 1) {
                    rest.insert(0, f);
                    return Some(rest);
                }
            }
            None
        }
        let p = path(&proto, &entry_md, 0).expect("a Loot under FightResultListEntry");
        let loot_md = p.last().unwrap().kind().as_message().unwrap().clone();
        let ints: Vec<String> = scalar_ints(&DynamicMessage::new(loot_md.clone())).into_iter().map(|(f, _)| f).collect();
        let mut loot = DynamicMessage::new(loot_md);
        loot.set_field_by_name(&ints[ints.len() - 1], PbValue::I32(legend as i32));
        loot.set_field_by_name(&ints[0], PbValue::I32(3)); // a quantity
                                                           // build entry -> ... -> Loot from the innermost message out
        let mut value = PbValue::Message(loot);
        for f in p.iter().rev() {
            let v = if f.is_list() { PbValue::List(vec![value]) } else { value };
            let mut parent = DynamicMessage::new(f.parent_message().clone());
            parent.set_field(f, v);
            value = PbValue::Message(parent);
        }
        let PbValue::Message(mut entry) = value else { unreachable!() };
        let mut fle = DynamicMessage::new(fle_md);
        fle.set_field_by_name(&fid_f, PbValue::I64(42));
        entry.set_field_by_name(&fle_f, PbValue::Message(fle));
        let mut msg = DynamicMessage::new(md);
        msg.set_field_by_name(&results_f, PbValue::List(vec![PbValue::Message(entry)]));
        let items = Loot::items(&hub.ctx, &msg);
        assert_eq!(items, vec![json!({"fighter": 42, "gid": legend, "name": data.item_name(legend), "type_id": 219})]);
    }

    /// A FightEndEvent captured on Dofus 3.7.12.12 (FightEndEvent = `jwp`; skipped on other builds, whose schema differs):
    /// the player looted 1 Plume de Piou Bleu (6897), the monster nothing.
    #[test]
    fn reads_a_captured_fight_end() {
        let Some((hub, proto)) = testkit::hub("loutil-loot-capture") else { return };
        if proto.obf_for_clear(END).as_deref() != Some("jwp") {
            return;
        }
        const FRAME: &str = "1a7f0a7d0a13747970652e616e6b616d612e636f6d2f6a7770126622430802122f10011a250a200a1e08011001189bbac6f5392095dea7dc39280130b9e2c5a33a38014077480110b60228a782b4a6491a0e0a0a1a05080118f13520eb0310052211120d100128ffffffffffffffffff011a0028ffffffffffffffffff0140e11e";
        let bytes: Vec<u8> = (0..FRAME.len()).step_by(2).map(|i| u8::from_str_radix(&FRAME[i..i + 2], 16).unwrap()).collect();
        // event (1a 7f) -> Any (0a 7d) -> type url (0a 13 + 19 bytes) -> value (12 66 + 102 bytes)
        let msg = proto.decode("jwp", &bytes[27..]).expect("decodes");
        let items = Loot::items(&hub.ctx, &msg);
        let ty = hub.ctx.data.items[&6897].type_id;
        assert_eq!(items, vec![json!({"fighter": 19676332327i64, "gid": 6897, "name": "Plume de Piou Bleu", "type_id": ty})]);
    }
}
