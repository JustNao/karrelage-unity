//! Auction house: renders the AH item-list message (prices per lot size, effects vs the item's defaults)
//! and keeps the latest listing per item. Port of formatters.format_ah_list + webapp.update_store.
use crate::kit::{unknown, Role, Store};
use crate::{Ctx, Interest, Message, Module};
use anyhow::Result;
use loutil_data::items::{info_key, Effect, ELEMENT_CHANGE_EFFECT, LOCK_EFFECT, LOT_SIZES};
use loutil_data::Data;
use loutil_proto::util::{as_i64, int, int_list, messages, oneof_value};
use loutil_proto::{DynamicMessage, Kind, PbValue, ReflectMessage};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Clear name pinned in pipeline overrides (work/overrides.json) for the AH listing event.
pub const AH_LIST: &str = "ExchangeTypesItemsExchangerDescriptionForUserEvent";
/// Answer to an AH purchase: the lot bought leaves the stored listing.
pub const AH_BUY: &str = "ExchangeBidHouseBuyResultEvent";
/// A row of the open AH listing changed (new prices after a purchase or a new offer) or left it.
pub const AH_ROW_UPDATED: &str = "ExchangeBidHouseInListUpdatedEvent";
pub const AH_ROW_REMOVED: &str = "ExchangeBidHouseInListRemovedEvent";
/// Opening a category of the AH: the gids of every item on sale in that item type.
pub const AH_TYPE: &str = "ExchangeTypesExchangerDescriptionForUserEvent";
/// The last lot of an item left the AH (the item is no longer listed in its category).
pub const AH_GONE: &str = "ExchangeBidHouseGenericItemRemovedEvent";
/// The player's own lots: the AH sell window opened (every lot they have on sale) / a lot put on sale.
pub const AH_SELLER: &str = "ExchangeBidSellerStartedEvent";
pub const AH_ADDED: &str = "ExchangeBidHouseItemAddedEvent";

// build-specific obfuscated field names, read from the data pack's bindings.json
#[derive(Deserialize, Clone)]
struct Bindings {
    ah_list: AhList,
    ah_item: AhItem,
    object_effect: ObjectEffect,
}
#[derive(Deserialize, Clone)]
struct AhList {
    items: String,
    ints: Vec<String>,
}
#[derive(Deserialize, Clone)]
struct AhItem {
    prices: String,
    effects: String,
    ints: Vec<String>,
}
#[derive(Deserialize, Clone)]
pub(crate) struct ObjectEffect {
    action: Vec<String>,
    oneof: String,
}
/// bindings.json `ah_buy` / `ah_row` / `ah_row_removed` (each optional, so that an older pack still loads the listing bindings).
#[derive(Deserialize, Clone)]
struct AhBuy {
    uid: String,
    bought: String,
}
#[derive(Deserialize, Clone)]
struct AhRow {
    prices: String,
    effects: String,
    ints: Vec<String>,
}
#[derive(Deserialize, Clone)]
struct AhRowRemoved {
    ints: Vec<String>,
}
/// bindings.json `ah_type` (category opened: gids on sale + the item type) / `ah_gone` (gid no longer on sale).
#[derive(Deserialize, Clone)]
struct AhType {
    gids: String,
    #[serde(rename = "type")]
    type_id: String,
}
#[derive(Deserialize, Clone)]
struct AhGone {
    gid: String,
}
/// bindings.json `ah_mine`: the seller's lots (`objects` -> `item`, or the `added_item` of a lot put on sale), an item
/// being three unordered ints uid / quantity / gid.
#[derive(Deserialize, Clone)]
struct AhMine {
    objects: String,
    item: String,
    added_item: String,
    ints: Vec<String>,
}

#[derive(Default)]
pub struct Auction {
    bind: Role<Bindings>,
    buy_bind: Role<AhBuy>,
    row_bind: Role<AhRow>,
    removed_bind: Role<AhRowRemoved>,
    type_bind: Role<AhType>,
    gone_bind: Role<AhGone>,
    mine_bind: Role<AhMine>,
    /// gid -> latest listing of an equipment item (`items.json`)
    store: Store<BTreeMap<String, Value>>,
    /// item type -> {type, t, gids}: what was on sale in that AH category when it was last opened (minus the items whose
    /// last lot left since)
    on_sale: Store<BTreeMap<i64, Value>>,
    /// uid -> {uid, gid}: the player's own lots on sale (as of the last time the sell window was opened, plus the lots put
    /// on sale since)
    mine: Store<BTreeMap<i64, Value>>,
    /// pack-derived answers, constant once the pack is loaded
    effects: OnceLock<Value>,
    recipes: OnceLock<Value>,
    effect_meta: OnceLock<Value>,
}

