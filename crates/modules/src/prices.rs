//! Stored prices: the cheapest per-unit price seen for every item opened in the auction house, grouped by item
//! category / type, with a manual override that wins over the captured price.
use crate::auction::{apply_change, AH_BUY, AH_LIST, AH_ROW_REMOVED, AH_ROW_UPDATED};
use crate::kit::{arg_i64, unknown, Store};
use crate::sys::unix_secs_f64 as now;
use crate::{Appender, Ctx, Interest, Message, Module};
use anyhow::{anyhow, Result};
use loutil_data::items::RUNE_MARKER_EFFECTS;
use loutil_proto::util::{int, messages};
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

/// `itemtypes.categoryId` -> label shown in the UI.
fn category_name(id: Option<i64>) -> &'static str {
    match id {
        Some(0) => "Équipement",
        Some(1) => "Consommable",
        Some(2) => "Ressource",
        _ => "Autre",
    }
}

/// Clear name of the message carrying the average prices of the last week.
pub const AVG_PRICES: &str = "ObjectAveragePricesEvent";

/// Effects restoring life / energy (consumables of the Crafting home).
const LIFE_EFFECT: i64 = 110;
const ENERGY_EFFECT: i64 = 139;
/// Item type « Parchemin de caractéristique ».
const SCROLL_TYPE: i64 = 76;
/// itemtypes id of "Rune de transcendance".
const RUNE_TYPE: i64 = 211;

/// A price read in the auction house is forgotten after this long (seconds); the weekly average and manual overrides stay.
const UNIT_MAX_AGE: f64 = 2.0 * 86400.0;

/// Drop the AH-read price of every entry older than `UNIT_MAX_AGE`, then the entries left with no price at all.
fn expire(s: &mut BTreeMap<i64, Value>, now: f64) -> bool {
    let mut changed = false;
    for e in s.values_mut() {
        if !e["unit"].is_null() && e["updated"].as_f64().is_some_and(|t| now - t > UNIT_MAX_AGE) {
            e["unit"] = Value::Null;
            e["updated"] = Value::Null;
            changed = true;
        }
    }
    let before = s.len();
    s.retain(|_, e| !e["unit"].is_null() || !e["override"].is_null() || !e["avg"].is_null());
    changed || s.len() != before
}

/// Draconiros: the game server of the prices saved before prices were stored per server, until the player confirms theirs.
pub const DEFAULT_SERVER: i64 = 295;

/// The stored prices of every game server (they differ per server), the current one being the player's.
struct State {
    /// game server -> gid -> {gid, name, type_id, type, category, unit, updated, avg, avg_updated, override}
    servers: BTreeMap<i64, BTreeMap<i64, Value>>,
    current: i64,
    /// the server the prices of an older (single server) store were put on, until the player confirms theirs
    unconfirmed: Option<i64>,
}

impl Default for State {
    fn default() -> Self {
        Self { servers: BTreeMap::new(), current: DEFAULT_SERVER, unconfirmed: None }
    }
}

impl State {
    /// The current server's prices.
    fn cur(&mut self) -> &mut BTreeMap<i64, Value> {
        self.servers.entry(self.current).or_default()
    }

    fn to_json(&self) -> Value {
        let servers = self.servers.iter().map(|(k, m)| (k.to_string(), Value::Object(m.iter().map(|(g, e)| (g.to_string(), e.clone())).collect())));
        json!({"version": 2, "current": self.current, "unconfirmed": self.unconfirmed, "servers": Value::Object(servers.collect())})
    }

    /// A saved store; the older format (gid -> entry, before servers) becomes Draconiros' prices, unconfirmed.
    fn from_json(v: Value) -> State {
        let gids = |o: &Value| -> BTreeMap<i64, Value> {
            o.as_object().into_iter().flatten().filter(|(_, e)| e.is_object()).filter_map(|(k, e)| Some((k.parse().ok()?, e.clone()))).collect()
        };
        if v["version"] != 2 {
            let old = gids(&v);
            let mut st = State::default();
            if !old.is_empty() {
                st.servers.insert(DEFAULT_SERVER, old);
                st.unconfirmed = Some(DEFAULT_SERVER);
            }
            return st;
        }
        let servers = v["servers"].as_object().into_iter().flatten().filter_map(|(k, m)| Some((k.parse().ok()?, gids(m)))).collect();
        State { servers, current: v["current"].as_i64().unwrap_or(DEFAULT_SERVER), unconfirmed: v["unconfirmed"].as_i64() }
    }
}

