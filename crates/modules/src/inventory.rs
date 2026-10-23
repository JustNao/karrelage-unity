//! Inventory: the character's equipment (worn and in the bag) with its real effects, read from the inventory sent at login
//! and kept up to date by the object added / modified / moved / deleted events (equipping = moving an object to a worn
//! position). The Stuff tab uses it to count an owned item at its actual roll instead of the perfect one, and to export the
//! worn equipment with the character's base points and scrolls (CharacterCharacteristicsEvent). Only equipment is kept; the
//! last character seen wins.
use crate::auction::{parse_effect, stat_effects, ObjectEffect};
use crate::kit::{unknown, Role, Store};
use crate::{Ctx, Interest, Message, Module};
use anyhow::Result;
use loutil_proto::util::{int, int_list, messages};
use loutil_proto::DynamicMessage;
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

pub const CONTENT: &str = "InventoryContentEvent";
pub const ADDED: &str = "ObjectAddedEvent";
pub const ADDED_LIST: &str = "ObjectsAddedEvent";
pub const MODIFIED: &str = "ObjectModifiedEvent";
pub const DELETED: &str = "ObjectDeletedEvent";
pub const DELETED_LIST: &str = "ObjectsDeletedEvent";
pub const MOVED: &str = "ObjectMovementEvent";
pub const CHARACTERISTICS: &str = "CharacterCharacteristicsEvent";
/// Characteristics kept for the export: vitality, wisdom, strength, intelligence, chance, agility.
const KEPT_CHARACTERISTICS: [i64; 6] = [11, 12, 10, 15, 13, 14];

/// `position` of an object that is in the bag (0-15: worn).
const BAG: i64 = 63;

/// Obfuscated field names of the inventory messages (bindings.json `inventory`).
#[derive(Deserialize, Clone)]
pub(crate) struct Bindings {
    pub objects: String,
    pub item: String,
    pub position: String,
    pub gid: String,
    pub uid: String,
    pub quantity: String,
    pub effects: String,
    pub added: String,
    pub added_list: String,
    pub modified: String,
    pub deleted: String,
    pub deleted_list: String,
}

/// ObjectMovementEvent fields (bindings.json `inventory_move`; a role of its own so that an older pack keeps the inventory).
#[derive(Deserialize, Clone)]
pub(crate) struct MoveBindings {
    pub uid: String,
    pub position: String,
}

/// CharacterCharacteristicsEvent path to each characteristic's base points and scrolls (bindings.json `characteristics`).
#[derive(Deserialize, Clone)]
pub(crate) struct CharacBindings {
    pub stats: String,
    pub list: String,
    pub id: String,
    pub detailed: String,
    pub base: String,
    pub additional: String,
}

/// An equipment piece as last seen in the inventory: its gid, stack size and effect values ([action, value]).
#[derive(Clone, Debug, Default)]
pub struct Seen {
    pub gid: i64,
    pub quantity: i64,
    pub effects: Vec<(i64, i64)>,
}

impl Seen {
    fn of(it: &Value) -> Option<Self> {
        let effects = it["effects"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|e| e["status"] != "removed")
            .filter_map(|e| Some((e["action"].as_i64()?, e["hi"].as_i64()?)))
            .collect();
        Some(Seen { gid: it["gid"].as_i64()?, quantity: it["quantity"].as_i64().unwrap_or(1), effects })
    }
}

/// Object uid -> the equipment piece, for every piece seen (loaded or captured), kept after its deletion: the Brisage
/// module resolves the uids of broken items with it (the item is gone from the inventory by then).
pub type UidMap = Arc<Mutex<HashMap<i64, Seen>>>;

/// What `items.json` holds: when the game last sent the whole inventory (login) and the equipment, by object uid.
#[derive(Default)]
struct Content {
    updated: f64,
    /// object uid -> item (see `Inventory::item`)
    items: BTreeMap<i64, Value>,
    /// the character's characteristics: {t, carac: {characteristic id: {base, scroll}}} (null = never seen)
    character: Value,
}

impl Content {
    fn json(&self) -> Value {
        json!({"updated": self.updated, "items": self.items.values().collect::<Vec<_>>(), "character": self.character})
    }
}

#[derive(Default)]
pub struct Inventory {
    bind: Role<Bindings>,
    effect_bind: Role<ObjectEffect>,
    move_bind: Role<MoveBindings>,
    carac_bind: Role<CharacBindings>,
    store: Store<Content>,
    uids: UidMap,
}