impl Auction {
    pub fn new() -> Self {
        Self::default()
    }

    fn bindings(&self, ctx: &Ctx) -> Option<&Bindings> {
        self.bind.root(ctx)
    }

    /// Keep the latest listing per item; a new packet for the same item replaces the old one.
    fn update_store(&self, m: &Message, ctx: &Ctx) {
        let Some(f) = &m.fmt else { return };
        if f["kind"] != "auction_items" {
            return;
        }
        let Some(rows) = f["rows"].as_array().filter(|r| !r.is_empty()) else { return };
        let Some(gid) = f["gid"].as_i64().or_else(|| rows[0]["gid"].as_i64()) else { return };
        if !ctx.data.is_equipment(gid) {
            return; // resources / consumables only feed the stored prices
        }
        let mins: Vec<i64> = rows.iter().filter_map(|r| r["prices"].get(0).and_then(|p| p["price"].as_i64())).filter(|p| *p > 0).collect();
        let ent = json!({
            "gid": gid,
            "name": if f["title"].is_null() { rows[0]["name"].clone() } else { f["title"].clone() },
            "type": if f["type"].is_null() { rows[0]["type"].clone() } else { f["type"].clone() },
            "companion": ctx.data.is_companion(Some(gid)), "feedable": ctx.data.is_feedable(Some(gid)), "level": rows[0]["level"], "icon": rows[0]["icon"], "updated": m.raw.t, "lots": rows.len(),
            "min_price": mins.iter().min(), "rows": rows, "defaults": f["defaults"], "demo": m.raw.srv == "demo",
        });
        self.store.update(|s| s.insert(gid.to_string(), ent.clone()));
        ctx.emit("auction", "item", ent);
    }

    /// A category opened (`auction_type`: every gid on sale in an item type) or an item gone from it (`auction_gone`).
    fn on_sale_change(&self, m: &Message, ctx: &Ctx) {
        let Some(f) = &m.fmt else { return };
        let ent = match f["kind"].as_str() {
            Some("auction_type") => {
                let Some(t) = f["type"].as_i64() else { return };
                let ent = json!({"type": t, "t": m.raw.t, "gids": f["gids"]});
                self.on_sale.lock().insert(t, ent.clone());
                ent
            }
            Some("auction_gone") => {
                let Some(gid) = f["gid"].as_i64() else { return };
                let Some(t) = ctx.data.items.get(&gid).map(|i| i.type_id) else { return };
                let mut s = self.on_sale.lock();
                let Some(ent) = s.get_mut(&t) else { return };
                let Some(gids) = ent["gids"].as_array_mut() else { return };
                gids.retain(|g| g.as_i64() != Some(gid));
                ent.clone()
            }
            _ => return,
        };
        self.on_sale.touch();
        ctx.emit("auction", "on_sale", ent);
    }

    /// The sell window opened (`auction_mine`: the whole list) or a lot put on sale (`auction_mine_added`).
    fn mine_change(&self, m: &Message, ctx: &Ctx) {
        let Some(f) = &m.fmt else { return };
        let lots = f["lots"].as_array().into_iter().flatten().filter_map(|l| Some((l["uid"].as_i64()?, l.clone())));
        {
            let mut s = self.mine.lock();
            if f["kind"] == "auction_mine" {
                s.clear();
            }
            s.extend(lots);
        }
        self.mine.touch();
        ctx.emit("auction", "mine", json!(self.mine.lock().values().collect::<Vec<_>>()));
    }

    /// A change of the open listing (`apply_change`): applied to the stored listing holding that lot, sent again with
    /// its scan time kept.
    fn on_change(&self, m: &Message, ctx: &Ctx) {
        let Some(f) = &m.fmt else { return };
        let mut s = self.store.lock();
        let Some(ent) = s.values_mut().find(|e| e["rows"].as_array().is_some_and(|r| r.iter().any(|x| !x["uid"].is_null() && x["uid"] == f["uid"]))) else {
            return;
        };
        let rows = ent["rows"].as_array_mut().unwrap();
        if apply_change(rows, f).is_none() {
            return;
        }
        let min = rows.iter().filter_map(|r| r["prices"].get(0).and_then(|p| p["price"].as_i64())).filter(|p| *p > 0).min();
        ent["lots"] = json!(rows.len());
        ent["min_price"] = json!(min);
        let ent = ent.clone();
        drop(s);
        self.store.touch();
        ctx.emit("auction", "item", ent);
    }
}