/// Merge one entry into another of the same item: per price (AH-read `unit`, weekly `avg`) the newer wins; an override is kept
/// over the other's.
fn merge_entry(dst: &mut Value, src: &Value) {
    for (field, t) in [("unit", "updated"), ("avg", "avg_updated")] {
        let newer = src[t].as_f64().unwrap_or(0.0) > dst[t].as_f64().unwrap_or(0.0);
        if !src[field].is_null() && (dst[field].is_null() || newer) {
            dst[field] = src[field].clone();
            dst[t] = src[t].clone();
        }
    }
    if dst["override"].is_null() {
        dst["override"] = src["override"].clone();
    }
}

#[derive(Default)]
pub struct Prices {
    store: Store<State>,
    /// pack-derived answers, constant once the pack is loaded
    runes: OnceLock<Value>,
    craft_runes: OnceLock<Value>,
    by_name: OnceLock<HashMap<String, i64>>,
    /// gid -> rows of the last listing seen this session, to follow its changes (purchases, updated / removed rows)
    listings: Mutex<HashMap<i64, Vec<Value>>>,
    /// `<app data>/prices/history.jsonl`, opened on first use (see `append_history`)
    history: OnceLock<Appender>,
}

/// Per-unit price of a listing: the cheapest price per unit of each stack size on sale (x1, x10, x100, x1000), then the
/// median of those (the lower one of the two middle values for an even count: an actual offer, and one absurd stack —
/// a x1000 at 5x the others — cannot set it). Equipment (one stack size) = its cheapest lot. None when nobody sells it.
fn unit_price(rows: &[Value]) -> Option<f64> {
    let mut by_qty: Vec<(f64, f64)> = vec![]; // (qty, cheapest unit price)
    for p in rows.iter().flat_map(|r| r["prices"].as_array().into_iter().flatten()) {
        let (Some(q), Some(x)) = (p["qty"].as_f64().filter(|q| *q > 0.0), p["price"].as_f64().filter(|x| *x > 0.0)) else { continue };
        match by_qty.iter_mut().find(|(k, _)| *k == q) {
            Some(e) => e.1 = e.1.min(x / q),
            None => by_qty.push((q, x / q)),
        }
    }
    let mut units: Vec<f64> = by_qty.into_iter().map(|(_, u)| u).collect();
    units.sort_by(f64::total_cmp);
    units.get(units.len().saturating_sub(1) / 2).copied()
}

/// Every offer of a listing as `[qty, price]`, sorted, for the price history.
fn offers(rows: &[Value]) -> Vec<[i64; 2]> {
    let mut v: Vec<[i64; 2]> = rows
        .iter()
        .flat_map(|r| r["prices"].as_array().into_iter().flatten())
        .filter_map(|p| Some([p["qty"].as_i64().filter(|q| *q > 0)?, p["price"].as_i64().filter(|x| *x > 0)?]))
        .collect();
    v.sort_unstable();
    v
}

/// Price history: every price change seen, one JSON line per event with its time `t` (unix seconds), never rewritten, read
/// back only on demand (`history`), kept for analyses to come. AH listing / change: {t, gid, kind: "ah", unit, offers:
/// [[qty, price]]}; weekly averages: {t, kind: "avg", prices: {gid: price}}; manual price: {t, gid, kind: "override", value}
/// (null = removed). Queued in memory and appended in the background (the capture thread does no disk IO).
fn history<'a>(cell: &'a OnceLock<Appender>, ctx: &Ctx) -> &'a Appender {
    cell.get_or_init(|| Appender::open(ctx.module_dir("prices").join("history.jsonl"), ctx.logger()))
}

/// History lines of one item (or every line when `gid` is None) at or after `since`; averages are reduced to that item's price.
fn read_history(path: &std::path::Path, gid: Option<i64>, since: f64) -> Vec<Value> {
    use std::io::BufRead;
    let Ok(f) = std::fs::File::open(path) else { return vec![] };
    std::io::BufReader::new(f)
        .lines()
        .map_while(Result::ok)
        .filter_map(|l| serde_json::from_str::<Value>(&l).ok())
        .filter(|v| v["t"].as_f64().unwrap_or(0.0) >= since)
        .filter_map(|mut v| match gid {
            None => Some(v),
            Some(g) if v["kind"] == "avg" => {
                let p = v["prices"].get(g.to_string())?.clone();
                Some(json!({"t": v["t"], "gid": g, "kind": "avg", "avg": p}))
            }
            Some(g) => (v["gid"] == g).then(|| v.take()),
        })
        .collect()
}

