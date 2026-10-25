//! Inventory: the character's equipment (worn and in the bag) with its real effects, read from the inventory sent at login
//! and kept up to date by the object added / modified / moved / deleted events (equipping = moving an object to a worn
//! position). The Stuff tab uses it to count an owned item at its actual roll instead of the perfect one, and to export the
//! worn equipment with the character's base points and scrolls (CharacterCharacteristicsEvent). Only equipment is kept; the
//! last character seen wins.
use crate::auction::{parse_effect, stat_effects, ObjectEffect};
use crate::kit::{unknown, Role, Store};
use crate::{Ctx, Interest, Message, Module};
use anyhow::Result;
use loutil_proto::util::{int, int_list, messages, scalar_ints};
use loutil_proto::{DynamicMessage, ReflectMessage};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Map, Value};
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
/// Characteristics whose base is known (invocations 1, prospection >= 100) and that never get a scroll (with range).
const INVOCATIONS: i64 = 26;
const PROSPECTION: i64 = 48;
const RANGE: i64 = 19;
/// Characteristic points of a level 200 character (5 per level).
const MAX_POINTS: i64 = 5 * 199;

/// Points spent to reach `value` in a characteristic: vitality 1 per point, wisdom 3, the four elements 1 / 2 / 3 / 4 per
/// point from 0 / 100 / 200 / 300 (the game's tiers).
fn points_cost(id: i64, value: i64) -> i64 {
    let v = value.max(0);
    match id {
        11 => v,
        12 => 3 * v,
        _ => v.min(100) + 2 * (v.min(200) - 100).max(0) + 3 * (v.min(300) - 200).max(0) + 4 * (v - 300).max(0),
    }
}

/// `position` of an object that is in the bag (0-15: worn).
const BAG: i64 = 63;

/// Obfuscated field names of the inventory messages (bindings.json `inventory`).
/// Same-typed fields cannot be told apart after a game update, so they come as lists told apart by value: `objects` = the
/// content's two ObjectItemInventory lists, `item` = the entry's two ObjectItem fields (see `entry_item`), `ints` = the
/// object's gid / uid / quantity (see `object_ints`).
#[derive(Deserialize, Clone)]
pub(crate) struct Bindings {
    pub objects: Vec<String>,
    pub item: Vec<String>,
    pub position: String,
    pub ints: Vec<String>,
    pub effects: String,
    pub added: String,
    pub added_list: String,
    pub modified: String,
    pub deleted: String,
    pub deleted_list: String,
}

/// Which of an ObjectItem's three ints (`Bindings::ints`) are the gid and the uid (the quantity is the third): learnt from
/// the login inventory (`learn_layout`), as field names.
type Layout = Option<(String, String)>;

/// The ObjectItem layout from many objects at once (the login inventory): the gid is the int that is most often an item
/// of the pack, the uid of the two others the one with the most distinct values (quantities repeat: 1, 100...).
fn learn_layout(data: &loutil_data::Data, b: &Bindings, objects: &[DynamicMessage]) -> Layout {
    if objects.len() < 10 || b.ints.len() != 3 {
        return None;
    }
    let col = |f: &str| objects.iter().map(|o| int(o, f)).collect::<Vec<i64>>();
    let items = |f: &String| col(f).iter().filter(|g| data.items.contains_key(g)).count();
    let gid = b.ints.iter().max_by_key(|f| items(f))?;
    let distinct = |f: &String| {
        let mut v = col(f);
        v.sort_unstable();
        v.dedup();
        (v.len(), v.last().copied().unwrap_or(0))
    };
    let uid = b.ints.iter().filter(|f| *f != gid).max_by_key(|f| distinct(f))?;
    Some((gid.clone(), uid.clone()))
}

