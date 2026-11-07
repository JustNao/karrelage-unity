//! Forgemagie: the equipment piece put in a forgemagie workshop and each rune's chances on it, for the overlay drawn over the
//! game's window (app/src/ForgemagieOverlay.svelte). The item comes with `ExchangeObjectsAddedEvent` (named by anchor) when it
//! is put in, `ExchangeObjectsModifiedEvent` / `ExchangeCraftResultEvent` after each rune; it goes with
//! `ExchangeObjectRemovedEvent` / `ExchangeObjectsRemovedEvent` (its uid) or `ExchangeLeaveEvent`. Their fields are read by
//! type: the lists of ObjectItemInventory, the result's ObjectItem holding an equipment piece.
//!
//! Chances = the players' model of dofocus.fr (`odds`, a port of its simulator, not Ankama's formula), checked against its own
//! code on the cases of the tests. Lines are keyed by characteristic like there: "+Force" and "-Force" are one signed line.
//!
//! Event `forgemagie:item` {uid, gid, name, level, lines: [{stat, value, min?, max?, runes: [{gid, name, points, odds:
//! {sc, sn, ec} (%) | null = refused}]}]} or null. The lines come in the item's own effect order, then its default effects it
//! no longer has (value 0); runes per line from the smallest (the game's Rune / Pa / Ra columns). Not checked on a capture yet.
use crate::auction::{parse_effect, stat_effects, ObjectEffect};
use crate::inventory::{entry_item, object_ints, Bindings};
use crate::kit::{unknown, Role};
use crate::{Ctx, Interest, Message, Module};
use anyhow::Result;
use loutil_data::Data;
use loutil_proto::util::{int_list, scalar_ints};
use loutil_proto::{DynamicMessage, Kind, ReflectMessage};
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

const ID: &str = "forgemagie";
const ADDED: &str = "ExchangeObjectsAddedEvent";
const MODIFIED: &str = "ExchangeObjectsModifiedEvent";
const RESULT: &str = "ExchangeCraftResultEvent";
const REMOVED: &str = "ExchangeObjectRemovedEvent";
const REMOVED_LIST: &str = "ExchangeObjectsRemovedEvent";
const LEAVE: &str = "ExchangeLeaveEvent";
/// Forgemagie runes' item type.
const RUNE_TYPE: i64 = 78;
/// Rune-typed items that are no forgemagie rune (brisage.rs `NO_RUNE_EFFECTS`).
const NO_RUNE_EFFECTS: [i64; 1] = [795];

// ---- the odds (dofocus.fr's simulator, same names for its constants where they help reading it against the original) ----

/// Weight an item can carry above its lines' maxima, and a line's own cap in weight (`Ez`).
const MAX_OVER: f64 = 101.0;
const EPS: f64 = 1e-9;
/// Characteristics: AP, MP, range (exo regime: a fixed 1.5 % critical success; an AP / MP exo on the item makes it count as perfect).
const AP: i64 = 1;
const MP: i64 = 23;
const RANGE: i64 = 19;

/// One line of the item: a characteristic, its signed value, its default range (none = exo) and its weight per point.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub stat: i64,
    pub value: f64,
    pub range: Option<(f64, f64)>,
    pub weight: f64,
}

/// Chances of a rune (%): critical success, neutral success, critical failure.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Odds {
    pub sc: f64,
    pub sn: f64,
    pub ec: f64,
}

fn bounds(l: &Line) -> Option<(f64, f64)> {
    l.range.map(|(a, b)| (a.min(b), a.max(b)))
}

/// Weight the item carries above its lines' maxima (exos whole, negative lines' maxima at half) (`Jp`).
fn over_weight<'a>(lines: impl IntoIterator<Item = &'a Line>) -> f64 {
    lines
        .into_iter()
        .filter(|l| l.weight > 0.0 && l.value > 0.0)
        .map(|l| match bounds(l) {
            None => l.value * l.weight,
            Some((_, hi)) if hi < 0.0 => l.value * l.weight - hi * l.weight * 0.5,
            Some((_, hi)) => (l.value - hi).max(0.0) * l.weight,
        })
        .sum()
}