fn raw_json(v: &PbValue) -> Value {
    match v {
        PbValue::String(s) => json!(s),
        PbValue::Bool(b) => json!(b),
        PbValue::F32(x) => json!(x),
        PbValue::F64(x) => json!(x),
        PbValue::Bytes(b) => json!(format!("0x{}", loutil_core::wire::hex(b))),
        other => as_i64(other).map_or(Value::Null, |n| json!(n)),
    }
}

/// The effects that are stats of the item (info effects, the lock and the element-change marker left out).
pub(crate) fn stat_effects(effs: Vec<Effect>) -> Vec<Effect> {
    effs.into_iter().filter(|e| info_key(e.action).is_none() && e.action != LOCK_EFFECT && e.action != ELEMENT_CHANGE_EFFECT).collect()
}

/// ObjectEffect message -> annotated-later Effect.
pub(crate) fn parse_effect(data: &Data, b: &ObjectEffect, e: &DynamicMessage) -> Effect {
    let acts: Vec<i64> = b.action.iter().map(|n| int(e, n)).collect();
    let action = acts.iter().find(|a| data.effects.contains_key(a)).or_else(|| acts.iter().find(|a| **a != 0)).or(acts.first()).copied().unwrap_or(0);
    let (mut params, mut raw): (Vec<Value>, Value) = (vec![], Value::Null);
    if let Some((_, v, _)) = oneof_value(e, &b.oneof) {
        match &v {
            PbValue::Message(sub) => {
                let mut obj = Map::new();
                let mut vals = vec![];
                for fd in sub.descriptor().fields() {
                    let val = sub.get_field(&fd);
                    match fd.kind() {
                        Kind::Message(_) => {}
                        k => {
                            obj.insert(fd.name().to_string(), raw_json(&val));
                            if matches!(k, Kind::Int64 | Kind::Int32 | Kind::Uint32 | Kind::Uint64) {
                                vals.push(as_i64(&val).unwrap_or(0));
                            }
                        }
                    }
                }
                raw = Value::Object(obj);
                params = match vals.as_slice() {
                    [a, b] => vec![(*a).into(), (*b).into()],
                    [n, s, c] if *n != 0 && *s != 0 => vec![(c + n).into(), (c + n * s).into()], // dice num, side, const
                    [_, _, c] => vec![(*c).into()],
                    _ => vec![],
                };
            }
            PbValue::String(s) => {
                params = vec![json!(s)];
                raw = json!(s);
            }
            other => {
                raw = raw_json(other);
                params = vec![raw.clone()];
            }
        }
    }
    let (text, row) = data.render_effect(action, &params);
    let hi = params.iter().filter_map(Value::as_i64).max();
    Effect {
        action,
        text,
        known: row.is_some(),
        hi,
        str: raw.as_str().map(str::to_owned),
        raw: if raw.is_array() { Value::Null } else { raw },
        params,
        group: String::new(),
        order: 0,
        density: 0.0,
        stat: 0,
        status: None,
        range: None,
    }
}

