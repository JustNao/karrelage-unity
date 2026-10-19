//! Item helpers shared by modules: id resolution, comparison with default effects (port of formatters.py).
use crate::Data;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Characteristics left out of the item density: prospection (magicFind) says nothing about what an item is worth.
pub const DENSITY_EXCLUDED_STATS: [i64; 1] = [48];

pub const LOT_SIZES: [i64; 4] = [1, 10, 100, 1000];
/// Info-only effects: "Modifié par : X", "Fabriqué par : X" -> metadata, not stats.
pub const INFO_EFFECTS: [(i64, &str); 2] = [(985, "modified_by"), (988, "crafted_by")];
/// "Empêche les futures forgemagies" -> shield flag.
pub const LOCK_EFFECT: i64 = 2825;
/// Lock (2825) and the other transcendance-rune markers: effects of a rune item that are not the effect it adds.
pub const RUNE_MARKER_EFFECTS: [i64; 3] = [LOCK_EFFECT, 2826, 2827];
/// "Change l'élément de frappe": only a marker, the lot already carries the new element's hit (added) and the old one (removed).
pub const ELEMENT_CHANGE_EFFECT: i64 = 700;

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
    /// Which stat the effect changes (see `Data::stat_of`): "+Force" and "-Force" share it.
    pub stat: i64,
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

    /// The characteristic an effect modifies, so an effect and its negative twin compare as the same stat. Effects without a
    /// characteristic (weapon hits, flags) stand alone: `-action`.
    pub fn stat_of(&self, action: i64) -> i64 {
        match self.effects.get(&action) {
            Some(r) if r.characteristic != 0 => r.characteristic,
            _ => -action,
        }
    }

    fn tag(&self, e: &mut Effect) {
        match self.effects.get(&e.action) {
            Some(r) => (e.group, e.order, e.density) = (r.group.clone(), r.order, r.density),
            None => (e.group, e.order, e.density) = ("other".into(), i64::MAX, 0.0),
        }
        e.stat = self.stat_of(e.action);
    }

    /// Annotate effects vs the item's default effects: added (not a default), above (over default max), normal; plus removed.
    pub fn compare_effects(&self, mut effs: Vec<Effect>, gid: Option<i64>) -> Vec<Effect> {
        let known_item = gid.is_some_and(|g| self.items.contains_key(&g));
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
            } else if e.hi.is_some_and(|h| h > dmax.unwrap()) {
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
                stat: 0,
            };
            self.tag(&mut removed);
            effs.push(removed);
        }
        effs
    }

    /// Value of an item instance: sum of (effect value x effect density). Densities are the game's effectPowerRate
    /// (AP 100, MP 90, vitality 0.2, ...), and negative effects carry half of their positive twin's density, so
    /// "5 vitalité" = 1.0 and "-10 vitalité" = -1.0. Ranges count as their mean; effects without a numeric value
    /// (flags, titles) count for nothing.
    pub fn item_density(&self, effs: &[Effect]) -> f64 {
        effs.iter().filter_map(|e| self.effect_density(e.action, &e.params)).sum()
    }

    /// Density of one effect instance (None when it has no numeric value or is unknown). See `item_density`.
    pub fn effect_density(&self, action: i64, params: &[Value]) -> Option<f64> {
        if action == ELEMENT_CHANGE_EFFECT || DENSITY_EXCLUDED_STATS.contains(&self.stat_of(action)) {
            return None;
        }
        let nums: Vec<f64> = params.iter().filter_map(Value::as_f64).collect();
        if nums.is_empty() {
            return None;
        }
        Some(nums.iter().sum::<f64>() / nums.len() as f64 * self.effects.get(&action)?.density)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn eff(action: i64, params: Vec<Value>) -> Effect {
        Effect {
            action,
            text: String::new(),
            params,
            raw: Value::Null,
            known: true,
            hi: None,
            str: None,
            group: String::new(),
            order: 0,
            density: 0.0,
            stat: 0,
            status: None,
            range: None,
        }
    }

    /// Needs the generated data pack; skipped otherwise.
    #[test]
    fn density_follows_effect_power_rate() {
        let pack = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../datapack");
        if !pack.join("manifest.json").exists() {
            return;
        }
        let d = Data::load(&pack, "fr").unwrap();
        let close = |a: f64, b: f64| assert!((a - b).abs() < 1e-3, "{a} vs {b}");
        close(d.item_density(&[eff(125, vec![5.into()])]), 1.0); // 5 vitalité
        close(d.item_density(&[eff(111, vec![1.into()]), eff(128, vec![1.into()])]), 190.0); // 1 PA + 1 PM
        close(d.item_density(&[eff(153, vec![10.into()])]), -1.0); // -10 vitalité: half of the 2.0 it would add
        close(d.item_density(&[eff(125, vec![5.into()]), eff(153, vec![10.into()])]), 0.0);
        close(d.item_density(&[eff(100, vec![8.into(), 10.into()])]), 9.0); // 8 à 10 dommages Neutre: mean 9, rate 1
        close(d.item_density(&[eff(981, vec![])]), 0.0); // flag without value
        close(d.item_density(&[eff(176, vec![30.into()]), eff(125, vec![5.into()])]), 1.0);
        // prospection is ignored
    }
}
