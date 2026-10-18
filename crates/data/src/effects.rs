//! Effect template rendering and labels (port of gamedata.py).
use crate::{Data, EffectRow};
use regex::Regex;
use serde_json::{json, Value};
use std::sync::OnceLock;

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

    pub fn effect_group(&self, a: i64) -> &'static str {
        let (Some(e), Some(lab)) = (self.effects.get(&a), self.effect_label(a)) else { return "other" };
        if lab.starts_with('-') {
            return "other";
        }
        let stat = e.use_dice != 0 && e.characteristic != 0;
        if stat && lab.contains("Dommage") && !lab.contains("Résistance") && !lab.contains("Renvoy") {
            "damage"
        } else if stat && lab.contains('%') && lab.contains("Résistance") {
            "resist"
        } else {
            "other"
        }
    }

    /// Stat-like effects (those that can appear on equipment) for the UI filter.
    pub fn filterable_effects(&self) -> Value {
        let mut out: Vec<(String, Value)> = self
            .effects
            .iter()
            .filter(|(_, e)| e.use_dice != 0 && e.characteristic != 0)
            .filter_map(|(a, _)| {
                let l = self.effect_label(*a)?;
                Some((l.to_lowercase(), json!({"id": a, "label": l, "group": self.effect_group(*a)})))
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Value::Array(out.into_iter().map(|x| x.1).collect())
    }
}
