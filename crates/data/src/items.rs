//! Item helpers shared by modules: id resolution, comparison with default effects (port of formatters.py).
use crate::Data;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub const LOT_SIZES: [i64; 4] = [1, 10, 100, 1000];
/// Info-only effects: "Modifié par : X", "Fabriqué par : X" -> metadata, not stats.
pub const INFO_EFFECTS: [(i64, &str); 2] = [(985, "modified_by"), (988, "crafted_by")];
/// "Empêche les futures forgemagies" -> shield flag.
pub const LOCK_EFFECT: i64 = 2825;

pub fn info_key(action: i64) -> Option<&'static str> {
    INFO_EFFECTS.iter().find(|(a, _)| *a == action).map(|x| x.1)
}

/// One effect of an item instance, annotated against the item's default effects.
#[derive(Debug, Clone, Serialize)]
pub struct Effect {
    pub action: i64,
    pub text: String,
    pub params: Vec<Value>,
    pub raw: Value,
    pub known: bool,
    pub hi: Option<i64>,
    pub str: Option<String>,
    /// Classification from the data pack (see EffectRow); used by the UI to order effects.
    pub group: String,
    pub order: i64,
    pub density: f64,
    /// "normal" | "added" | "above" | "removed"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<[i64; 2]>,
}

impl Data {
    /// From the unordered ints (uid?, gid, type) pick (gid, type, uid): gid is an item whose typeId equals another int.
    pub fn resolve_gid_type(&self, ints: &[i64]) -> (Option<i64>, Option<i64>, Option<i64>) {
        for (i, &g) in ints.iter().enumerate() {
            let Some(it) = self.items.get(&g) else { continue };
            for (j, &t) in ints.iter().enumerate() {
                if j != i && it.type_id == t {
                    let rest = ints.iter().enumerate().find(|(k, _)| *k != i && *k != j).map(|x| *x.1);
                    return (Some(g), Some(t), rest);
                }
            }
        }
        // a lone int that is an item id, or an item's nameId (the server may send name ids)
        for (i, &g) in ints.iter().enumerate() {
            if g > 0 {
                if let Some(it) = self.items.get(&g) {
                    if !ints.contains(&it.type_id) {
                        let uid = ints.iter().enumerate().find(|(k, x)| *k != i && !self.itemtypes.contains_key(x)).map(|x| *x.1);
                        return (Some(g), Some(it.type_id), uid);
                    }
                }
            }
        }
        for &n in ints {
            if let Some(&g) = self.by_name_id.get(&n) {
                return (Some(g), Some(self.items[&g].type_id), None);
            }
        }
        (None, None, None)
    }

    fn default_ranges(&self, gid: Option<i64>) -> BTreeMap<i64, Vec<(i64, i64)>> {
        let mut m: BTreeMap<i64, Vec<(i64, i64)>> = BTreeMap::new();
        for d in gid.and_then(|g| self.items.get(&g)).map(|i| i.effects.as_slice()).unwrap_or_default() {
            m.entry(d.a).or_default().push((d.min, d.max));
        }
        m
    }

    fn tag(&self, e: &mut Effect) {
        match self.effects.get(&e.action) {
            Some(r) => (e.group, e.order, e.density) = (r.group.clone(), r.order, r.density),
            None => (e.group, e.order, e.density) = ("other".into(), i64::MAX, 0.0),
        }
    }

    /// Annotate effects vs the item's default effects: added (not a default), above (over default max), normal; plus removed.
    pub fn compare_effects(&self, mut effs: Vec<Effect>, gid: Option<i64>) -> Vec<Effect> {
        let known_item = gid.map_or(false, |g| self.items.contains_key(&g));
        let defaults = self.default_ranges(gid);
        let mut present = std::collections::HashSet::new();
        for e in &mut effs {
            self.tag(e);
            present.insert(e.action);
            let d = defaults.get(&e.action);
            let dmax = d.map(|d| d.iter().map(|x| x.1).max().unwrap());
            e.status = Some(if !known_item {
                "normal"
            } else if d.is_none() {
                "added"
            } else if e.hi.map_or(false, |h| h > dmax.unwrap()) {
                "above"
            } else {
                "normal"
            });
            if let Some(d) = d {
                e.range = Some([d.iter().map(|x| x.0).min().unwrap(), dmax.unwrap()]);
            }
        }
        for (a, ds) in &defaults {
            let Some(er) = self.effects.get(a) else { continue };
            if present.contains(a) || info_key(*a).is_some() || *a == LOCK_EFFECT || !(er.use_dice != 0 || er.characteristic != 0) {
                continue;
            }
            let (lo, hi) = (ds.iter().map(|x| x.0).min().unwrap(), ds.iter().map(|x| x.1).max().unwrap());
            let params: Vec<Value> = if hi != lo { vec![lo.into(), hi.into()] } else { vec![lo.into()] };
            let (text, _) = self.render_effect(*a, &params);
            let mut removed = Effect {
                action: *a,
                text,
                params: vec![lo.into(), hi.into()],
                raw: Value::Null,
                known: true,
                hi: Some(hi),
                str: None,
                status: Some("removed"),
                range: Some([lo, hi]),
                group: String::new(),
                order: 0,
                density: 0.0,
            };
            self.tag(&mut removed);
            effs.push(removed);
        }
        effs
    }

    /// Consolidated default effects of an item: [{a, min, max, label}] (info/lock effects excluded).
    pub fn defaults_for(&self, gid: Option<i64>) -> Vec<Value> {
        let mut out = vec![];
        let ranges = self.default_ranges(gid);
        let mut seen = std::collections::HashSet::new();
        // order of first appearance in the item
        for d in gid.and_then(|g| self.items.get(&g)).map(|i| i.effects.as_slice()).unwrap_or_default() {
            if !seen.insert(d.a) {
                continue;
            }
            let ds = &ranges[&d.a];
            let (min, max) = (ds.iter().map(|x| x.0).min().unwrap(), ds.iter().map(|x| x.1).max().unwrap());
            if info_key(d.a).is_some() || d.a == LOCK_EFFECT {
                continue;
            }
            if let Some(label) = self.effect_label(d.a) {
                out.push(json!({"a": d.a, "min": min, "max": max, "label": label}));
            }
        }
        out
    }
}