/// How close the other lines are to their maxima, by weight (0..1) (`cqe`).
fn quality(lines: &[Line], skip: Option<usize>) -> f64 {
    let (mut have, mut max) = (0.0, 0.0);
    for (i, l) in lines.iter().enumerate() {
        let Some((_, hi)) = bounds(l) else { continue };
        if Some(i) == skip || l.weight <= 0.0 || hi < 0.0 {
            continue;
        }
        have += l.weight * l.value.max(0.0).min(hi);
        max += l.weight * hi;
    }
    if max > 0.0 {
        have / max
    } else {
        0.0
    }
}

/// Difficulty of a rune on a line with a range (`uqe` - `Cz`).
#[allow(clippy::too_many_arguments)]
fn difficulty(position: f64, over: bool, over_w: f64, ratio: f64, power: f64, quality: f64, level: i64, step: bool) -> f64 {
    let mut c = 5.0 + 4.0 * power * (ratio - 14.0).max(0.0) + 60.0 * quality.powi(12);
    if over {
        c += 65.0 + 4.0 * over_w.max(0.0);
    } else if step && position >= 0.8 - EPS {
        c += 45.0;
    }
    let low = if level < 170 { 20.0 * (1.1 - quality).max(0.0) } else { 0.0 } + if level < 100 { 15.0 } else { 0.0 };
    c - low
}

/// Chances from a difficulty (`dqe`).
fn from_difficulty(e: f64, over: bool) -> Odds {
    let sc = 2f64.max((100.0 - e) / 2.0);
    let sn = if over {
        5f64.max(40.0 - 0.375 * (e - 100.0))
    } else if e <= 100.0 {
        50f64.min(100.0 - sc)
    } else {
        50.0 - 0.375 * (e - 100.0)
    };
    let sn = sn.max(0.0).min(100.0 - sc);
    Odds { sc, sn, ec: 100.0 - sc - sn }
}

/// Chances of an exo (a rune on a line the item has no range for), from the line's weight once added (`fqe`).
fn exo(stat: i64, weight: f64, quality: f64, level: i64) -> Odds {
    if [AP, MP, RANGE].contains(&stat) {
        return Odds { sc: 1.5, sn: 0.0, ec: 98.5 };
    }
    let mut i = if weight <= 2.0 {
        57.0
    } else if weight <= 15.0 {
        57.0 - (weight - 2.0) * 38.0 / 13.0
    } else if weight < 25.0 {
        19.0 - (weight - 15.0) * 1.9
    } else {
        0.0
    };
    if level < 100 && weight <= 50.0 {
        i = i.max(8.0 + (3.0 - 8.0) * (weight - 25.0) / 25.0);
    }
    if i <= 0.0 {
        return Odds { sc: 1.0, sn: 0.0, ec: 99.0 };
    }
    let a = (0.077 * i).max(1.0) + 0.923 * i;
    let o = a / (100.0 - a) * (1.2 * (0.85 - quality)).exp();
    let l = (100.0 * o / (1.0 + o)).max(1.0);
    let c = (0.077 * l).max(1.0);
    Odds { sc: c, sn: l - c, ec: 100.0 - l }
}

