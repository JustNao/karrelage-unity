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
    /// weapon | stats | main | secondary | damage | resist | other (classified by pipeline/export_pack.py).
    #[serde(default = "other_group")]
    pub group: String,
    /// Rank inside the group (the in-game characteristics panel order).
    #[serde(default)]
    pub order: i64,
    /// effectPowerRate: rune weight per unit (AP 100, MP 90, vitality 0.2, ...). Kept for later features.
    #[serde(default)]
    pub density: f64,
}

fn other_group() -> String {
    "other".into()
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ItemType {
    pub name_id: i64,
    /// 0 = equipment (weapons, rings, hats, pets, dofus, ...).
    pub category_id: Option<i64>,
}

/// Item levels scanned to decide which effects are "equipment effects" (used by the effect picker).
pub const EQUIPMENT_LEVELS: std::ops::RangeInclusive<i64> = 150..=200;
const EQUIPMENT_CATEGORY: i64 = 0;

pub struct Data {
    pub dir: PathBuf,
    pub manifest: Manifest,
    pub items: HashMap<i64, Item>,
    pub effects: HashMap<i64, EffectRow>,
    pub itemtypes: HashMap<i64, ItemType>,
    pub strings: HashMap<i64, String>,
    /// Build-specific obfuscated names per role (bindings.json), see pipeline/bindings.json.
    pub bindings: Value,
    /// Effect ids appearing in the default effects of equipment of levels EQUIPMENT_LEVELS.
    pub equipment_effects: std::collections::HashSet<i64>,
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
        let itemtypes: HashMap<i64, ItemType> = read_map(dir, "itemtypes.json")?;
        let equipment_effects = items
            .values()
            .filter(|i| EQUIPMENT_LEVELS.contains(&i.level) && itemtypes.get(&i.type_id).and_then(|t| t.category_id) == Some(EQUIPMENT_CATEGORY))
            .flat_map(|i| i.effects.iter().map(|e| e.a))
            .collect();
        Ok(Self {
            manifest: read_manifest(dir)?,
            effects: read_map(dir, "effects.json")?,
            itemtypes,
            equipment_effects,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs the generated data pack (`python pipeline/export_pack.py`); skipped otherwise.
    #[test]
    fn effect_picker_only_lists_equipment_effects() {
        let pack = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../datapack");
        if !pack.join("manifest.json").exists() {
            eprintln!("skipped: no datapack/");
            return;
        }
        let d = Data::load(&pack, "fr").unwrap();
        let listed = d.filterable_effects();
        let listed = listed.as_array().unwrap();
        assert!(!listed.is_empty() && listed.len() < d.effects.len() / 2, "{} of {}", listed.len(), d.effects.len());
        assert!(listed.iter().all(|e| d.equipment_effects.contains(&e["id"].as_i64().unwrap())));
        // Force (118) is on every level-200 equipment piece
        assert!(listed.iter().any(|e| e["id"] == 118));
        // classification + density (effectPowerRate) from the pack
        assert_eq!((d.effects[&111].group.as_str(), d.effects[&111].density), ("main", 100.0)); // PA
        assert_eq!((d.effects[&128].group.as_str(), d.effects[&128].density), ("main", 90.0)); // PM
        assert_eq!(d.effects[&118].group, "stats");
        assert_eq!(d.effects[&210].group, "resist");
        assert_eq!(d.effects[&430].group, "damage");
        assert_eq!(d.effects[&174].group, "secondary");
        // weapon hits live in their own group; negatives and most "other" effects are not filterable
        assert_eq!(d.effects[&100].group, "weapon");
        assert_eq!(d.effects[&95].group, "weapon"); // vole Neutre
        assert!(d.is_filterable(100) && d.is_filterable(effects::HUNTING_WEAPON_EFFECT));
        assert!(!d.is_filterable(101) && !d.is_filterable(157) && !d.is_filterable(168), "negatives"); // -PA hit, -Force, -PA
        assert!(!d.is_filterable(281) && !d.is_filterable(981), "other group");
        assert!(listed.iter().all(|e| e["filterable"] == true && !e["label"].as_str().unwrap().starts_with('-')));
        // the picker is ordered stats -> main -> secondary -> damage -> resist
        assert_eq!(listed[0]["group"], "weapon");
    }
}
