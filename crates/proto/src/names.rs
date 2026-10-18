//! Clear names for obfuscated messages/fields: names.json (auto matches) + overrides (hand-confirmed).
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

pub struct Names {
    base: Value,
    overrides: Value,
    /// merged view: obf message -> {name?, conf?, fields: {obf field -> {name?, cands?}}}
    merged: HashMap<String, Value>,
}

impl Names {
    pub fn new(base: Value, overrides: Value) -> Self {
        let mut n = Names { base, overrides, merged: HashMap::new() };
        n.rebuild();
        n
    }

    fn rebuild(&mut self) {
        let mut m: HashMap<String, Value> = self.base.as_object().cloned().unwrap_or_default().into_iter().collect();
        for (k, ov) in self.overrides.as_object().into_iter().flatten() {
            let e = m.entry(k.clone()).or_insert_with(|| json!({"fields": {}}));
            let e = e.as_object_mut().unwrap();
            if let Some(name) = ov.get("name").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                e.insert("name".into(), name.into());
                e.insert("conf".into(), "manual".into());
            }
            let fields = e.entry("fields").or_insert_with(|| json!({})).as_object_mut().unwrap();
            for (fk, fv) in ov.get("fields").and_then(Value::as_object).into_iter().flatten() {
                fields.insert(fk.clone(), json!({"name": fv, "exact": true, "manual": true}));
            }
        }
        self.merged = m;
    }

    pub fn len(&self) -> usize {
        self.merged.len()
    }

    pub fn is_empty(&self) -> bool {
        self.merged.is_empty()
    }

    pub fn overrides(&self) -> &Value {
        &self.overrides
    }

    pub fn message_name(&self, type_name: &str) -> (String, Option<String>) {
        match self.merged.get(type_name).and_then(|m| m.get("name")).and_then(Value::as_str) {
            Some(n) => (n.to_string(), self.merged[type_name].get("conf").and_then(Value::as_str).map(str::to_owned)),
            None => (type_name.to_string(), None),
        }
    }

    pub fn obf_for_clear(&self, clear: &str) -> Option<String> {
        self.merged.iter().find(|(_, v)| v.get("name").and_then(Value::as_str) == Some(clear)).map(|(k, _)| k.clone())
    }

    /// Label of a field: its clear name, or "obf (a|b?)" with the candidates not already taken by a sibling.
    pub fn field_label(&self, msg: &str, field: &str) -> String {
        let fields = self.merged.get(msg).and_then(|m| m.get("fields")).and_then(Value::as_object);
        let Some(f) = fields.and_then(|f| f.get(field)) else { return field.to_string() };
        if let Some(n) = f.get("name").and_then(Value::as_str) {
            return n.to_string();
        }
        let taken: HashSet<&str> = fields.unwrap().values().filter_map(|x| x.get("name").and_then(Value::as_str)).collect();
        let c: Vec<&str> = f.get("cands").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).filter(|x| !taken.contains(x)).collect();
        if c.is_empty() {
            field.to_string()
        } else {
            format!("{field} ({}?)", c.join("|"))
        }
    }

    /// Persist-ready edit of the overrides document.
    pub fn set_override(&mut self, msg: &str, field: Option<&str>, name: &str) {
        let ov = self.overrides.as_object_mut().unwrap();
        let e = ov.entry(msg).or_insert_with(|| json!({})).as_object_mut().unwrap();
        match field {
            Some(f) => {
                let fields = e.entry("fields").or_insert_with(|| json!({})).as_object_mut().unwrap();
                if name.is_empty() {
                    fields.remove(f);
                } else {
                    fields.insert(f.into(), name.into());
                }
            }
            None if !name.is_empty() => {
                e.insert("name".into(), name.into());
            }
            None => {
                e.remove("name");
            }
        }
        if e.get("fields").and_then(Value::as_object).map_or(false, Map::is_empty) {
            e.remove("fields");
        }
        if e.is_empty() {
            ov.remove(msg);
        }
        self.rebuild();
    }
}