/// Case / nbsp / apostrophe / accent insensitive item name, for `gids_by_name`.
fn norm_name(s: &str) -> String {
    s.replace('\u{a0}', " ")
        .replace('\u{2019}', "'")
        .trim()
        .to_lowercase()
        .chars()
        .flat_map(|c| -> Vec<char> {
            match c {
                'é' | 'è' | 'ê' | 'ë' => vec!['e'],
                'à' | 'â' | 'ä' => vec!['a'],
                'î' | 'ï' => vec!['i'],
                'ô' | 'ö' => vec!['o'],
                'ù' | 'û' | 'ü' => vec!['u'],
                'ç' => vec!['c'],
                'œ' => vec!['o', 'e'],
                c => vec![c],
            }
        })
        .collect()
}

/// Apply a listing change (`auction_bought` / `auction_row` / `auction_row_removed`) to the stored listings; the gid of the
/// listing changed, None when nothing changed. The row is found by uid in any listing, else (a row change carries its gid)
/// in that gid's listing: a resource listing's single row is replaced / removed, another row is added. A resource row seen
/// with no listing of its item starts one (it holds the cheapest offer of every stack size); an equipment row doesn't (one
/// lot says nothing of the cheapest).
fn change_listing(listings: &mut HashMap<i64, Vec<Value>>, f: &Value, is_equipment: impl Fn(i64) -> bool) -> Option<i64> {
    let has_uid = |r: &Vec<Value>| !f["uid"].is_null() && r.iter().any(|x| x["uid"] == f["uid"]);
    if let Some((gid, rows)) = listings.iter_mut().find(|(_, r)| has_uid(r)) {
        return apply_change(rows, f).map(|_| *gid);
    }
    let kind = f["kind"].as_str()?;
    if kind != "auction_row" && kind != "auction_row_removed" {
        return None;
    }
    let gid = f["gid"].as_i64()?;
    let equipment = is_equipment(gid);
    match (listings.get_mut(&gid), kind) {
        (Some(rows), "auction_row") if rows.len() == 1 && !equipment => rows[0] = f["row"].clone(),
        (Some(rows), "auction_row") => rows.push(f["row"].clone()),
        (Some(rows), _) if rows.len() == 1 && !equipment => rows.clear(),
        (None, "auction_row") if !equipment => {
            listings.insert(gid, vec![f["row"].clone()]);
        }
        _ => return None,
    }
    Some(gid)
}

impl Prices {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fill the display fields from the data pack (names may change with the pack, so they are never trusted from disk).
    fn describe(ctx: &Ctx, gid: i64, e: &mut Value) {
        let type_id = ctx.data.items.get(&gid).map(|i| i.type_id);
        e["name"] = json!(ctx.data.item_name(gid));
        e["type_id"] = json!(type_id);
        e["type"] = json!(ctx.data.type_name(type_id));
        e["category"] = json!(category_name(type_id.and_then(|t| ctx.data.itemtypes.get(&t)).and_then(|t| t.category_id)));
    }
}

impl Prices {
    /// Average per-unit prices of the last week: preloads the store. They only fill `avg`; prices read in the auction
    /// house (`unit`) and manual ones (`override`) always win over it.
    fn on_average_prices(&self, m: &Message, ctx: &Ctx) {
        let (Some(msg), Some(b)) = (m.decoded.as_ref(), ctx.data.bindings.get("avg_prices")) else { return };
        let field = |k: &str| b[k].as_str().unwrap_or_default();
        // two int64 per entry that cannot be told apart after an update: the price and one never set (seen on captures)
        let prices: Vec<&str> = b["prices"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        let mut changed = vec![];
        let mut hist = serde_json::Map::new();
        let mut st = self.store.lock();
        let srv = st.current;
        let s = st.cur();
        for e in messages(msg, field("items")) {
            let (gid, price) = (int(&e, field("gid")), prices.iter().map(|f| int(&e, f)).max().unwrap_or(0));
            if price <= 0 || !ctx.data.items.contains_key(&gid) {
                continue;
            }
            let ent = s.entry(gid).or_insert_with(|| json!({"gid": gid, "override": null, "unit": null}));
            ent["avg"] = json!(price);
            ent["avg_updated"] = json!(m.raw.t);
            Self::describe(ctx, gid, ent);
            changed.push(ent.clone());
            hist.insert(gid.to_string(), json!(price));
        }
        drop(st);
        if changed.is_empty() {
            return;
        }
        history(&self.history, ctx).push(json!({"t": m.raw.t, "srv": srv, "kind": "avg", "prices": hist}).to_string());
        self.store.touch();
        ctx.emit("prices", "bulk", Value::Array(changed));
    }
}

impl Module for Prices {
    fn id(&self) -> &'static str {
        "prices"
    }