/// One AH row (a lot: prices per stack size, effects) from its ints / prices / effects fields; `fallback` = the
/// listing's (gid, type) when the row's ints do not resolve.
fn format_row(
    data: &Data,
    oe: &ObjectEffect,
    it: &DynamicMessage,
    ints: Vec<i64>,
    prices_f: &str,
    effects_f: &str,
    fallback: (Option<i64>, Option<i64>),
) -> Value {
    let (mut gid, mut typ, mut uid) = data.resolve_gid_type(&ints);
    if gid.is_none() {
        (gid, typ, uid) = (fallback.0, fallback.1, None);
    }
    let prices = int_list(it, prices_f);
    let info = gid.and_then(|g| data.items.get(&g));
    let effs: Vec<Effect> = messages(it, effects_f).iter().map(|e| parse_effect(data, oe, e)).collect();
    let meta: Map<String, Value> =
        effs.iter().filter_map(|e| info_key(e.action).map(|k| (k.to_string(), e.str.clone().map_or(Value::Null, Value::from)))).collect();
    let locked = effs.iter().any(|e| e.action == LOCK_EFFECT);
    let stats = stat_effects(effs);
    let density = (data.item_density(&stats) * 1000.0).round() / 1000.0;
    json!({
        "uid": uid, "gid": gid, "ints": ints,
        "name": gid.map_or_else(|| "item#None".to_string(), |g| data.item_name(g)),
        "level": info.map(|i| i.level), "type": data.type_name(typ.or(info.map(|i| i.type_id))),
        "icon": info.map(|i| i.icon_id),
        "prices": prices.iter().enumerate().map(|(i, p)| json!({"qty": LOT_SIZES.get(i).copied().unwrap_or(i as i64), "price": p})).collect::<Vec<_>>(),
        "meta": meta, "locked": locked, "density": density,
        "effects": data.compare_effects(stats, gid),
    })
}

/// One of the player's lots on sale: the item's gid is the int that is an item id, its uid the largest of the others
/// (the quantity of an equipment lot is 1).
fn mine_lot(data: &Data, b: &AhMine, item: &DynamicMessage) -> Option<Value> {
    let ints: Vec<i64> = b.ints.iter().map(|n| int(item, n)).collect();
    let gi = ints.iter().position(|g| data.items.contains_key(g))?;
    let uid = ints.iter().enumerate().filter(|(i, _)| *i != gi).map(|(_, u)| *u).max()?;
    Some(json!({"uid": uid, "gid": ints[gi], "name": data.item_name(ints[gi])}))
}

/// Apply a listing change (fmt `auction_bought` / `auction_row` / `auction_row_removed`) to the rows of a listing.
/// None when it changes nothing (no row with that uid...). A purchase only removes a row holding a single offer (an
/// equipment lot): a resource row lists the cheapest stack of every size, the server's `auction_row` updates it.
pub(crate) fn apply_change(rows: &mut Vec<Value>, f: &Value) -> Option<()> {
    let uid = f["uid"].as_i64()?;
    let i = rows.iter().position(|r| r["uid"].as_i64() == Some(uid))?;
    match f["kind"].as_str()? {
        "auction_row" => rows[i] = f["row"].clone(),
        "auction_row_removed" => {
            rows.remove(i);
        }
        "auction_bought" => {
            let offers = rows[i]["prices"].as_array().map_or(0, |p| p.iter().filter(|p| p["price"].as_i64().unwrap_or(0) > 0).count());
            if f["bought"] != true || offers > 1 {
                return None;
            }
            rows.remove(i);
        }
        _ => return None,
    }
    Some(())
}

fn format_ah_list(data: &Data, b: &Bindings, msg: &DynamicMessage) -> Value {
    let names = |v: &[String]| -> Vec<i64> { v.iter().map(|n| int(msg, n)).collect() };
    let (gid0, type0, _) = data.resolve_gid_type(&names(&b.ah_list.ints));
    let mut rows = vec![];
    for it in messages(msg, &b.ah_list.items) {
        let ints: Vec<i64> = b.ah_item.ints.iter().map(|n| int(&it, n)).collect();
        rows.push(format_row(data, &b.object_effect, &it, ints, &b.ah_item.prices, &b.ah_item.effects, (gid0, type0)));
    }
    json!({
        "kind": "auction_items", "defaults": data.defaults_for(gid0),
        "title": gid0.map(|g| data.item_name(g)), "type": type0.map(|t| data.type_name(Some(t))),
        "gid": gid0, "rows": rows,
    })
}

