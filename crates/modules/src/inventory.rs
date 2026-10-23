//! Inventory: the character's equipment (worn and in the bag) with its real effects, read from the inventory sent at login
//! and kept up to date by the object added / modified / deleted events. The Stuff tab uses it to count an owned item at its
//! actual roll instead of the perfect one. Only equipment is kept; the last character seen wins.
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
}

impl Content {
    fn json(&self) -> Value {
        json!({"updated": self.updated, "items": self.items.values().collect::<Vec<_>>()})
    }
}

#[derive(Default)]
pub struct Inventory {
    bind: Role<Bindings>,
    effect_bind: Role<ObjectEffect>,
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
        Interest::Types(&[CONTENT, ADDED, ADDED_LIST, MODIFIED, DELETED, DELETED_LIST])
    }

    fn init(&self, ctx: &Ctx) {
        let Some(saved) = self.store.open(ctx, "inventory", "items.json", Content::json) else { return };
        let mut s = self.store.lock();
        s.updated = saved["updated"].as_f64().unwrap_or(0.0);
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
        let (Some(msg), Some((b, eb))) = (m.decoded.as_ref(), self.bindings(ctx)) else { return };
        let t = m.raw.t;
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

        inv.on_message(&message(DELETED, del), &hub.ctx);
        assert!(inv.call("list", Value::Null, &hub.ctx).unwrap()["items"].as_array().unwrap().is_empty());
    }
}