    fn interest(&self) -> Interest {
        Interest::Types(&[AH_LIST, AH_BUY, AH_ROW_UPDATED, AH_ROW_REMOVED, AVG_PRICES])
    }

    fn init(&self, ctx: &Ctx) {
        let saved = self.store.open(ctx, "prices", "prices.json", State::to_json);
        if let Some(saved) = saved {
            let mut st = State::from_json(saved);
            let mut changed = st.unconfirmed.is_some(); // an older store: written again in the new format
            for m in st.servers.values_mut() {
                for (gid, v) in m.iter_mut() {
                    Self::describe(ctx, *gid, v);
                }
                changed |= expire(m, now());
            }
            *self.store.lock() = st;
            if changed {
                self.store.touch();
            }
        }
    }

    fn on_message(&self, m: &Message, ctx: &Ctx) {
        if m.name == AVG_PRICES {
            return self.on_average_prices(m, ctx);
        }
        let Some(f) = m.fmt.as_ref() else { return };
        if m.raw.srv == "demo" {
            return;
        }
        let (gid, unit, offers) = if f["kind"] == "auction_items" {
            let Some(rows) = f["rows"].as_array() else { return };
            let Some(gid) = f["gid"].as_i64().or_else(|| rows.first().and_then(|r| r["gid"].as_i64())) else { return };
            self.listings.lock().insert(gid, rows.clone());
            (gid, unit_price(rows), offers(rows))
        } else {
            // a purchase / an updated or removed row of a listing
            let mut l = self.listings.lock();
            let Some(gid) = change_listing(&mut l, f, |g| ctx.data.is_equipment(g)) else { return };
            let rows = &l[&gid];
            (gid, unit_price(rows), offers(rows))
        };
        // recorded even when nobody sells it any more (empty offers)
        let srv = self.store.lock().current;
        let line = json!({"t": m.raw.t, "srv": srv, "gid": gid, "kind": "ah", "unit": unit.map(|u| u.round() as i64), "offers": offers});
        history(&self.history, ctx).push(line.to_string());
        let Some(unit) = unit else { return }; // nobody is selling it any more: keep the previous price
        let mut st = self.store.lock();
        let ent = st.cur().entry(gid).or_insert_with(|| json!({"gid": gid, "override": null}));
        ent["unit"] = json!(unit.round() as i64);
        ent["updated"] = json!(m.raw.t);
        Self::describe(ctx, gid, ent);
        let ent = ent.clone();
        drop(st);
        self.store.touch();
        ctx.emit("prices", "price", ent);
    }