impl Inventory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Shared uid -> item map (see `UidMap`).
    pub fn uids(&self) -> UidMap {
        self.uids.clone()
    }

    fn bindings(&self, ctx: &Ctx) -> Option<(&Bindings, &ObjectEffect)> {
        Some((self.bind.get(ctx, "inventory")?, self.effect_bind.get(ctx, "object_effect")?))
    }

    /// ObjectItemInventory -> (uid, item) for equipment, shaped like an auction lot (`effects` compared with the item's defaults)
    /// so the UI can reuse the lot helpers; `prices` is empty.
    fn item(ctx: &Ctx, b: &Bindings, eb: &ObjectEffect, inv: &DynamicMessage, t: f64) -> Option<(i64, Value)> {
        let it = inv.get_field_by_name(&b.item)?.as_message()?.clone();
        let (gid, uid) = (int(&it, &b.gid), int(&it, &b.uid));
        if !ctx.data.is_equipment(gid) {
            return None;
        }
        let position = int(inv, &b.position);
        let effs = messages(&it, &b.effects).iter().map(|e| parse_effect(&ctx.data, eb, e)).collect();
        let info = ctx.data.items.get(&gid);
        Some((
            uid,
            json!({
                "uid": uid, "gid": gid, "name": ctx.data.item_name(gid), "level": info.map(|i| i.level), "icon": info.map(|i| i.icon_id),
                "quantity": int(&it, &b.quantity), "position": position, "equipped": position != BAG, "updated": t,
                "prices": [], "effects": ctx.data.compare_effects(stat_effects(effs), Some(gid)),
            }),
        ))
    }

    /// CharacterCharacteristicsEvent -> {characteristic id: {base, scroll}} for `KEPT_CHARACTERISTICS`.
    fn characteristics(b: &CharacBindings, msg: &DynamicMessage) -> Value {
        let mut out = serde_json::Map::new();
        let Some(stats) = msg.get_field_by_name(&b.stats).and_then(|v| v.as_message().cloned()) else { return Value::Null };
        for c in messages(&stats, &b.list) {
            let id = int(&c, &b.id);
            if !KEPT_CHARACTERISTICS.contains(&id) {
                continue;
            }
            let Some(d) = c.get_field_by_name(&b.detailed).and_then(|v| v.as_message().cloned()) else { continue };
            out.insert(id.to_string(), json!({"base": int(&d, &b.base), "scroll": int(&d, &b.additional)}));
        }
        Value::Object(out)
    }

    fn changed(&self, ctx: &Ctx) {
        self.store.touch();
        let list = self.store.lock().json();
        ctx.emit("inventory", "changed", list);
    }
}