/// (gid, uid, quantity) of an ObjectItem: by the learnt layout, else guessed from this object alone: the gid is its int that
/// is an item of the pack (the larger one when several are, the largest being taken for the uid when there are three), the
/// uid the larger other one (0 = unset: then the other is the quantity).
pub(crate) fn object_ints(data: &loutil_data::Data, b: &Bindings, it: &DynamicMessage, layout: &Layout) -> (i64, i64, i64) {
    if let Some((g, u)) = layout {
        let q = b.ints.iter().find(|f| *f != g && *f != u).map_or(1, |f| int(it, f));
        return (int(it, g), int(it, u), q);
    }
    let mut ints: Vec<i64> = b.ints.iter().map(|f| int(it, f)).collect();
    ints.sort_unstable();
    let items: Vec<usize> = (0..ints.len()).filter(|&i| data.items.contains_key(&ints[i])).collect();
    let at = match items[..] {
        [] => ints.len().saturating_sub(2),
        [i] => i,
        _ if items.len() == ints.len() => items[items.len() - 2],
        _ => *items.last().unwrap(),
    };
    if ints.len() != 3 {
        return (ints.get(at).copied().unwrap_or(0), 0, 1);
    }
    let gid = ints.remove(at);
    let (lo, hi) = (ints[0], ints[1]);
    if lo == 0 {
        (gid, 0, hi)
    } else {
        (gid, hi, lo)
    }
}

/// The ObjectItem of an ObjectItemInventory: the one of its two ObjectItem fields that is set (the other one is, so far,
/// never seen set: when both are, the one holding an item of the pack).
fn entry_item(data: &loutil_data::Data, b: &Bindings, inv: &DynamicMessage, layout: &Layout) -> Option<DynamicMessage> {
    let set: Vec<DynamicMessage> = b.item.iter().filter(|f| inv.has_field_by_name(f)).filter_map(|f| inv.get_field_by_name(f)?.as_message().cloned()).collect();
    let known = |it: &DynamicMessage| data.items.contains_key(&object_ints(data, b, it, layout).0);
    set.iter().find(|it| known(it)).or(set.first()).cloned()
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
    /// fields told apart by their values, for messages whose unnamed ints look alike (their names rotate per build):
    /// ObjectMovementEvent `move_position`, CharacterCharacteristicDetailed `carac_base` / `carac_scroll`
    learned: Map<String, Value>,
}

impl Content {
    fn json(&self) -> Value {
        json!({"updated": self.updated, "items": self.items.values().collect::<Vec<_>>(), "character": self.character, "learned": self.learned})
    }

    fn learned(&self, key: &str) -> Option<String> {
        self.learned.get(key).and_then(Value::as_str).map(str::to_owned)
    }

    /// Remember `field` for `key`; true when that is new.
    fn learn(&mut self, key: &str, field: &str) -> bool {
        if self.learned(key).as_deref() == Some(field) {
            return false;
        }
        self.learned.insert(key.into(), json!(field));
        true
    }
}

