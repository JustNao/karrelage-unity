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
    /// Sizes of the game files the protos were dumped from (absent in older packs).
    #[serde(default)]
    pub game: Option<GameFingerprint>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct GameFingerprint {
    pub assembly: Option<u64>,
    pub metadata: Option<u64>,
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
    /// Item set (panoplie) the item belongs to, if any (absent in older packs).
    #[serde(default)]
    pub set_id: Option<i64>,
    pub effects: Vec<DefaultEffect>,
}

/// Item set: `effects[i]` = the bonus granted when `i + 1` of its pieces are worn.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ItemSet {
    #[serde(default)]
    pub name_id: Option<i64>,
    pub items: Vec<i64>,
    pub effects: Vec<Vec<DefaultEffect>>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SpellRow {
    pub name_id: i64,
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
/// Item types treated as companions (Familier, Montilier, Dragodinde, Muldo, Volkorne): tracked by level instead of by stats.
pub const COMPANION_TYPES: [i64; 5] = [18, 121, 331, 332, 333];
/// Companions that are levelled up by feeding them resources (the mounts are not).
pub const FEEDABLE_TYPES: [i64; 2] = [18, 121];

pub struct Data {
    pub dir: PathBuf,
    pub manifest: Manifest,
    pub items: HashMap<i64, Item>,
    pub effects: HashMap<i64, EffectRow>,
    pub itemtypes: HashMap<i64, ItemType>,
    /// Legendary passives: spell id -> name string id (absent in older packs).
    pub spells: HashMap<i64, SpellRow>,
    /// Crafting recipes: result gid -> recipes, each a list of (ingredient gid, quantity) (absent in older packs).
    pub recipes: HashMap<i64, Vec<Vec<(i64, i64)>>>,
    /// Item sets (panoplies): set id -> pieces and bonuses per piece count (absent in older packs).
    pub itemsets: HashMap<i64, ItemSet>,
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
            spells: read_map(dir, "spells.json").unwrap_or_default(),
            recipes: read_map(dir, "recipes.json").unwrap_or_default(),
            itemsets: read_map(dir, "itemsets.json").unwrap_or_default(),
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

    /// Every craftable item's first recipe, with the names the UI needs: {recipes: {gid: [[ingredient, qty]]}, names: {gid: name}}.
    pub fn recipes_json(&self) -> Value {
        let mut recipes = serde_json::Map::new();
        let mut names = serde_json::Map::new();
        for (g, rs) in &self.recipes {
            let Some(r) = rs.first() else { continue };
            recipes.insert(g.to_string(), serde_json::json!(r));
            for id in std::iter::once(g).chain(r.iter().map(|(i, _)| i)) {
                names.entry(id.to_string()).or_insert_with(|| Value::String(self.item_name(*id)));
            }
        }
        serde_json::json!({"recipes": recipes, "names": names})
    }

    /// Equipment category (weapons, rings, hats, pets...): the only items the HDV tab keeps.
    pub fn is_equipment(&self, gid: i64) -> bool {
        self.items.get(&gid).and_then(|i| self.itemtypes.get(&i.type_id)).and_then(|t| t.category_id) == Some(EQUIPMENT_CATEGORY)
    }

    pub fn is_feedable(&self, gid: Option<i64>) -> bool {
        gid.and_then(|g| self.items.get(&g)).is_some_and(|i| FEEDABLE_TYPES.contains(&i.type_id))
    }

    pub fn is_companion(&self, gid: Option<i64>) -> bool {
        gid.and_then(|g| self.items.get(&g)).is_some_and(|i| COMPANION_TYPES.contains(&i.type_id))
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

        // legendary passive: the spell id param is rendered as the passive's name
        if d.spells.contains_key(&11352) {
            assert_eq!(d.render_effect(effects::LEGENDARY_EFFECT, &[11352.into(), 11352.into()]).0, "Appétit de Crocobur");
        }
        // Dofus passive: the spell id is rendered as the name of the special effect
        if d.spells.contains_key(&8396) {
            assert_eq!(d.render_effect(effects::LEGENDARY_EFFECT, &[8396.into(), 8396.into()]).0, "Rouge Vermeil");
        }
        assert_eq!(d.effects[&118].group, "stats");
        assert_eq!(d.effects[&210].group, "resist");
        assert_eq!(d.stat_of(118), d.stat_of(157), "+Force and -Force are one stat");
        assert_ne!(d.stat_of(118), d.stat_of(126));
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