    fn call(&self, method: &str, args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            "list" => {
                let mut st = self.store.lock();
                let s = st.cur();
                let changed = expire(s, now());
                let out = Value::Array(s.values().cloned().collect());
                drop(st);
                if changed {
                    self.store.touch();
                }
                Ok(out)
            }
            // Rune de transcendance items: [{gid, name, a, value}] (the effect a rune adds to an item)
            "runes" => Ok(self
                .runes
                .get_or_init(|| {
                    let mut v: Vec<Value> = ctx
                        .data
                        .items
                        .iter()
                        .filter(|(_, i)| i.type_id == RUNE_TYPE)
                        .filter_map(|(g, i)| {
                            let e = i.effects.iter().find(|e| !RUNE_MARKER_EFFECTS.contains(&e.a))?;
                            Some(json!({"gid": g, "name": ctx.data.item_name(*g), "a": e.a, "value": e.max}))
                        })
                        .collect();
                    v.sort_by_key(|x| x["gid"].as_i64());
                    Value::Array(v)
                })
                .clone()),
            // Every craftable rune (forgemagie type 78 and transcendance type 211): [{gid, name, effect, transcend}]
            "craft_runes" => Ok(self
                .craft_runes
                .get_or_init(|| {
                    let mut v: Vec<Value> = ctx
                        .data
                        .items
                        .iter()
                        .filter(|(g, i)| (i.type_id == 78 || i.type_id == RUNE_TYPE) && ctx.data.recipes.contains_key(g))
                        .map(|(g, i)| {
                            // the effect the rune grants, rendered like in game ("+10 Force")
                            let effect = i.effects.iter().find(|e| !RUNE_MARKER_EFFECTS.contains(&e.a)).map(|e| ctx.data.render_effect(e.a, &[json!(e.max)]).0);
                            json!({"gid": g, "name": ctx.data.item_name(*g), "effect": effect, "transcend": i.type_id == RUNE_TYPE})
                        })
                        .collect();
                    v.sort_by_key(|x| x["gid"].as_i64());
                    Value::Array(v)
                })
                .clone()),
            // args: {names: [string]} -> {name: gid} (case / nbsp / apostrophe insensitive; lowest gid wins on duplicates); used by
            // Crafting > Montures and lib/mounts.ts (mounts and neurones are named in the UI code)
            "gids_by_name" => {
                let by = self.by_name.get_or_init(|| {
                    let mut by: HashMap<String, i64> = HashMap::new();
                    for g in ctx.data.items.keys() {
                        by.entry(norm_name(&ctx.data.item_name(*g))).and_modify(|x| *x = (*x).min(*g)).or_insert(*g);
                    }
                    by
                });
                let mut out = serde_json::Map::new();
                for n in args["names"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                    if let Some(g) = by.get(&norm_name(n)) {
                        out.insert(n.to_string(), json!(g));
                    }
                }
                Ok(Value::Object(out))
            }
            // Crafting home cards: {consumables: [{gid, name, level, type, life, energy}] (items restoring life, effect 110, or
            // energy, 139: mean of their roll), scrolls: [{gid, name}] (characteristic scrolls, type 76)}
            "home_items" => {
                let mean = |e: &loutil_data::DefaultEffect| if e.max > e.min { (e.min + e.max) as f64 / 2.0 } else { e.min as f64 };
                let mut consumables = vec![];
                let mut scrolls = vec![];
                for (g, i) in &ctx.data.items {
                    if i.type_id == SCROLL_TYPE {
                        scrolls.push(json!({"gid": g, "name": ctx.data.item_name(*g)}));
                    }
                    let restores = |a: i64| i.effects.iter().filter(|e| e.a == a).map(mean).sum::<f64>();
                    let (life, energy) = (restores(LIFE_EFFECT), restores(ENERGY_EFFECT));
                    if life > 0.0 || energy > 0.0 {
                        let name = ctx.data.item_name(*g);
                        consumables.push(
                            json!({"gid": g, "name": name, "level": i.level, "type": ctx.data.type_name(Some(i.type_id)), "life": life, "energy": energy}),
                        );
                    }
                }
                Ok(json!({"consumables": consumables, "scrolls": scrolls}))
            }
            // args: {gids: [gid]} -> {gid: name} (pack names; unknown gids left out)
            "item_names" => {
                let gids = args["gids"].as_array().into_iter().flatten().filter_map(Value::as_i64);
                Ok(Value::Object(gids.filter(|g| ctx.data.items.contains_key(g)).map(|g| (g.to_string(), json!(ctx.data.item_name(g)))).collect()))
            }
            // args: {gid?, since?} -> history lines, oldest first (not used by the UI yet)
            "history" => {
                let h = history(&self.history, ctx);
                h.flush();
                Ok(Value::Array(read_history(h.path(), args["gid"].as_i64(), args["since"].as_f64().unwrap_or(0.0))))
            }
            // args: {gid, value: number | null}; null removes the override
            "set_override" => {
                let gid = arg_i64(&args, "gid")?;
                let value = args["value"].as_f64().filter(|v| *v >= 0.0).map(|v| v.round() as i64);
                let mut st = self.store.lock();
                let srv = st.current;
                let ent = st.cur().get_mut(&gid).ok_or_else(|| anyhow!("prices: unknown item {gid}"))?;
                ent["override"] = json!(value);
                let ent = ent.clone();
                drop(st);
                history(&self.history, ctx).push(json!({"t": now(), "srv": srv, "gid": gid, "kind": "override", "value": value}).to_string());
                self.store.touch();
                ctx.emit("prices", "price", ent.clone());
                Ok(ent)
            }
            // args: {id, confirm?} -> {server, moved}: the game server whose prices are used (Paramètres « Serveur de jeu »);
            // `confirm` (first-start modal) also moves the prices of an older store, put on Draconiros, to that server
            "set_server" => {
                let id = arg_i64(&args, "id")?;
                let mut st = self.store.lock();
                let mut moved = 0;
                let confirm = args["confirm"].as_bool() == Some(true);
                if confirm {
                    if let Some(from) = st.unconfirmed.take().filter(|f| *f != id) {
                        let old = st.servers.remove(&from).unwrap_or_default();
                        moved = old.len();
                        let dst = st.servers.entry(id).or_default();
                        for (gid, e) in old {
                            match dst.get_mut(&gid) {
                                Some(d) => merge_entry(d, &e),
                                None => {
                                    dst.insert(gid, e);
                                }
                            }
                        }
                    }
                }
                let switched = st.current != id;
                st.current = id;
                let list = Value::Array(st.cur().values().cloned().collect());
                drop(st);
                if confirm || switched {
                    self.store.touch();
                }
                if switched || moved > 0 {
                    self.listings.lock().clear();
                    ctx.emit("prices", "cleared", json!({}));
                    ctx.emit("prices", "bulk", list);
                }
                Ok(json!({"server": id, "moved": moved}))
            }
            // -> {server, unconfirmed}: the current game server, and the one older prices wait on until the player confirms theirs
            "server" => {
                let st = self.store.lock();
                Ok(json!({"server": st.current, "unconfirmed": st.unconfirmed}))
            }
            // -> [{id, name, type}] the game servers of the pack (none in an older pack)
            "servers" => {
                let mut v: Vec<Value> = ctx
                    .data
                    .servers
                    .iter()
                    .map(|(id, s)| json!({"id": id, "name": ctx.data.text(s.name_id), "type": s.type_name_id.and_then(|t| ctx.data.text(t))}))
                    .collect();
                v.sort_by_key(|s| s["id"].as_i64());
                Ok(Value::Array(v))
            }
            "forget" => {
                self.store.update(|s| s.cur().clear());
                ctx.emit("prices", "cleared", json!({}));
                Ok(json!({"ok": true}))
            }
            _ => Err(unknown("prices", method)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::testkit::{self, hub, msg};
    use loutil_core::RawMessage;

    /// The Crafting home's items: Pain d'Incarnam restores 12 life, Borodinski 390 energy, the characteristic scrolls are listed.
    #[test]
    fn home_items_from_the_pack() {
        let Some((hub, _)) = hub("loutil-prices-home") else { return };
        let h = hub.call("prices", "home_items", Value::Null).unwrap();
        let find = |gid: i64| h["consumables"].as_array().unwrap().iter().find(|c| c["gid"] == gid).cloned().unwrap();
        assert_eq!((find(468)["life"].as_f64(), find(468)["energy"].as_f64()), (Some(12.0), Some(0.0)));
        assert_eq!(find(520)["energy"].as_f64(), Some(390.0));
        let scrolls = h["scrolls"].as_array().unwrap();
        assert!(scrolls.iter().any(|s| s["name"] == "Puissant Parchemin de Force") && scrolls.len() >= 24);
    }

    #[test]
    fn unit_price_is_the_median_stack() {
        let row = |p: &[(i64, i64)]| json!({"prices": p.iter().map(|(q, x)| json!({"qty": q, "price": x})).collect::<Vec<_>>()});
        // per unit 300 / 250 / 240 / 5,000: the overpriced x1000 is ignored -> lower of the middle two (250, 300) = 250
        assert_eq!(unit_price(&[row(&[(1, 300), (10, 2500), (100, 24000), (1000, 5_000_000)])]), Some(250.0));
        // three sizes, one without a price -> median of 3
        assert_eq!(unit_price(&[row(&[(1, 300), (10, 0), (100, 20000), (1000, 150_000)])]), Some(200.0));
        // equipment: one lot per row, all x1 -> the cheapest
        assert_eq!(unit_price(&[row(&[(1, 900)]), row(&[(1, 700)]), row(&[(1, 800)])]), Some(700.0));
        assert_eq!(unit_price(&[row(&[(1, 0)])]), None);
    }

    #[test]
    fn average_prices_preload_but_never_override() {
        let Some((hub, proto)) = hub("loutil-avgprices") else { return };
        // two entries: gid 1711 -> 5000, gid 1728 -> 300 (third field unused)
        let payload = vec![0x1a, 0x06, 0x08, 0xaf, 0x0d, 0x10, 0x88, 0x27, 0x1a, 0x08, 0x08, 0xc0, 0x0d, 0x10, 0xac, 0x02, 0x18, 0x01];
        let decoded = proto.decode("isr", &payload);
        let msg = |t| msg(AVG_PRICES, decoded.clone(), t);
        let p = Prices::new();
        p.on_message(&msg(1.0), &hub.ctx);
        let list = p.call("list", Value::Null, &hub.ctx).unwrap();
        let get = |l: &Value, gid: i64| l.as_array().unwrap().iter().find(|e| e["gid"] == gid).cloned().unwrap();
        assert_eq!(get(&list, 1711)["avg"], 5000);
        assert_eq!(get(&list, 1728)["avg"], 300);
        assert!(get(&list, 1711)["unit"].is_null());
        // the averages are appended to the history, reduced to the asked item
        let h = p.call("history", json!({"gid": 1728}), &hub.ctx).unwrap();
        assert_eq!(h, json!([{"t": 1.0, "gid": 1728, "kind": "avg", "avg": 300}]));
        // a manual override survives a later average
        p.call("set_override", json!({"gid": 1711, "value": 42}), &hub.ctx).unwrap();
        let h = p.call("history", json!({"gid": 1711}), &hub.ctx).unwrap();
        assert_eq!((h[1]["kind"].as_str(), h[1]["value"].as_i64()), (Some("override"), Some(42)));
        p.on_message(&msg(2.0), &hub.ctx);
        let list = p.call("list", Value::Null, &hub.ctx).unwrap();
        assert_eq!(get(&list, 1711)["override"], 42);
    }

    /// An older store (gid -> entry) becomes Draconiros' prices, unconfirmed; confirming another server moves them there, and
    /// switching servers switches the prices listed.
    #[test]
    fn prices_per_game_server() {
        let old = json!({"1711": {"gid": 1711, "unit": 100, "updated": now(), "override": null}});
        let st = State::from_json(old);
        assert_eq!((st.current, st.unconfirmed, st.servers[&DEFAULT_SERVER].len()), (DEFAULT_SERVER, Some(DEFAULT_SERVER), 1));
        let again = State::from_json(st.to_json());
        assert_eq!((again.unconfirmed, again.servers[&DEFAULT_SERVER][&1711]["unit"].clone()), (Some(DEFAULT_SERVER), json!(100)));
        // the fresh store of a new install: nothing to move
        assert_eq!(State::from_json(json!({})).unconfirmed, None);
        // merging keeps the newer price of each kind and the override
        let mut d = json!({"unit": 10, "updated": 1.0, "avg": 50, "avg_updated": 9.0, "override": 7});
        merge_entry(&mut d, &json!({"unit": 20, "updated": 2.0, "avg": 40, "avg_updated": 3.0, "override": null}));
        assert_eq!((d["unit"].clone(), d["avg"].clone(), d["override"].clone()), (json!(20), json!(50), json!(7)));

        let Some((hub, _)) = hub("loutil-prices-servers") else { return };
        let p = Prices::new();
        *p.store.lock() = again;
        let gids = |p: &Prices| p.call("list", Value::Null, &hub.ctx).unwrap().as_array().unwrap().iter().map(|e| e["gid"].clone()).collect::<Vec<_>>();
        let r = p.call("set_server", json!({"id": 291, "confirm": true}), &hub.ctx).unwrap();
        assert_eq!(r["moved"], 1);
        assert_eq!(gids(&p), vec![json!(1711)]);
        assert_eq!(p.call("server", Value::Null, &hub.ctx).unwrap(), json!({"server": 291, "unconfirmed": null}));
        p.call("set_server", json!({"id": DEFAULT_SERVER}), &hub.ctx).unwrap();
        assert!(gids(&p).is_empty());
        let servers = p.call("servers", Value::Null, &hub.ctx).unwrap();
        assert!(servers.as_array().unwrap().iter().any(|s| s["id"] == DEFAULT_SERVER && s["name"] == "Draconiros"));
    }

    #[test]
    fn listing_changes_follow_purchases() {
        let row = |uid: i64, gid: i64, p: &[i64]| json!({"uid": uid, "gid": gid, "prices": p.iter().zip([1, 10, 100, 1000]).map(|(p, q)| json!({"qty": q, "price": p})).collect::<Vec<_>>()});
        let change = |kind: &str, uid: i64, gid: i64, r: Value| json!({"kind": kind, "uid": uid, "gid": gid, "row": r});
        let equip = |g: i64| g == 9;
        let mut l: HashMap<i64, Vec<Value>> = HashMap::new();
        l.insert(5, vec![row(50, 5, &[100, 900, 8000, 0])]);
        // same uid: replaced
        let r = row(50, 5, &[120, 900, 8000, 0]);
        assert_eq!(change_listing(&mut l, &change("auction_row", 50, 5, r), equip), Some(5));
        assert_eq!(unit_price(&l[&5]), Some(90.0));
        // another uid for the resource's single row (uid not the listing's): replaced through the gid
        let r = row(51, 5, &[150, 1200, 8000, 0]);
        assert_eq!(change_listing(&mut l, &change("auction_row", 51, 5, r), equip), Some(5));
        assert_eq!(l[&5].len(), 1);
        assert_eq!(unit_price(&l[&5]), Some(120.0));
        // a resource never listed this session: its row starts the listing; an equipment lot doesn't
        assert_eq!(change_listing(&mut l, &change("auction_row", 70, 7, row(70, 7, &[30, 0, 0, 0])), equip), Some(7));
        assert_eq!(unit_price(&l[&7]), Some(30.0));
        assert_eq!(change_listing(&mut l, &change("auction_row", 90, 9, row(90, 9, &[5000, 0, 0, 0])), equip), None);
        // the resource's last row removed (unknown uid): the listing empties, no price
        assert_eq!(change_listing(&mut l, &json!({"kind": "auction_row_removed", "uid": 77, "gid": 7}), equip), Some(7));
        assert_eq!(unit_price(&l[&7]), None);
        // a purchase answer without a known uid changes nothing
        assert_eq!(change_listing(&mut l, &json!({"kind": "auction_bought", "uid": 1, "bought": true}), equip), None);
    }

    /// work/capture_hdv_buy.jsonl (recorder « hdv », 3.7.13.17): ten purchases of x1 Rune Tac (20819) at 137 K; each answered by
    /// ExchangeBidHouseInListUpdatedEvent (the listing's row, same uid 73940) then ExchangeBidHouseBuyResultEvent; the last one
    /// leaves the x1 at 138 K.
    #[test]
    fn replays_a_recorded_purchase() {
        let Some(pack) = testkit::pack() else { return };
        let rec = pack.join("../work/capture_hdv_buy.jsonl");
        let Ok(text) = std::fs::read_to_string(&rec) else { return };
        let (hub, _) = testkit::hub("loutil-prices-buy").unwrap();
        let hex = |s: &str| (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect::<Vec<u8>>();
        let unit = || {
            let l = hub.call("prices", "list", Value::Null).unwrap();
            l.as_array().unwrap().iter().find(|e| e["gid"] == 20819).map(|e| e["unit"].clone())
        };
        let mut seen = vec![];
        // replayed as if it happened now: prices read in the auction house expire after UNIT_MAX_AGE
        let mut shift = None;
        for line in text.lines() {
            let r: Value = serde_json::from_str(line).unwrap();
            let Some(h) = r["hex"].as_str() else { continue };
            let dir = if r["dir"] == "C->S" { "C->S" } else { "S->C" };
            let t = r["t"].as_f64().unwrap();
            let shift = *shift.get_or_insert(now() - t);
            hub.handle(RawMessage { t: t + shift, srv: r["srv"].as_str().unwrap().into(), dir, payload: hex(h) });
            if r["name"] == "ExchangeBidHouseInListUpdatedEvent" {
                seen.push(unit());
            }
        }
        // per unit: median of x1 137 / x10 131 / x100 160 = 137, then 138 once the x1 went up
        assert_eq!(seen.len(), 10);
        assert_eq!(seen.first().unwrap(), &Some(json!(137)));
        assert_eq!(seen.last().unwrap(), &Some(json!(138)));
    }

    #[test]
    fn gids_by_name_finds_foods() {
        let Some((hub, _)) = hub("loutil-gids") else { return };
        let r = Prices::new().call("gids_by_name", json!({"names": ["Croquette enrichie", "Queue d'Ecumouth", "Eau calme"]}), &hub.ctx).unwrap();
        println!("{r}");
        assert_eq!(r["Croquette enrichie"], 20125);
        assert!(r.as_object().unwrap().len() >= 2);
    }

    #[test]
    fn item_names_by_gid() {
        let Some((hub, _)) = hub("loutil-names") else { return };
        let r = Prices::new().call("item_names", json!({"gids": [20125, -1]}), &hub.ctx).unwrap();
        assert_eq!(r, json!({"20125": "Croquette enrichie"}));
    }
}