/// A rune of `points` points of characteristic `stat` weighing `weight` in all, on an item of this level with these lines (`Pz`).
/// None: the game refuses it (the line or the item would go past its cap).
pub fn odds(lines: &[Line], level: i64, stat: i64, points: f64, weight: f64) -> Option<Odds> {
    let r = lines.iter().position(|l| l.stat == stat);
    let line = r.map(|i| &lines[i]);
    let per = weight / points;
    let o = line.map_or(0.0, |l| l.value);
    let cap = ((MAX_OVER / per + EPS).floor()).max(line.and_then(bounds).map_or(f64::MIN, |(_, hi)| hi));
    if o + points > cap {
        return None;
    }
    let mut after = lines.to_vec();
    match r {
        Some(i) => after[i].value = o + points,
        None => after.push(Line { stat, value: points, range: None, weight: per }),
    }
    if over_weight(&after) > MAX_OVER + EPS {
        return None;
    }
    let q = quality(lines, r);
    let Some((line, (lo, hi))) = line.and_then(|l| Some((l, bounds(l)?))) else {
        return Some(exo(stat, (o + points) * per, q, level));
    };
    let over = o + points > hi;
    if over {
        let others = over_weight(lines.iter().filter(|l| !std::ptr::eq(*l, line)));
        let own = if hi < 0.0 { over_weight([&after[r.unwrap()]]) } else { (o + points) * per };
        if own + others > MAX_OVER + EPS {
            return None;
        }
    }
    let fixed = hi == lo;
    let position = if fixed { 1.0 } else { (o + points - lo) / (hi - lo) };
    let exo_ap_mp = lines.iter().any(|l| (l.stat == AP || l.stat == MP) && l.range.is_none() && l.value > 0.0);
    let q = if exo_ap_mp { 1.0 } else { q };
    let over_w = if over { (o + points - hi) * per } else { 0.0 };
    let e = difficulty(position, over, over_w, o / points, weight, q, level, !fixed);
    Some(from_difficulty(e, over))
}

// ---- the item in the workshop ----

/// A rune: item gid and what it adds.
#[derive(Clone, Debug)]
struct Rune {
    gid: i64,
    points: i64,
}

/// Characteristic -> (weight per point, its runes from the smallest).
fn rune_table(data: &Data) -> BTreeMap<i64, (f64, Vec<Rune>)> {
    let mut out: BTreeMap<i64, (f64, Vec<Rune>)> = BTreeMap::new();
    for (g, i) in &data.items {
        let [e] = i.effects.as_slice() else { continue };
        let Some(row) = data.effects.get(&e.a) else { continue };
        if i.type_id != RUNE_TYPE || e.max <= 0 || NO_RUNE_EFFECTS.contains(&e.a) || row.characteristic == 0 || row.density <= 0.0 {
            continue;
        }
        let entry = out.entry(row.characteristic).or_insert((row.density, vec![]));
        if !entry.1.iter().any(|r| r.points == e.max) {
            entry.1.push(Rune { gid: *g, points: e.max });
        }
    }
    for (_, runes) in out.values_mut() {
        runes.sort_by_key(|r| (r.points, r.gid));
    }
    out
}

/// The item's lines from its effects ([action, value]) and its default ones: one per characteristic that has runes.
fn lines_of(data: &Data, runes: &BTreeMap<i64, (f64, Vec<Rune>)>, gid: i64, effects: &[(i64, i64)]) -> Vec<Line> {
    // (characteristic, sign) of an effect that has runes: "-Force" (negative effectPowerRate) counts against "+Force"
    let stat = |a: i64| {
        let row = data.effects.get(&a)?;
        runes.contains_key(&row.characteristic).then_some((row.characteristic, if row.density < 0.0 { -1.0 } else { 1.0 }))
    };
    let mut ranges: BTreeMap<i64, (f64, f64)> = BTreeMap::new();
    let mut order: Vec<i64> = vec![];
    for d in data.items.get(&gid).map(|i| i.effects.as_slice()).unwrap_or_default() {
        let Some((c, sign)) = stat(d.a) else { continue };
        let (lo, hi) = (d.min as f64 * sign, d.max as f64 * sign);
        // a fixed value: from 0 (as the simulator gets it: diceSide 0)
        ranges.insert(c, if lo == hi { (lo.min(0.0), hi.max(0.0)) } else { (lo.min(hi), lo.max(hi)) });
        order.push(c);
    }
    let mut lines: Vec<Line> = vec![];
    for &(a, v) in effects {
        let Some((c, sign)) = stat(a) else { continue };
        match lines.iter_mut().find(|l| l.stat == c) {
            Some(l) => l.value += v as f64 * sign,
            None => lines.push(Line { stat: c, value: v as f64 * sign, range: ranges.get(&c).copied(), weight: runes[&c].0 }),
        }
    }
    for c in order {
        if !lines.iter().any(|l| l.stat == c) {
            lines.push(Line { stat: c, value: 0.0, range: ranges.get(&c).copied(), weight: runes[&c].0 });
        }
    }
    lines
}

