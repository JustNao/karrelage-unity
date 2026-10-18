//! Data pack (produced by pipeline/export_pack.py): items, effects, item types and localized strings,
//! plus the effect-template rendering and item helpers shared by feature modules.
pub mod effects;
pub mod items;

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Clone)]
pub struct Manifest {
    pub pack: u32,
    pub generated: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DefaultEffect {
    pub a: i64,
    pub min: i64,
    pub max: i64,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub name_id: i64,
    pub type_id: i64,
    pub level: i64,
    pub icon_id: i64,
    pub effects: Vec<DefaultEffect>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EffectRow {
    pub description_id: i64,
    pub characteristic: i64,
    pub use_dice: i64,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ItemType {
    pub name_id: i64,
}

pub struct Data {
    pub dir: PathBuf,
    pub manifest: Manifest,
    pub items: HashMap<i64, Item>,
    pub effects: HashMap<i64, EffectRow>,
    pub itemtypes: HashMap<i64, ItemType>,
    pub strings: HashMap<i64, String>,
    /// Build-specific obfuscated names per role (bindings.json), see pipeline/bindings.json.
    pub bindings: Value,
    pub(crate) by_name_id: HashMap<i64, i64>,
}

fn read_map<T: for<'de> Deserialize<'de>>(dir: &Path, file: &str) -> Result<HashMap<i64, T>> {
    let raw: HashMap<String, T> = serde_json::from_slice(&std::fs::read(dir.join(file)).with_context(|| format!("reading {file}"))?)?;
    Ok(raw.into_iter().filter_map(|(k, v)| Some((k.parse().ok()?, v))).collect())
}

pub fn read_manifest(dir: &Path) -> Result<Manifest> {
    Ok(serde_json::from_slice(&std::fs::read(dir.join("manifest.json"))?)?)
}

impl Data {
    pub fn load(dir: &Path, lang: &str) -> Result<Self> {
        let items: HashMap<i64, Item> = read_map(dir, "items.json")?;
        let strings = read_map::<String>(dir, &format!("strings.{lang}.json")).or_else(|_| read_map::<String>(dir, "strings.fr.json"))?;
        let by_name_id = items.iter().map(|(k, v)| (v.name_id, *k)).collect();
        Ok(Self {
            manifest: read_manifest(dir)?,
            effects: read_map(dir, "effects.json")?,
            itemtypes: read_map(dir, "itemtypes.json")?,
            bindings: serde_json::from_slice(&std::fs::read(dir.join("bindings.json"))?)?,
            dir: dir.to_path_buf(),
            items,
            strings,
            by_name_id,
        })
    }

    pub fn text(&self, id: i64) -> Option<&str> {
        self.strings.get(&id).map(String::as_str)
    }

    pub fn item_name(&self, gid: i64) -> String {
        self.items.get(&gid).and_then(|i| self.text(i.name_id)).map(str::to_owned).unwrap_or_else(|| format!("item#{gid}"))
    }

    pub fn type_name(&self, tid: Option<i64>) -> String {
        match tid {
            Some(t) => self.itemtypes.get(&t).and_then(|t| self.text(t.name_id)).map(str::to_owned).unwrap_or_else(|| format!("type#{t}")),
            None => "type#None".into(),
        }
    }
}
