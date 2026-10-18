//! Effect template rendering and labels (port of gamedata.py).
use crate::{Data, EffectRow};
use regex::Regex;
use serde_json::{json, Value};
use std::sync::OnceLock;

/// "Arme de chasse": the only effect of the "other" group kept in the filter.
pub const HUNTING_WEAPON_EFFECT: i64 = 795;

fn re(cell: &'static OnceLock<Regex>, p: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(p).unwrap())
}
fn cond_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"\{+~([0-9])~([0-9])([^}]*)\}+")
}
fn plural_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"\{+~[a-z]+[^}]*\}+")
}
fn hole_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"#([1-9])")
}
fn tag_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"</?[a-z]+[^>]*>")
}
fn anytag_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"<[^>]*>")
}
fn ws_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"\s+")
}

fn spaced(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

impl Data {
    /// Render an effect description. `params` = [p1, p2, ...] (null = absent). Returns (text, effect row if known).
    pub fn render_effect(&self, action: i64, params: &[Value]) -> (String, Option<&EffectRow>) {
        let row = self.effects.get(&action);
        let Some(tpl) = row.and_then(|e| self.text(e.description_id)) else {
            return (format!("effect {action}: {}", Value::Array(params.to_vec())), row);
        };
        // weapon hits can arrive as (max, min): always show the range ascending
        let mut ordered = params.to_vec();
        if row.is_some_and(|e| e.group == "weapon") {
            if let (Some(a), Some(b)) = (ordered.first().and_then(Value::as_i64), ordered.get(1).and_then(Value::as_i64)) {
                if a > b {
                    ordered.swap(0, 1);
                }
            }
        }
        let params = &ordered[..];
        let val = |i: usize| params.get(i - 1).filter(|v| !v.is_null());
        let out = cond_re().replace_all(tpl, |c: &regex::Captures| {
            let b: usize = c[2].parse().unwrap();
            let keep = !matches!(val(b), None) && val(b).and_then(Value::as_i64) != Some(0);
            if keep { c[3].to_string() } else { String::new() }
        });
        let out = hole_re().replace_all(&out, |c: &regex::Captures| match val(c[1].parse().unwrap()) {
            None => String::new(),
            Some(Value::Number(n)) if n.is_i64() || n.is_u64() => spaced(n.as_i64().unwrap_or(0)),
            Some(Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
        });
        let out = plural_re().replace_all(&out, "");
        let out = ws_re().replace_all(&tag_re().replace_all(&out, ""), " ").trim().to_string();
        (out, row)
    }

    /// Template with the numeric placeholders removed: "#1{{~1~2 à }}#2% Résistance Terre" -> "% Résistance Terre".
    pub fn effect_label(&self, a: i64) -> Option<String> {
        let tpl = self.text(self.effects.get(&a)?.description_id)?;
        let t = cond_re().replace_all(tpl, "");
        let t = hole_re().replace_all(&t, "");
        let t = plural_re().replace_all(&t, "");
        let t = anytag_re().replace_all(&t, "");
        let t = ws_re().replace_all(&t, " ");
        let t = t.trim_matches(|c| c == ' ' || c == ':');
        (!t.is_empty()).then(|| t.to_string())
    }

    /// Whether the effect belongs in the item filter: no negative twins, and from the "other" group only the
    /// hunting weapon mark (everything else there is flags, spell tweaks, titles...).
    pub fn is_filterable(&self, a: i64) -> bool {
        let (Some(e), Some(label)) = (self.effects.get(&a), self.effect_label(a)) else { return false };
        !(e.density < 0.0 || label.starts_with('-')) && (e.group != "other" || a == HUNTING_WEAPON_EFFECT)
    }

    /// {id, label, group, order, density, filterable} for one effect (None when it has no label).
    pub fn effect_entry(&self, a: i64) -> Option<Value> {
        let (e, label) = (self.effects.get(&a)?, self.effect_label(a)?);
        Some(json!({"id": a, "label": label, "group": e.group, "order": e.order, "density": e.density, "stat": self.stat_of(a), "filterable": self.is_filterable(a)}))
    }

    /// Every labelled effect, so the UI can group/label anything it meets in a lot.
    pub fn effect_meta(&self) -> Value {
        Value::Array(self.effects.keys().filter_map(|a| self.effect_entry(*a)).collect())
    }

    /// Effects that appear on equipment of levels 150-200 (see `Data::equipment_effects`), for the UI effect picker.
    pub fn filterable_effects(&self) -> Value {
        const GROUPS: [&str; 7] = ["weapon", "stats", "main", "secondary", "damage", "resist", "other"];
        let rank = |g: &str| GROUPS.iter().position(|x| *x == g).unwrap_or(GROUPS.len());
        let mut out: Vec<(usize, i64, String, Value)> = self
            .equipment_effects
            .iter()
            .filter(|a| crate::items::info_key(**a).is_none() && **a != crate::items::LOCK_EFFECT && self.is_filterable(**a))
            .filter_map(|a| {
                let (e, v) = (self.effects.get(a)?, self.effect_entry(*a)?);
                Some((rank(&e.group), e.order, v["label"].as_str()?.to_lowercase(), v))
            })
            .collect();
        out.sort_by(|a, b| (a.0, a.1, &a.2).cmp(&(b.0, b.1, &b.2)));
        Value::Array(out.into_iter().map(|x| x.3).collect())
    }
}