impl Module for Auction {
    fn id(&self) -> &'static str {
        "auction"
    }

    fn interest(&self) -> Interest {
        Interest::Types(&[AH_LIST, AH_BUY, AH_ROW_UPDATED, AH_ROW_REMOVED, AH_TYPE, AH_GONE, AH_SELLER, AH_ADDED])
    }

    fn init(&self, ctx: &Ctx) {
        let saved = self.mine.open(ctx, "auction", "mine.json", |s| json!(s.values().collect::<Vec<_>>()));
        for l in saved.as_ref().and_then(Value::as_array).into_iter().flatten() {
            if let Some(uid) = l["uid"].as_i64() {
                self.mine.lock().insert(uid, l.clone());
            }
        }
        let saved = self.on_sale.open(ctx, "auction", "on_sale.json", |s| json!(s.values().collect::<Vec<_>>()));
        for e in saved.as_ref().and_then(Value::as_array).into_iter().flatten() {
            if let Some(t) = e["type"].as_i64() {
                self.on_sale.lock().insert(t, e.clone());
            }
        }
        let saved = self.store.open(ctx, "auction", "items.json", |s| {
            Value::Object(s.iter().filter(|(_, v)| v["demo"] != json!(true)).map(|(k, v)| (k.clone(), v.clone())).collect())
        });
        if let Some(Value::Object(saved)) = saved {
            let mut s = self.store.lock();
            for (k, mut v) in saved {
                if !v["gid"].as_i64().is_some_and(|g| ctx.data.is_equipment(g)) {
                    continue; // stored before the HDV tab was limited to equipment
                }
                v["companion"] = json!(ctx.data.is_companion(v["gid"].as_i64()));
                v["feedable"] = json!(ctx.data.is_feedable(v["gid"].as_i64()));
                if v.get("defaults").is_none() {
                    v["defaults"] = Value::Array(ctx.data.defaults_for(v["gid"].as_i64()));
                }
                // Stored lots may predate (or disagree with) the current data pack: re-tag their effects with the pack's
                // group/order/density, and recompute the lot density from them (the formula may have changed).
                if let Some(rows) = v.get_mut("rows").and_then(Value::as_array_mut) {
                    for e in rows.iter_mut().flat_map(|r| r["effects"].as_array_mut()).flatten() {
                        // the stored text was rendered with the pack of the time (e.g. a Dofus passive shown as a spell id)
                        if let (Some(action), Some(params)) = (e["action"].as_i64(), e["params"].as_array()) {
                            e["text"] = json!(ctx.data.render_effect(action, params).0);
                        }
                        if let Some(row) = e["action"].as_i64().and_then(|a| ctx.data.effects.get(&a)) {
                            e["group"] = json!(row.group);
                            e["order"] = json!(row.order);
                            e["density"] = json!(row.density);
                            e["stat"] = json!(ctx.data.stat_of(e["action"].as_i64().unwrap_or(0)));
                        }
                    }
                    for r in rows.iter_mut() {
                        let sum: f64 = r["effects"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|e| e["status"] != "removed")
                            .filter_map(|e| ctx.data.effect_density(e["action"].as_i64()?, e["params"].as_array()?))
                            .sum();
                        r["density"] = json!((sum * 1000.0).round() / 1000.0);
                    }
                }
                s.insert(k, v);
            }
        }
    }

    fn format(&self, m: &Message, ctx: &Ctx) -> Option<Value> {
        let msg = m.decoded.as_ref()?;
        match m.name.as_str() {
            AH_LIST => Some(format_ah_list(&ctx.data, self.bindings(ctx)?, msg)),
            AH_BUY => {
                let b = self.buy_bind.get(ctx, "ah_buy")?;
                let bought = msg.get_field_by_name(&b.bought).and_then(|v| v.as_bool()).unwrap_or(false);
                Some(json!({"kind": "auction_bought", "uid": int(msg, &b.uid), "bought": bought}))
            }
            AH_ROW_UPDATED => {
                let (b, oe) = (self.row_bind.get(ctx, "ah_row")?, &self.bindings(ctx)?.object_effect);
                let ints: Vec<i64> = b.ints.iter().map(|n| int(msg, n)).collect();
                let row = format_row(&ctx.data, oe, msg, ints, &b.prices, &b.effects, (None, None));
                Some(json!({"kind": "auction_row", "uid": row["uid"], "gid": row["gid"], "row": row}))
            }
            AH_ROW_REMOVED => {
                let b = self.removed_bind.get(ctx, "ah_row_removed")?;
                let ints: Vec<i64> = b.ints.iter().map(|n| int(msg, n)).collect();
                let (gid, _, uid) = ctx.data.resolve_gid_type(&ints);
                Some(json!({"kind": "auction_row_removed", "uid": uid, "gid": gid, "ints": ints}))
            }
            AH_TYPE => {
                let b = self.type_bind.get(ctx, "ah_type")?;
                let gids = int_list(msg, &b.gids);
                Some(
                    json!({"kind": "auction_type", "type": int(msg, &b.type_id), "type_name": ctx.data.type_name(Some(int(msg, &b.type_id))), "count": gids.len(), "gids": gids}),
                )
            }
            AH_GONE => {
                let b = self.gone_bind.get(ctx, "ah_gone")?;
                let gid = int(msg, &b.gid);
                Some(json!({"kind": "auction_gone", "gid": gid, "name": ctx.data.item_name(gid)}))
            }
            AH_SELLER => {
                let b = self.mine_bind.get(ctx, "ah_mine")?;
                let items = messages(msg, &b.objects).iter().filter_map(|o| o.get_field_by_name(&b.item)?.as_message().cloned()).collect::<Vec<_>>();
                Some(json!({"kind": "auction_mine", "lots": items.iter().filter_map(|i| mine_lot(&ctx.data, b, i)).collect::<Vec<_>>()}))
            }
            AH_ADDED => {
                let b = self.mine_bind.get(ctx, "ah_mine")?;
                let item = msg.get_field_by_name(&b.added_item)?.as_message()?.clone();
                Some(json!({"kind": "auction_mine_added", "lots": mine_lot(&ctx.data, b, &item).into_iter().collect::<Vec<_>>()}))
            }
            _ => None,
        }
    }

    fn on_message(&self, m: &Message, ctx: &Ctx) {
        if m.name == AH_TYPE || m.name == AH_GONE {
            return self.on_sale_change(m, ctx);
        }
        if m.name == AH_SELLER || m.name == AH_ADDED {
            return self.mine_change(m, ctx);
        }
        if m.name != AH_LIST {
            return self.on_change(m, ctx);
        }
        self.update_store(m, ctx);
    }

    fn call(&self, method: &str, _args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            "items" => Ok(Value::Array(self.store.lock().values().cloned().collect())),
            // what is on sale per item type: {type id: {type, t (category opened), gids}}
            "on_sale" => Ok(Value::Object(self.on_sale.lock().iter().map(|(t, e)| (t.to_string(), e.clone())).collect())),
            // the player's own lots on sale: [{uid, gid, name}]
            "mine" => Ok(json!(self.mine.lock().values().collect::<Vec<_>>())),
            "effects" => Ok(self.effects.get_or_init(|| ctx.data.filterable_effects()).clone()),
            "recipes" => Ok(self.recipes.get_or_init(|| ctx.data.recipes_json()).clone()),
            "effect_meta" => Ok(self.effect_meta.get_or_init(|| ctx.data.effect_meta()).clone()),
            "clear" => {
                self.store.update(|s| s.clear());
                ctx.emit("auction", "cleared", json!({}));
                Ok(json!({"ok": true}))
            }
            _ => Err(unknown("auction", method)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::testkit;
    use loutil_core::RawMessage;
    use loutil_proto::Proto;
    use parking_lot::Mutex;
    use std::sync::Arc;

    fn strip_keys(v: &mut Value, keys: &[&str]) {
        match v {
            Value::Object(m) => {
                m.retain(|k, _| !keys.contains(&k.as_str()));
                m.values_mut().for_each(|x| strip_keys(x, keys));
            }
            Value::Array(a) => a.iter_mut().for_each(|x| strip_keys(x, keys)),
            _ => {}
        }
    }

    fn strip_nulls(v: &mut Value) {
        match v {
            Value::Object(m) => {
                m.retain(|_, x| !x.is_null());
                m.values_mut().for_each(strip_nulls);
            }
            Value::Array(a) => a.iter_mut().for_each(strip_nulls),
            _ => {}
        }
    }

    /// A category opened (gids on sale in "Amulette"), then one of them gone from the AH. Needs the data pack.
    #[test]
    fn items_on_sale_per_category() {
        let Some((hub, proto)) = crate::kit::testkit::hub("loutil-onsale") else { return };
        let data = hub.ctx.data.clone();
        let (tb, gb) = (data.bindings["ah_type"].clone(), data.bindings["ah_gone"].clone());
        let (Some(t_obf), Some(g_obf)) = (proto.obf_for_clear(AH_TYPE), proto.obf_for_clear(AH_GONE)) else { return };
        let mut t = DynamicMessage::new(proto.message(&t_obf).unwrap());
        t.set_field_by_name(tb["gids"].as_str().unwrap(), PbValue::List(vec![PbValue::I32(11609), PbValue::I32(31789)]));
        t.set_field_by_name(tb["type"].as_str().unwrap(), PbValue::I32(1));
        let mut g = DynamicMessage::new(proto.message(&g_obf).unwrap());
        g.set_field_by_name(gb["gid"].as_str().unwrap(), PbValue::I32(11609));
        let a = Auction::new();
        let send = |name: &str, decoded: DynamicMessage, t: f64| {
            let mut m = testkit::msg(name, Some(decoded), t);
            m.fmt = a.format(&m, &hub.ctx);
            a.on_message(&m, &hub.ctx);
        };
        send(AH_TYPE, t, 10.0);
        let s = a.call("on_sale", Value::Null, &hub.ctx).unwrap();
        assert_eq!((s["1"]["gids"].clone(), s["1"]["t"].as_f64()), (json!([11609, 31789]), Some(10.0)));
        send(AH_GONE, g, 11.0);
        let s = a.call("on_sale", Value::Null, &hub.ctx).unwrap();
        assert_eq!((s["1"]["gids"].clone(), s["1"]["t"].as_f64()), (json!([31789]), Some(10.0)), "gone, category scan time kept");
    }

    /// The sell window lists the player's lots (uid told from the gid / quantity by value), a lot put on sale is added and
    /// the next sell window replaces the list. Needs the data pack.
    #[test]
    fn own_lots_on_sale() {
        let Some((hub, proto)) = crate::kit::testkit::hub("loutil-mine") else { return };
        let b: AhMine = serde_json::from_value(hub.ctx.data.bindings["ah_mine"].clone()).unwrap();
        let (Some(s_obf), Some(a_obf)) = (proto.obf_for_clear(AH_SELLER), proto.obf_for_clear(AH_ADDED)) else { return };
        let (s_md, a_md) = (proto.message(&s_obf).unwrap(), proto.message(&a_obf).unwrap());
        let item = |uid: i32, gid: i32| {
            let md = a_md.get_field_by_name(&b.added_item).unwrap().kind().as_message().unwrap().clone();
            let mut it = DynamicMessage::new(md);
            for (n, v) in b.ints.iter().zip([1, uid, gid]) {
                it.set_field_by_name(n, PbValue::I32(v));
            }
            it
        };
        let objects = s_md.get_field_by_name(&b.objects).unwrap();
        let entry = |uid: i32, gid: i32| {
            let mut o = DynamicMessage::new(objects.kind().as_message().unwrap().clone());
            o.set_field_by_name(&b.item, PbValue::Message(item(uid, gid)));
            PbValue::Message(o)
        };
        let seller = |lots: Vec<PbValue>| {
            let mut m = DynamicMessage::new(s_md.clone());
            m.set_field_by_name(&b.objects, PbValue::List(lots));
            m
        };
        let mut added = DynamicMessage::new(a_md.clone());
        added.set_field_by_name(&b.added_item, PbValue::Message(item(987654, 11609)));
        let a = Auction::new();
        let send = |name: &str, decoded: DynamicMessage| {
            let mut m = testkit::msg(name, Some(decoded), 1.0);
            m.fmt = a.format(&m, &hub.ctx);
            a.on_message(&m, &hub.ctx);
        };
        let uids = || a.call("mine", Value::Null, &hub.ctx).unwrap().as_array().unwrap().iter().map(|l| l["uid"].as_i64().unwrap()).collect::<Vec<_>>();
        send(AH_SELLER, seller(vec![entry(123456, 11609), entry(234567, 31789)]));
        assert_eq!(uids(), vec![123456, 234567]);
        assert_eq!(a.call("mine", Value::Null, &hub.ctx).unwrap()[1]["gid"], json!(31789));
        send(AH_ADDED, added);
        assert_eq!(uids(), vec![123456, 234567, 987654]);
        send(AH_SELLER, seller(vec![entry(234567, 31789)]));
        assert_eq!(uids(), vec![234567], "the sell window gives the whole list");
    }

    #[test]
    fn listing_changes() {
        let lot = |uid: i64, prices: &[i64]| json!({"uid": uid, "prices": prices.iter().map(|p| json!({"price": p})).collect::<Vec<_>>()});
        let mut rows = vec![lot(1, &[100]), lot(2, &[5, 40, 300, 0])];
        // an equipment lot bought leaves the listing; a failed purchase or an unknown uid changes nothing
        assert!(apply_change(&mut rows, &json!({"kind": "auction_bought", "uid": 1, "bought": false})).is_none());
        assert!(apply_change(&mut rows, &json!({"kind": "auction_bought", "uid": 9, "bought": true})).is_none());
        assert!(apply_change(&mut rows, &json!({"kind": "auction_bought", "uid": 1, "bought": true})).is_some());
        assert_eq!(rows.len(), 1);
        // a resource row (several stack sizes) waits for the server's updated row
        assert!(apply_change(&mut rows, &json!({"kind": "auction_bought", "uid": 2, "bought": true})).is_none());
        apply_change(&mut rows, &json!({"kind": "auction_row", "uid": 2, "row": lot(2, &[6, 45, 0, 0])})).unwrap();
        assert_eq!(rows[0]["prices"][1]["price"], json!(45));
        apply_change(&mut rows, &json!({"kind": "auction_row_removed", "uid": 2})).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn backfills_density_of_stored_lots() {
        let Some(pack) = testkit::pack() else { return };
        let dir = testkit::temp_dir("loutil-backfill");
        let store = json!({"25222": {"gid": 25222, "updated": 0, "rows": [{"effects": [
            {"action": 125, "params": [5], "status": "normal"},
            {"action": 118, "params": [10], "status": "removed"}, // default effect missing on the lot: not counted
            {"action": 153, "params": [10], "status": "normal"},
            {"action": 176, "params": [30], "status": "normal"}]}], "density": 99.0}}); // prospection ignored; stale density replaced
        crate::save_json(&dir.join("auction/items.json"), &store).unwrap();
        let data = Arc::new(Data::load(&pack, "fr").unwrap());
        let proto = Arc::new(Proto::load(&pack, dir.join("o.json")).unwrap());
        let hub = crate::Hub::new(data, proto, dir, Arc::new(|_, _| {}), Arc::new(|_| {}));
        let items = hub.call("auction", "items", Value::Null).unwrap();
        assert_eq!(items[0]["rows"][0]["density"], json!(0.0)); // +5 vitalité (1.0) and -10 vitalité (-1.0)

        // effects stored without a group get the pack's (the UI's perfect-roll flags depend on it)
        assert_eq!(items[0]["rows"][0]["effects"][0]["group"], json!("stats"));
        assert_eq!(items[0]["rows"][0]["effects"][0]["density"], json!(0.2f32 as f64));
    }

    /// Golden test: the python formatter's output for the synthetic AH packet must equal ours.
    /// Needs the generated data pack (`python pipeline/export_pack.py`), otherwise it is skipped.
    #[test]
    fn matches_python_formatter() {
        let Some(pack) = testkit::pack() else { return };
        let root = pack.join("..");
        let data = Arc::new(Data::load(&pack, "fr").unwrap());
        let tmp = std::env::temp_dir().join("loutil-golden-overrides.json");
        let proto = Arc::new(Proto::load(&pack, tmp).unwrap());
        let events = Arc::new(Mutex::new(vec![]));
        let ev = events.clone();
        let emit: Arc<dyn Fn(&str, Value) + Send + Sync> = Arc::new(move |n, v| ev.lock().push((n.to_string(), v)));
        let hub = crate::Hub::new(data, proto, testkit::temp_dir("loutil-golden"), emit, Arc::new(|_| {}));
        hub.call("packets", "set_active", json!({"on": true})).unwrap(); // the inspector only records while its tab is open

        let payload = std::fs::read(root.join("fixtures/ah_demo.bin")).unwrap();
        hub.handle(RawMessage { t: 1.0, srv: "demo".into(), dir: "S->C", payload });

        let mut expected: Value = serde_json::from_slice(&std::fs::read(root.join("fixtures/ah_demo.expected.json")).unwrap()).unwrap();
        let events = events.lock();
        let list = hub.call("packets", "list", Value::Null).unwrap();
        let pkt = &list[0];
        assert_eq!(pkt["name"], expected["name"]);
        assert!(pkt["decoded"]["__t"].is_string(), "the inspector decodes the stored bytes when listing");
        let mut got = pkt["fmt"].clone();
        strip_keys(&mut got, &["group", "order", "density", "stat"]); // added after the python reference was written
        let mut want = expected["fmt"].take();
        strip_nulls(&mut got);
        strip_nulls(&mut want);
        assert_eq!(got, want);
        assert!(events.iter().any(|(n, _)| n == "auction:item"));
    }
}