#[derive(Clone, Debug, PartialEq)]
struct Item {
    uid: i64,
    gid: i64,
    effects: Vec<(i64, i64)>,
}

pub struct Forgemagie {
    enabled: AtomicBool,
    item: Mutex<Option<Item>>,
    bind: Role<Bindings>,
    effect_bind: Role<ObjectEffect>,
    runes: OnceLock<BTreeMap<i64, (f64, Vec<Rune>)>>,
}

impl Default for Forgemagie {
    fn default() -> Self {
        Self::new()
    }
}

impl Forgemagie {
    pub fn new() -> Self {
        Self { enabled: AtomicBool::new(false), item: Mutex::new(None), bind: Role::default(), effect_bind: Role::default(), runes: OnceLock::new() }
    }

    fn runes(&self, data: &Data) -> &BTreeMap<i64, (f64, Vec<Rune>)> {
        self.runes.get_or_init(|| rune_table(data))
    }

    /// An ObjectItem that is an equipment piece: (uid, gid, effects).
    fn equipment(ctx: &Ctx, b: &Bindings, eb: &ObjectEffect, it: &DynamicMessage) -> Option<Item> {
        let (gid, uid, _) = object_ints(&ctx.data, b, it, &None);
        if !ctx.data.is_equipment(gid) {
            return None;
        }
        let effects = stat_effects(loutil_proto::util::messages(it, &b.effects).iter().map(|e| parse_effect(&ctx.data, eb, e)).collect());
        Some(Item { uid, gid, effects: effects.iter().filter_map(|e| Some((e.action, e.hi?))).collect() })
    }

    /// The equipment piece of a message: in its lists of ObjectItemInventory (added / modified), or one of its ObjectItem
    /// fields (the craft result's object).
    fn found(&self, ctx: &Ctx, name: &str, m: &DynamicMessage) -> Option<Item> {
        let (b, eb) = (self.bind.get(ctx, "inventory")?, self.effect_bind.get(ctx, "object_effect")?);
        let msg = if name == RESULT {
            let obj = ctx.proto.field_named(m.descriptor().full_name(), "object")?;
            m.get_field_by_name(&obj)?.as_message()?.clone()
        } else {
            m.clone()
        };
        for f in msg.descriptor().fields() {
            if !matches!(f.kind(), Kind::Message(_)) || !msg.has_field(&f) {
                continue;
            }
            let value = msg.get_field(&f);
            let entries: Vec<DynamicMessage> = match value.as_list() {
                Some(l) => l.iter().filter_map(|v| v.as_message().cloned()).collect(),
                None => value.as_message().cloned().into_iter().collect(),
            };
            for e in entries {
                // an ObjectItemInventory (its ObjectItem inside) or an ObjectItem
                let entry = b.item.iter().any(|n| e.descriptor().get_field_by_name(n).is_some());
                let it = if entry { entry_item(&ctx.data, b, &e, &None) } else { Some(e) };
                if let Some(item) = it.and_then(|it| Self::equipment(ctx, b, eb, &it)) {
                    return Some(item);
                }
            }
        }
        None
    }

    fn json(&self, data: &Data, item: &Item) -> Value {
        let runes = self.runes(data);
        let level = data.items.get(&item.gid).map_or(200, |i| i.level);
        let lines = lines_of(data, runes, item.gid, &item.effects);
        let out: Vec<Value> = lines
            .iter()
            .map(|l| {
                let (per, list) = &runes[&l.stat];
                let tiers: Vec<Value> = list
                    .iter()
                    .map(|r| {
                        let o = odds(&lines, level, l.stat, r.points as f64, r.points as f64 * per);
                        json!({"gid": r.gid, "name": data.item_name(r.gid), "points": r.points, "odds": o.map(|o| json!({"sc": o.sc, "sn": o.sn, "ec": o.ec}))})
                    })
                    .collect();
                json!({"stat": l.stat, "value": l.value, "min": l.range.map(|r| r.0), "max": l.range.map(|r| r.1), "runes": tiers})
            })
            .collect();
        json!({"uid": item.uid, "gid": item.gid, "name": data.item_name(item.gid), "level": level, "lines": out})
    }