/// Of `candidates`, the single one (sure: true), else the learned field when it is among them.
fn pick<'a>(candidates: &[&'a str], learned: Option<&str>) -> (Option<&'a str>, bool) {
    match candidates {
        [one] => (Some(*one), true),
        _ => (candidates.iter().copied().find(|c| Some(*c) == learned), false),
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
    fn item(ctx: &Ctx, b: &Bindings, eb: &ObjectEffect, inv: &DynamicMessage, t: f64, layout: &Layout) -> Option<(i64, Value)> {
        let it = entry_item(&ctx.data, b, inv, layout)?;
        let (gid, uid, quantity) = object_ints(&ctx.data, b, &it, layout);
        if !ctx.data.is_equipment(gid) {
            return None;
        }
        let position = int(inv, &b.position);
        let stats = stat_effects(messages(&it, &b.effects).iter().map(|e| parse_effect(&ctx.data, eb, e)).collect());
        let density = (ctx.data.item_density(&stats) * 1000.0).round() / 1000.0;
        let info = ctx.data.items.get(&gid);
        Some((
            uid,
            json!({
                "uid": uid, "gid": gid, "name": ctx.data.item_name(gid), "level": info.map(|i| i.level), "icon": info.map(|i| i.icon_id),
                "quantity": quantity, "position": position, "equipped": position != BAG, "updated": t,
                "prices": [], "density": density, "effects": ctx.data.compare_effects(stats, Some(gid)),
            }),
        ))
    }

    /// CharacterCharacteristicsEvent -> characteristic id -> the integer fields of its CharacterCharacteristicDetailed. The path
    /// has exact clear names (stats -> characteristics -> characteristic_id / detailed); the detail's ints do not.
    fn detailed(ctx: &Ctx, msg: &DynamicMessage) -> Option<HashMap<i64, HashMap<String, i64>>> {
        let field = |m: &DynamicMessage, clear: &str| ctx.proto.field_named(m.descriptor().full_name(), clear);
        let stats = msg.get_field_by_name(&field(msg, "stats")?)?.as_message()?.clone();
        let mut out = HashMap::new();
        for c in messages(&stats, &field(&stats, "characteristics")?) {
            let (Some(id), Some(det)) = (field(&c, "characteristic_id"), field(&c, "detailed")) else { continue };
            if let Some(d) = c.get_field_by_name(&det).and_then(|v| v.as_message().cloned()) {
                out.insert(int(&c, &id), scalar_ints(&d).into_iter().collect());
            }
        }
        Some(out)
    }

    /// The detail fields holding the base points and the scrolls, told by their values: the base is 1 for invocations, at
    /// least 100 for prospection, and the six characteristics cost at most `MAX_POINTS`; scrolls are at most 100 on the six,
    /// 0 on range / invocations / prospection, and one is set. Several fit: the field learned before. Returns (base, scroll
    /// (None = the character has none), learned something new).
    fn base_and_scroll(c: &mut Content, det: &HashMap<i64, HashMap<String, i64>>) -> (Option<String>, Option<String>, bool) {
        let v = |id: i64, f: &str| det.get(&id).and_then(|m| m.get(f)).copied().unwrap_or(0);
        let mut fields: Vec<&str> = det.values().flat_map(|m| m.keys().map(String::as_str)).collect();
        fields.sort();
        fields.dedup();
        let base_c: Vec<&str> = fields
            .iter()
            .copied()
            .filter(|f| v(INVOCATIONS, f) == 1 && v(PROSPECTION, f) >= 100)
            .filter(|f| KEPT_CHARACTERISTICS.iter().map(|id| points_cost(*id, v(*id, f))).sum::<i64>() <= MAX_POINTS)
            .collect();
        let (base, sure_base) = pick(&base_c, c.learned("carac_base").as_deref());
        let scroll_c: Vec<&str> = fields
            .iter()
            .copied()
            .filter(|f| Some(*f) != base && [RANGE, INVOCATIONS, PROSPECTION].iter().all(|id| v(*id, f) == 0))
            .filter(|f| KEPT_CHARACTERISTICS.iter().all(|id| (0..=100).contains(&v(*id, f))) && KEPT_CHARACTERISTICS.iter().any(|id| v(*id, f) > 0))
            .collect();
        let (scroll, sure_scroll) = pick(&scroll_c, c.learned("carac_scroll").as_deref());
        let mut new = false;
        if let (Some(b), true) = (base, sure_base) {
            new |= c.learn("carac_base", b);
        }
        if let (Some(s), true) = (scroll, sure_scroll) {
            new |= c.learn("carac_scroll", s);
        }
        (base.map(str::to_owned), scroll.map(str::to_owned), new)
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
        s.learned = saved["learned"].as_object().cloned().unwrap_or_default();
        for it in saved["items"].as_array().into_iter().flatten() {
            if let Some(uid) = it["uid"].as_i64() {
                if let Some(seen) = Seen::of(it) {
                    self.uids.lock().insert(uid, seen);
                }
                let mut it = it.clone();
                if it.get("density").is_none() {
                    // stored before items carried their density (the Vente tab of the HDV compares it)
                    let sum: f64 = it["effects"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|e| e["status"] != "removed")
                        .filter_map(|e| ctx.data.effect_density(e["action"].as_i64()?, e["params"].as_array()?))
                        .sum();
                    it["density"] = json!((sum * 1000.0).round() / 1000.0);
                }
                s.items.insert(uid, it);
            }
        }
    }

    fn on_message(&self, m: &Message, ctx: &Ctx) {
        let Some(msg) = m.decoded.as_ref() else { return };
        let t = m.raw.t;
        match m.name.as_str() {
            MOVED => {
                // the uid is the int naming a stored piece of equipment (resources are not kept: their moves are skipped);
                // the position is the other int in 1..=63 (0 = unset or the amulet slot: settled by the field learned before)
                let ints = scalar_ints(msg);
                let moved = {
                    let mut c = self.store.lock();
                    let Some((uid_f, uid)) = ints.iter().find(|(_, v)| c.items.contains_key(v)).cloned() else { return };
                    let cands: Vec<&str> = ints.iter().filter(|(f, v)| *f != uid_f && (1..=BAG).contains(v)).map(|(f, _)| f.as_str()).collect();
                    let learned = c.learned("move_position");
                    let (field, sure) = match pick(&cands, learned.as_deref()) {
                        // nothing set besides the uid: moved to position 0, if the position field is known
                        (None, _) if cands.is_empty() => (learned.filter(|l| *l != uid_f), false),
                        (f, sure) => (f.map(str::to_owned), sure),
                    };
                    let Some(field) = field else {
                        ctx.log(format!("inventory: move of {uid} skipped (position field not known yet)"));
                        return;
                    };
                    let learned_now = sure && c.learn("move_position", &field);
                    let position = ints.iter().find(|(f, _)| *f == field).map_or(0, |(_, v)| *v);
                    let Some(it) = c.items.get_mut(&uid) else { return };
                    let was = it["position"].as_i64();
                    it["position"] = json!(position);
                    it["equipped"] = json!(position != BAG);
                    learned_now || was != Some(position)
                };
                if moved {
                    self.changed(ctx);
                }
                return;
            }
            CHARACTERISTICS => {
                let Some(det) = Self::detailed(ctx, msg) else { return };
                let changed = {
                    let mut c = self.store.lock();
                    let (base, scroll, learned) = Self::base_and_scroll(&mut c, &det);
                    let Some(base) = base else {
                        ctx.log("inventory: characteristics skipped (base points not recognised)");
                        return;
                    };
                    let v = |id: i64, f: Option<&str>| f.and_then(|f| det.get(&id)?.get(f).copied()).unwrap_or(0);
                    let carac: Map<String, Value> = KEPT_CHARACTERISTICS
                        .iter()
                        .map(|id| (id.to_string(), json!({"base": v(*id, Some(&base)), "scroll": v(*id, scroll.as_deref())})))
                        .collect();
                    let carac = Value::Object(carac);
                    let same = c.character["carac"] == carac;
                    c.character = json!({"t": t, "carac": carac});
                    learned || !same
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
        let objects = || b.objects.iter().flat_map(|f| messages(msg, f)).collect::<Vec<_>>();
        if m.name == CONTENT {
            // the login inventory: enough objects to learn which int is which
            let items: Vec<DynamicMessage> = objects().iter().filter_map(|x| entry_item(&ctx.data, b, x, &None)).collect();
            if let Some((g, u)) = learn_layout(&ctx.data, b, &items) {
                let mut c = self.store.lock();
                if c.learn("object_gid", &g) | c.learn("object_uid", &u) {
                    ctx.log(format!("inventory: object fields (gid, uid) = ({g}, {u})"));
                }
            }
        }
        let layout: Layout = {
            let c = self.store.lock();
            c.learned("object_gid").zip(c.learned("object_uid")).filter(|(g, u)| b.ints.contains(g) && b.ints.contains(u))
        };
        let parse = |l: Vec<DynamicMessage>| -> Vec<(i64, Value)> {
            let out: Vec<(i64, Value)> = l.iter().filter_map(|x| Self::item(ctx, b, eb, x, t, &layout)).collect();
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
                    // both lists of the content (only one was ever filled on the captures)
                    s.extend(parse(objects()));
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
        let (inv_md, item_md) = (sub(&md, &b.objects[0]), sub(&sub(&md, &b.objects[0]), &b.item[0]));
        let equip = *data.items.keys().find(|g| data.is_equipment(**g) && !data.items[g].effects.is_empty()).unwrap();
        let resource = *data.items.keys().find(|g| !data.is_equipment(**g)).unwrap();
        let entry = |gid: i64, uid: i32, pos: i32| {
            let mut it = DynamicMessage::new(item_md.clone());
            // the ints in any order: told apart by value
            for (f, v) in b.ints.iter().zip([1, gid as i32, uid]) {
                it.set_field_by_name(f, PbValue::I32(v));
            }
            let mut inv = DynamicMessage::new(inv_md.clone());
            inv.set_field_by_name(&b.item[0], PbValue::Message(it));
            inv.set_field_by_name(&b.position, PbValue::I32(pos));
            PbValue::Message(inv)
        };
        let mut msg = DynamicMessage::new(md);
        msg.set_field_by_name(&b.objects[0], PbValue::List(vec![entry(equip, 5400001, 6), entry(resource, 5400002, 63)]));
        let mut del = DynamicMessage::new(proto.message(&deleted).unwrap());
        del.set_field_by_name(&b.deleted, PbValue::I32(5400001));
        let message = |name: &str, decoded: DynamicMessage| testkit::msg(name, Some(decoded), 5.0);
        let inv = Inventory::new();
        inv.on_message(&message(CONTENT, msg), &hub.ctx);
        let l = inv.call("list", Value::Null, &hub.ctx).unwrap();
        assert_eq!(l["items"].as_array().unwrap().len(), 1, "only equipment is kept");
        assert_eq!(l["items"][0]["gid"], json!(equip));
        assert_eq!(l["items"][0]["equipped"], json!(true));
        assert_eq!(l["updated"], json!(5.0));

        // unequipped (moved to the bag), worn again in another slot, then in slot 0 (the position field is unset): the
        // fields are told apart by their values, whichever ints of the build's ObjectMovementEvent hold them
        let move_md = proto.message(&proto.obf_for_clear(MOVED).expect("ObjectMovementEvent named by its anchor")).unwrap();
        let ints: Vec<String> = scalar_ints(&DynamicMessage::new(move_md.clone())).into_iter().map(|(f, _)| f).collect();
        let (uid_f, pos_f) = (ints[0].clone(), ints[ints.len() - 1].clone());
        let moved = |pos: i32| {
            let mut m = DynamicMessage::new(move_md.clone());
            m.set_field_by_name(&uid_f, PbValue::I32(5400001));
            m.set_field_by_name(&pos_f, PbValue::I32(pos));
            m
        };
        let position = |inv: &Inventory| inv.call("list", Value::Null, &hub.ctx).unwrap()["items"][0]["position"].clone();
        inv.on_message(&message(MOVED, moved(63)), &hub.ctx);
        let l = inv.call("list", Value::Null, &hub.ctx).unwrap();
        assert_eq!((l["items"][0]["position"].clone(), l["items"][0]["equipped"].clone()), (json!(63), json!(false)));
        assert_eq!(l["learned"]["move_position"], json!(pos_f));
        inv.on_message(&message(MOVED, moved(9)), &hub.ctx);
        assert_eq!(position(&inv), json!(9));
        inv.on_message(&message(MOVED, moved(0)), &hub.ctx);
        assert_eq!(position(&inv), json!(0), "amulet slot: position unset, read from the learned field");

        inv.on_message(&message(DELETED, del), &hub.ctx);
        assert!(inv.call("list", Value::Null, &hub.ctx).unwrap()["items"].as_array().unwrap().is_empty());
    }

    #[test]
    fn characteristic_points() {
        // 202 strength = 100 + 2 x 100 + 3 x 2; the capture's character: 995 points = level 200
        assert_eq!(points_cost(10, 202), 306);
        assert_eq!(points_cost(12, 10), 30);
        assert_eq!(points_cost(11, 1), 1);
        assert_eq!(points_cost(13, 350), 100 + 200 + 300 + 200);
        assert_eq!([(10, 202), (13, 200), (14, 144), (15, 150), (11, 1)].iter().map(|(i, v)| points_cost(*i, *v)).sum::<i64>(), 995);
    }

    /// A CharacterCharacteristicsEvent with a capture's values (level 200 character): the base and scroll fields are found by
    /// their values, whichever of the build's detail ints hold them.
    #[test]
    fn reads_characteristics() {
        let Some((hub, proto)) = testkit::hub("loutil-inventory-carac") else { return };
        let md = proto.message(&proto.obf_for_clear(CHARACTERISTICS).unwrap()).unwrap();
        let named = |m: &loutil_proto::MessageDescriptor, clear: &str| proto.field_named(m.full_name(), clear).unwrap_or_else(|| panic!("{clear}"));
        let sub = |m: &loutil_proto::MessageDescriptor, f: &str| m.get_field_by_name(f).unwrap().kind().as_message().unwrap().clone();
        let stats_f = named(&md, "stats");
        let stats_md = sub(&md, &stats_f);
        let list_f = named(&stats_md, "characteristics");
        let c_md = sub(&stats_md, &list_f);
        let (id_f, det_f) = (named(&c_md, "characteristic_id"), named(&c_md, "detailed"));
        let d_md = sub(&c_md, &det_f);
        let f: Vec<String> = scalar_ints(&DynamicMessage::new(d_md.clone())).into_iter().map(|(f, _)| f).collect();
        assert!(f.len() >= 4, "detail ints: {f:?}");
        // as captured: (id, base, scroll, items, a 4th field) — the 4th (pods 1000) is in another field than the base
        let rows = [
            (10, 202, 100, 100, 0),
            (11, 1, 0, 2686, 0),
            (12, 0, 0, 295, 0),
            (13, 200, 100, 199, 0),
            (14, 144, 100, 247, 0),
            (15, 150, 100, 247, 0),
            (19, 0, 0, 4, 0),
            (26, 1, 0, 3, 0),
            (40, 16967, 0, 0, 1000),
            (48, 149, 0, 37, 0),
        ];
        let (base_f, scroll_f) = (f[2].clone(), f[0].clone());
        let carac = |(id, base, scroll, items, other): (i32, i32, i32, i32, i32)| {
            let mut d = DynamicMessage::new(d_md.clone());
            for (field, v) in [(&base_f, base), (&scroll_f, scroll), (&f[1], items), (&f[3], other)] {
                d.set_field_by_name(field, PbValue::I32(v));
            }
            let mut c = DynamicMessage::new(c_md.clone());
            c.set_field_by_name(&id_f, PbValue::I32(id));
            c.set_field_by_name(&det_f, PbValue::Message(d));
            PbValue::Message(c)
        };
        let mut stats = DynamicMessage::new(stats_md.clone());
        stats.set_field_by_name(&list_f, PbValue::List(rows.into_iter().map(carac).collect()));
        let mut msg = DynamicMessage::new(md);
        msg.set_field_by_name(&stats_f, PbValue::Message(stats));
        let inv = Inventory::new();
        inv.on_message(&testkit::msg(CHARACTERISTICS, Some(msg), 7.0), &hub.ctx);
        let l = inv.call("list", Value::Null, &hub.ctx).unwrap();
        assert_eq!((l["learned"]["carac_base"].clone(), l["learned"]["carac_scroll"].clone()), (json!(base_f), json!(scroll_f)));
        let c = &l["character"];
        assert_eq!(c["carac"]["10"], json!({"base": 202, "scroll": 100}));
        assert_eq!(c["carac"]["11"], json!({"base": 1, "scroll": 0}));
        assert_eq!(c["carac"]["12"], json!({"base": 0, "scroll": 0}));
        assert_eq!(c["carac"].as_object().unwrap().len(), 6, "only the six characteristics");
        assert_eq!(c["t"], json!(7.0));
    }

    /// Dev check on a capture: `INV_PAYLOAD=<file holding an InventoryContentEvent payload in hex> cargo test -p loutil-modules
    /// real_login -- --nocapture`: the object fields are learnt and the equipment read (gid / uid / quantity told apart by value).
    #[test]
    fn real_login() {
        let Ok(path) = std::env::var("INV_PAYLOAD") else { return };
        let Some((hub, proto)) = testkit::hub("loutil-inventory-real") else { return };
        let h = std::fs::read_to_string(path).unwrap();
        let payload: Vec<u8> = (0..h.trim().len() / 2).map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap()).collect();
        let decoded = proto.decode(&proto.obf_for_clear(CONTENT).unwrap(), &payload);
        let inv = Inventory::new();
        inv.on_message(&testkit::msg(CONTENT, decoded, 1.0), &hub.ctx);
        let l = inv.call("list", Value::Null, &hub.ctx).unwrap();
        let items = l["items"].as_array().unwrap();
        eprintln!("learned {}; {} equipment pieces:", l["learned"], items.len());
        for i in items.iter().take(5) {
            eprintln!("  {} {} uid {} x{} pos {}", i["gid"], i["name"], i["uid"], i["quantity"], i["position"]);
        }
        assert!(!items.is_empty());
        assert!(l["learned"]["object_gid"].is_string() && l["learned"]["object_uid"].is_string());
        assert!(items.iter().all(|i| i["name"].is_string() && i["uid"].as_i64() > i["gid"].as_i64()), "gid / uid told apart");
    }
}