impl Module for Inventory {
    fn id(&self) -> &'static str {
        "inventory"
    }

    fn interest(&self) -> Interest {
        Interest::Types(&[CONTENT, ADDED, ADDED_LIST, MODIFIED, DELETED, DELETED_LIST, MOVED, CHARACTERISTICS])
    }

    fn init(&self, ctx: &Ctx) {
        let Some(saved) = self.store.open(ctx, "inventory", "items.json", Content::json) else { return };
        let mut s = self.store.lock();
        s.updated = saved["updated"].as_f64().unwrap_or(0.0);
        s.character = saved["character"].clone();
        for it in saved["items"].as_array().into_iter().flatten() {
            if let Some(uid) = it["uid"].as_i64() {
                if let Some(seen) = Seen::of(it) {
                    self.uids.lock().insert(uid, seen);
                }
                s.items.insert(uid, it.clone());
            }
        }
    }

    fn on_message(&self, m: &Message, ctx: &Ctx) {
        let Some(msg) = m.decoded.as_ref() else { return };
        let t = m.raw.t;
        match m.name.as_str() {
            MOVED => {
                let Some(b) = self.move_bind.get(ctx, "inventory_move") else { return };
                let (uid, position) = (int(msg, &b.uid), int(msg, &b.position));
                let moved = {
                    let mut c = self.store.lock();
                    // only equipment is kept: a resource moved to the shortcut bar is not found here
                    c.items.get_mut(&uid).is_some_and(|it| {
                        let was = it["position"].as_i64();
                        it["position"] = json!(position);
                        it["equipped"] = json!(position != BAG);
                        was != Some(position)
                    })
                };
                if moved {
                    self.changed(ctx);
                }
                return;
            }
            CHARACTERISTICS => {
                let Some(b) = self.carac_bind.get(ctx, "characteristics") else { return };
                let carac = Self::characteristics(b, msg);
                if carac.as_object().is_none_or(|o| o.is_empty()) {
                    return;
                }
                let changed = {
                    let mut c = self.store.lock();
                    let same = c.character["carac"] == carac;
                    c.character = json!({"t": t, "carac": carac});
                    !same
                };
                // sent after every fight / level / scroll: saved only when the points or scrolls changed
                if changed {
                    self.changed(ctx);
                }
                return;
            }
            _ => {}
        }
        let Some((b, eb)) = self.bindings(ctx) else { return };
        let parse = |l: Vec<DynamicMessage>| -> Vec<(i64, Value)> {
            let out: Vec<(i64, Value)> = l.iter().filter_map(|x| Self::item(ctx, b, eb, x, t)).collect();
            let mut u = self.uids.lock();
            for (uid, it) in &out {
                if let Some(seen) = Seen::of(it) {
                    u.insert(*uid, seen);
                }
            }
            out
        };
        let one = |name: &str| msg.get_field_by_name(name).and_then(|v| v.as_message().cloned()).into_iter().collect::<Vec<_>>();
        // most object events are about resources (loot, quantities): only an actual change of the equipment is saved and sent
        let put = |s: &mut BTreeMap<i64, Value>, l: Vec<(i64, Value)>| {
            l.into_iter().fold(false, |ch, (uid, it)| s.insert(uid, it.clone()).as_ref() != Some(&it) || ch)
        };
        let changed = {
            let mut c = self.store.lock();
            if m.name == CONTENT {
                // a new login (maybe another character): the whole inventory
                c.updated = t;
            }
            let s = &mut c.items;
            match m.name.as_str() {
                CONTENT => {
                    s.clear();
                    s.extend(parse(messages(msg, &b.objects)));
                    true
                }
                ADDED => put(s, parse(one(&b.added))),
                ADDED_LIST => put(s, parse(messages(msg, &b.added_list))),
                MODIFIED => put(s, parse(one(&b.modified))),
                DELETED => s.remove(&int(msg, &b.deleted)).is_some(),
                DELETED_LIST => int_list(msg, &b.deleted_list).iter().filter(|uid| s.remove(uid).is_some()).count() > 0,
                _ => return,
            }
        };
        if changed {
            self.changed(ctx);
        }
    }

    fn call(&self, method: &str, _args: Value, _ctx: &Ctx) -> Result<Value> {
        match method {
            "list" => Ok(self.store.lock().json()),
            _ => Err(unknown("inventory", method)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::testkit;
    use loutil_proto::PbValue;

    /// A synthetic InventoryContentEvent (one worn item, one bag resource), then the item's deletion.
    #[test]
    fn reads_inventory_and_follows_deletions() {
        let Some((hub, proto)) = testkit::hub("loutil-inventory") else { return };
        let data = hub.ctx.data.clone();
        let b: Bindings = serde_json::from_value(data.bindings["inventory"].clone()).unwrap();
        let (content, deleted) = (proto.obf_for_clear(CONTENT).unwrap(), proto.obf_for_clear(DELETED).unwrap());
        let md = proto.message(&content).unwrap();
        let sub = |m: &loutil_proto::MessageDescriptor, f: &str| m.get_field_by_name(f).unwrap().kind().as_message().unwrap().clone();
        let (inv_md, item_md) = (sub(&md, &b.objects), sub(&sub(&md, &b.objects), &b.item));
        let equip = *data.items.keys().find(|g| data.is_equipment(**g) && !data.items[g].effects.is_empty()).unwrap();
        let resource = *data.items.keys().find(|g| !data.is_equipment(**g)).unwrap();
        let entry = |gid: i64, uid: i32, pos: i32| {
            let mut it = DynamicMessage::new(item_md.clone());
            it.set_field_by_name(&b.gid, PbValue::I32(gid as i32));
            it.set_field_by_name(&b.uid, PbValue::I32(uid));
            it.set_field_by_name(&b.quantity, PbValue::I32(1));
            let mut inv = DynamicMessage::new(inv_md.clone());
            inv.set_field_by_name(&b.item, PbValue::Message(it));
            inv.set_field_by_name(&b.position, PbValue::I32(pos));
            PbValue::Message(inv)
        };
        let mut msg = DynamicMessage::new(md);
        msg.set_field_by_name(&b.objects, PbValue::List(vec![entry(equip, 1001, 6), entry(resource, 1002, 63)]));
        let mut del = DynamicMessage::new(proto.message(&deleted).unwrap());
        del.set_field_by_name(&b.deleted, PbValue::I32(1001));
        let message = |name: &str, decoded: DynamicMessage| testkit::msg(name, Some(decoded), 5.0);
        let inv = Inventory::new();
        inv.on_message(&message(CONTENT, msg), &hub.ctx);
        let l = inv.call("list", Value::Null, &hub.ctx).unwrap();
        assert_eq!(l["items"].as_array().unwrap().len(), 1, "only equipment is kept");
        assert_eq!(l["items"][0]["gid"], json!(equip));
        assert_eq!(l["items"][0]["equipped"], json!(true));
        assert_eq!(l["updated"], json!(5.0));

        // unequipped (moved to the bag), then worn again in another slot
        let mb: MoveBindings = serde_json::from_value(data.bindings["inventory_move"].clone()).unwrap();
        let moved = |pos: i32| {
            let mut m = DynamicMessage::new(proto.message(&proto.obf_for_clear(MOVED).unwrap()).unwrap());
            m.set_field_by_name(&mb.uid, PbValue::I32(1001));
            m.set_field_by_name(&mb.position, PbValue::I32(pos));
            m
        };
        inv.on_message(&message(MOVED, moved(63)), &hub.ctx);
        let l = inv.call("list", Value::Null, &hub.ctx).unwrap();
        assert_eq!((l["items"][0]["position"].clone(), l["items"][0]["equipped"].clone()), (json!(63), json!(false)));
        inv.on_message(&message(MOVED, moved(9)), &hub.ctx);
        assert_eq!(inv.call("list", Value::Null, &hub.ctx).unwrap()["items"][0]["position"], json!(9));

        inv.on_message(&message(DELETED, del), &hub.ctx);
        assert!(inv.call("list", Value::Null, &hub.ctx).unwrap()["items"].as_array().unwrap().is_empty());
    }

    /// A synthetic CharacterCharacteristicsEvent: base points and scrolls of the six characteristics, nothing else.
    #[test]
    fn reads_characteristics() {
        let Some((hub, proto)) = testkit::hub("loutil-inventory-carac") else { return };
        let b: CharacBindings = serde_json::from_value(hub.ctx.data.bindings["characteristics"].clone()).unwrap();
        let md = proto.message(&proto.obf_for_clear(CHARACTERISTICS).unwrap()).unwrap();
        let sub = |m: &loutil_proto::MessageDescriptor, f: &str| m.get_field_by_name(f).unwrap().kind().as_message().unwrap().clone();
        let stats_md = sub(&md, &b.stats);
        let (c_md, d_md) = (sub(&stats_md, &b.list), sub(&sub(&stats_md, &b.list), &b.detailed));
        let carac = |id: i32, base: i32, scroll: i32| {
            let mut d = DynamicMessage::new(d_md.clone());
            d.set_field_by_name(&b.base, PbValue::I32(base));
            d.set_field_by_name(&b.additional, PbValue::I32(scroll));
            let mut c = DynamicMessage::new(c_md.clone());
            c.set_field_by_name(&b.id, PbValue::I32(id));
            c.set_field_by_name(&b.detailed, PbValue::Message(d));
            PbValue::Message(c)
        };
        let mut stats = DynamicMessage::new(stats_md.clone());
        stats.set_field_by_name(&b.list, PbValue::List(vec![carac(10, 202, 100), carac(11, 1, 0), carac(1, 7, 0)]));
        let mut msg = DynamicMessage::new(md);
        msg.set_field_by_name(&b.stats, PbValue::Message(stats));
        let inv = Inventory::new();
        inv.on_message(&testkit::msg(CHARACTERISTICS, Some(msg), 7.0), &hub.ctx);
        let c = &inv.call("list", Value::Null, &hub.ctx).unwrap()["character"];
        assert_eq!(c["carac"], json!({"10": {"base": 202, "scroll": 100}, "11": {"base": 1, "scroll": 0}}), "PA is not kept");
        assert_eq!(c["t"], json!(7.0));
    }
}