    fn state(&self, data: &Data) -> Value {
        self.item.lock().as_ref().map_or(Value::Null, |i| self.json(data, i))
    }

    fn set(&self, ctx: &Ctx, item: Option<Item>) {
        let mut cur = self.item.lock();
        if *cur == item {
            return;
        }
        *cur = item;
        let v = cur.as_ref().map_or(Value::Null, |i| self.json(&ctx.data, i));
        drop(cur);
        ctx.emit(ID, "item", v);
    }
}

impl Module for Forgemagie {
    fn id(&self) -> &'static str {
        ID
    }

    fn interest(&self) -> Interest {
        Interest::Types(&[ADDED, MODIFIED, RESULT, REMOVED, REMOVED_LIST, LEAVE])
    }

    /// Off (Automation « Forgemagie »): the workshop messages are not even decoded.
    fn wants(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    fn on_message(&self, m: &Message, ctx: &Ctx) {
        let Some(d) = &m.decoded else { return };
        let uid = self.item.lock().as_ref().map(|i| i.uid);
        match m.name.as_str() {
            LEAVE => self.set(ctx, None),
            // the item taken out: its uid (the event's only int, or in its list)
            REMOVED | REMOVED_LIST => {
                let mut uids: Vec<i64> = scalar_ints(d).into_iter().map(|(_, v)| v).collect();
                for f in d.descriptor().fields().filter(|f| f.is_list()) {
                    uids.extend(int_list(d, f.name()));
                }
                if uid.is_some_and(|u| uids.contains(&u)) {
                    self.set(ctx, None);
                }
            }
            _ => {
                if let Some(item) = self.found(ctx, &m.name, d) {
                    self.set(ctx, Some(item));
                }
            }
        }
    }

    /// `state`: the item as in `forgemagie:item`. `set_enabled` {on}: the feature switch (off: the item forgotten).
    fn call(&self, method: &str, args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            "state" => Ok(self.state(&ctx.data)),
            "set_enabled" => {
                let on = args["on"].as_bool().unwrap_or(false);
                self.enabled.store(on, Ordering::Relaxed);
                if !on {
                    self.set(ctx, None);
                }
                Ok(json!({"enabled": on}))
            }
            "window" => Ok(crate::sys::game_window().map_or(Value::Null, |w| json!({"x": w.x, "y": w.y, "width": w.width, "height": w.height}))),
            "refocus" => Ok(json!(crate::sys::focus_game(None))),
            _ => Err(unknown(ID, method)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const L: fn(i64, f64, f64, f64, f64) -> Line = |stat, value, min, max, weight| Line { stat, value, range: Some((min, max)), weight };

    /// Cape au Logis (level 198) as dofocus.fr has it: a fixed MP from 1 to 0.
    fn cape() -> Vec<Line> {
        vec![
            L(11, 320.0, 301.0, 350.0, 0.2),
            L(10, 80.0, 71.0, 100.0, 1.0),
            L(18, 5.0, 4.0, 6.0, 10.0),
            L(23, 1.0, 1.0, 0.0, 90.0),
            L(92, 15.0, 14.0, 18.0, 5.0),
            L(88, 16.0, 14.0, 18.0, 5.0),
            L(34, 8.0, 7.0, 10.0, 6.0),
            L(35, 7.0, 6.0, 8.0, 6.0),
            L(36, 4.0, 4.0, 5.0, 6.0),
            L(87, 12.0, 11.0, 15.0, 2.0),
        ]
    }
    fn with(stat: i64, value: f64) -> Vec<Line> {
        cape().into_iter().map(|l| if l.stat == stat { Line { value, ..l } } else { l }).collect()
    }

    /// The values dofocus.fr's own code gives (its `Pz` run on the same lines, 2026-11-07).
    #[test]
    fn matches_the_simulator() {
        let perfect: Vec<Line> = cape().into_iter().map(|l| Line { value: l.range.unwrap().0.max(l.range.unwrap().1), ..l }).collect();
        let malus = [cape(), vec![L(12, -10.0, -15.0, -10.0, 3.0)]].concat();
        let exo_ap = [cape(), vec![Line { stat: 1, value: 1.0, range: None, weight: 100.0 }]].concat();
        #[rustfmt::skip]
        let cases: Vec<(&str, Vec<Line>, i64, i64, f64, f64, Option<(f64, f64)>)> = vec![
            ("vi", cape(), 198, 11, 5.0, 1.0, Some((2.0, 7.050817767406016))),
            ("pa vi", cape(), 198, 11, 15.0, 3.0, Some((2.0, 49.05081776740602))),
            ("ra vi", cape(), 198, 11, 50.0, 10.0, Some((2.2344236898746885, 41.675817767406016))),
            ("fo", cape(), 198, 10, 1.0, 1.0, Some((2.0, 0.0))),
            ("ra fo near max", with(10, 98.0), 198, 10, 10.0, 10.0, None),
            ("cc over", with(18, 6.0), 198, 18, 1.0, 10.0, Some((2.0, 32.203617354247456))),
            ("pm repass", with(23, 0.0), 198, 23, 1.0, 90.0, Some((21.15996901881236, 50.0))),
            ("exo pa", cape(), 198, 1, 1.0, 100.0, Some((1.5, 0.0))),
            ("exo po", cape(), 198, 19, 1.0, 51.0, Some((1.5, 0.0))),
            ("exo sa", cape(), 198, 12, 3.0, 9.0, Some((2.784019410934153, 33.372076834963934))),
            ("exo ini low", cape(), 60, 44, 10.0, 1.0, Some((4.3577737759881945, 52.23669084723511))),
            ("res crit low level", cape(), 60, 87, 1.0, 2.0, Some((51.93749663839442, 48.06250336160558))),
            ("perfect item vi", perfect, 198, 11, 5.0, 1.0, Some((2.0, 5.0))),
            ("malus line", malus, 198, 12, 1.0, 3.0, Some((3.824962956347143, 42.868722217260355))),
            ("exo pa present", exo_ap, 198, 11, 5.0, 1.0, Some((2.0, 0.0))),
            ("line cap", cape(), 198, 18, 10.0, 100.0, None),
        ];
        for (name, lines, level, stat, points, weight, want) in cases {
            let got = odds(&lines, level, stat, points, weight).map(|o| (o.sc, o.sn));
            match (got, want) {
                (Some((sc, sn)), Some((wsc, wsn))) => assert!((sc - wsc).abs() < 1e-9 && (sn - wsn).abs() < 1e-9, "{name}: {sc} {sn}, want {wsc} {wsn}"),
                (g, w) => assert_eq!(g.is_some(), w.is_some(), "{name}: {g:?}, want {w:?}"),
            }
        }
    }

    /// Synthetic workshop messages: the item put in (added), a rune passed (craft result), the workshop left; a rune put in
    /// (not equipment) changes nothing. Their fields found by type.
    #[test]
    fn follows_the_workshop_item() {
        use loutil_proto::{MessageDescriptor, PbValue};
        let Some((hub, proto)) = crate::kit::testkit::hub("loutil-forgemagie-item") else { return };
        let data = hub.ctx.data.clone();
        let b: Bindings = serde_json::from_value(data.bindings["inventory"].clone()).unwrap();
        let md = |name: &str| proto.message(&proto.obf_for_clear(name).unwrap_or_else(|| panic!("{name} named"))).unwrap();
        let msg_field = |m: &MessageDescriptor, list: bool| m.fields().find(|f| f.is_list() == list && matches!(f.kind(), Kind::Message(_))).unwrap();
        let (added_md, result_md) = (md(ADDED), md(RESULT));
        let inv_f = msg_field(&added_md, true);
        let inv_md = inv_f.kind().as_message().unwrap().clone();
        let item_md = inv_md.get_field_by_name(&b.item[0]).unwrap().kind().as_message().unwrap().clone();
        let object = |gid: i64, uid: i32| {
            let mut it = DynamicMessage::new(item_md.clone());
            for (f, v) in b.ints.iter().zip([1, gid as i32, uid]) {
                it.set_field_by_name(f, PbValue::I32(v));
            }
            it
        };
        let added = |gid: i64, uid: i32| {
            let mut inv = DynamicMessage::new(inv_md.clone());
            inv.set_field_by_name(&b.item[0], PbValue::Message(object(gid, uid)));
            let mut m = DynamicMessage::new(added_md.clone());
            m.set_field(&inv_f, PbValue::List(vec![PbValue::Message(inv)]));
            m
        };
        let fm = Forgemagie::new();
        fm.call("set_enabled", json!({"on": true}), &hub.ctx).unwrap();
        let send = |name: &str, d: DynamicMessage| fm.on_message(&crate::kit::testkit::msg(name, Some(d), 1.0), &hub.ctx);
        let rune = *data.items.iter().find(|(_, i)| i.type_id == RUNE_TYPE).unwrap().0;
        send(ADDED, added(rune, 7000001));
        assert!(fm.call("state", Value::Null, &hub.ctx).unwrap().is_null(), "a rune is no workshop item");
        send(ADDED, added(14168, 7000002));
        let s = fm.call("state", Value::Null, &hub.ctx).unwrap();
        assert_eq!((s["gid"].as_i64(), s["uid"].as_i64(), s["level"].as_i64()), (Some(14168), Some(7000002), Some(198)));
        assert_eq!(s["lines"].as_array().unwrap().len(), 10, "every default line, at 0 without effects");
        let vi = &s["lines"][0]["runes"];
        assert_eq!(vi.as_array().unwrap().len(), 3);
        assert!(vi[0]["odds"]["sc"].as_f64().unwrap() > 0.0);

        send(ADDED, added(rune, 7000003));
        assert_eq!(fm.call("state", Value::Null, &hub.ctx).unwrap()["uid"].as_i64(), Some(7000002), "the rune put in keeps the item");
        // the craft result: object -> its ObjectItem fields (the second one set here)
        let obj_f = result_md.get_field_by_name(&proto.field_named(result_md.full_name(), "object").unwrap()).unwrap();
        let desc_md = obj_f.kind().as_message().unwrap().clone();
        let item_fields: Vec<_> = desc_md.fields().filter(|f| f.kind().as_message() == Some(&item_md)).collect();
        assert_eq!(item_fields.len(), 2, "two ObjectItem fields told apart by content");
        let mut desc = DynamicMessage::new(desc_md);
        desc.set_field(&item_fields[1], PbValue::Message(object(19985, 7000004)));
        let mut result = DynamicMessage::new(result_md);
        result.set_field(&obj_f, PbValue::Message(desc));
        send(RESULT, result);
        assert_eq!(fm.call("state", Value::Null, &hub.ctx).unwrap()["gid"].as_i64(), Some(19985));
        send(LEAVE, DynamicMessage::new(md(LEAVE)));
        assert!(fm.call("state", Value::Null, &hub.ctx).unwrap().is_null());
    }

    /// The pack's runes and an item's lines: signed per characteristic, fixed values from 0, lost default effects at 0.
    #[test]
    fn lines_from_the_pack() {
        let Some((hub, _)) = crate::kit::testkit::hub("loutil-forgemagie-lines") else { return };
        let data = &hub.ctx.data;
        let runes = rune_table(data);
        let vi = &runes[&11];
        assert_eq!(vi.1.iter().map(|r| r.points).collect::<Vec<_>>(), [5, 15, 50], "Rune / Pa / Ra Vi");
        assert!((vi.0 - 0.2).abs() < 1e-6);
        assert_eq!(runes[&1].1.len(), 1, "one AP rune");
        // Cape au Logis: vitality, -10 strength (its "-Force" effect), MP lost
        let lines = lines_of(data, &runes, 14168, &[(125, 320), (157, 10), (115, 5)]);
        assert_eq!(lines[0], Line { stat: 11, value: 320.0, range: Some((301.0, 350.0)), weight: vi.0 });
        assert_eq!((lines[1].stat, lines[1].value), (10, -10.0));
        let mp = lines.iter().find(|l| l.stat == 23).unwrap();
        assert_eq!((mp.value, mp.range), (0.0, Some((0.0, 1.0))));
        assert_eq!(lines.len(), 10, "every default effect once");
    }
}
