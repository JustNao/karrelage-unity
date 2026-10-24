//! Schema-aware decoding with the descriptor set exported by pipeline/build_protos.py, and the clear-name layer
//! (names.json = beta matches + hand-confirmed overrides, plus per-user overrides). Port of schema.py.
pub mod names;
pub mod util;

use anyhow::{Context, Result};
use parking_lot::{Mutex, RwLock};
use prost_reflect::DescriptorPool;
pub use prost_reflect::{DynamicMessage, FieldDescriptor, Kind, MapKey, MessageDescriptor, ReflectMessage, Value as PbValue};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub use names::Names;

pub struct Proto {
    pool: DescriptorPool,
    names: RwLock<Names>,
    override_path: PathBuf,
    /// serialises override writes, so the file always ends with the latest edit (taken before `names`)
    saving: Mutex<()>,
}

impl Proto {
    /// `dir` = data pack folder; `override_path` = user overrides file (same format as work/overrides.json).
    pub fn load(dir: &Path, override_path: PathBuf) -> Result<Self> {
        let pool = DescriptorPool::decode(std::fs::read(dir.join("protocol.desc")).context("protocol.desc")?.as_slice())?;
        let base: Value = serde_json::from_slice(&std::fs::read(dir.join("names.json")).context("names.json")?)?;
        let overrides: Value = std::fs::read(&override_path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_else(|| json!({}));
        Ok(Self { pool, names: RwLock::new(Names::new(base, overrides)), override_path, saving: Mutex::new(()) })
    }

    pub fn message(&self, type_name: &str) -> Option<MessageDescriptor> {
        self.pool.get_message_by_name(type_name)
    }

    /// Top-level messages known to the descriptor set.
    pub fn message_count(&self) -> usize {
        self.pool.all_messages().filter(|m| m.parent_message().is_none()).count()
    }

    /// (obfuscated type, clear name, confidence) of every named message.
    pub fn named(&self) -> Vec<(String, String, Option<String>)> {
        self.names.read().entries()
    }

    pub fn decode(&self, type_name: &str, bytes: &[u8]) -> Option<DynamicMessage> {
        DynamicMessage::decode(self.message(type_name)?, bytes).ok()
    }

    /// (clear name or the obfuscated type name, confidence)
    pub fn message_name(&self, type_name: &str) -> (String, Option<String>) {
        self.names.read().message_name(type_name)
    }

    pub fn mapped_count(&self) -> usize {
        self.names.read().len()
    }

    /// Obfuscated name of a field of message `msg` (descriptor full name) from its exact clear name.
    pub fn field_named(&self, msg: &str, clear: &str) -> Option<String> {
        self.names.read().field_named(msg, clear)
    }

    /// Obfuscated type name for a clear message name (first match), if known in this build.
    pub fn obf_for_clear(&self, clear: &str) -> Option<String> {
        self.names.read().obf_for_clear(clear)
    }

    /// Persist a confirmed name (`field = None` -> the message itself). Empty `name` removes it.
    pub fn set_override(&self, msg: &str, field: Option<&str>, name: &str) -> Result<()> {
        let _saving = self.saving.lock();
        // the names lock is held only for the edit: every captured packet needs it for its name
        let bytes = {
            let mut n = self.names.write();
            n.set_override(msg, field, name);
            serde_json::to_vec_pretty(n.overrides())?
        };
        if let Some(p) = self.override_path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let tmp = self.override_path.with_extension("tmp");
        let mut f = std::fs::File::create(&tmp)?;
        std::io::Write::write_all(&mut f, &bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, &self.override_path)?;
        Ok(())
    }

    pub fn overrides(&self) -> Value {
        self.names.read().overrides().clone()
    }

    /// Decoded message as JSON with clear field names plus `__t` (message name) and `__f` (label -> obfuscated field).
    pub fn to_json(&self, m: &DynamicMessage) -> Value {
        let names = self.names.read();
        rename(&names, m)
    }
}

fn rename(names: &Names, m: &DynamicMessage) -> Value {
    let desc = m.descriptor();
    let mname = desc.full_name().to_string();
    let (mut out, mut meta) = (Map::new(), Map::new());
    for (fd, v) in m.fields() {
        let label = names.field_label(&mname, fd.name());
        out.insert(label.clone(), value_json(names, v, &fd.kind()));
        meta.insert(label, Value::String(fd.name().to_string()));
    }
    out.insert("__t".into(), Value::String(mname));
    out.insert("__f".into(), Value::Object(meta));
    Value::Object(out)
}

fn key_str(k: &MapKey) -> String {
    match k {
        MapKey::Bool(b) => b.to_string(),
        MapKey::I32(x) => x.to_string(),
        MapKey::I64(x) => x.to_string(),
        MapKey::U32(x) => x.to_string(),
        MapKey::U64(x) => x.to_string(),
        MapKey::String(s) => s.clone(),
    }
}

fn value_json(names: &Names, v: &PbValue, kind: &Kind) -> Value {
    match v {
        PbValue::Bool(b) => json!(b),
        PbValue::I32(x) => json!(x),
        PbValue::U32(x) => json!(x),
        // 64-bit ints as strings, like protobuf's JSON mapping (JS cannot hold them exactly)
        PbValue::I64(x) => json!(x.to_string()),
        PbValue::U64(x) => json!(x.to_string()),
        PbValue::F32(x) => json!(x),
        PbValue::F64(x) => json!(x),
        PbValue::String(s) => json!(s),
        PbValue::Bytes(b) => json!(format!("0x{}", b.iter().map(|x| format!("{x:02x}")).collect::<String>())),
        PbValue::EnumNumber(n) => match kind {
            Kind::Enum(e) => e.get_value(*n).map_or(json!(n), |v| json!(v.name())),
            _ => json!(n),
        },
        PbValue::Message(m) => rename(names, m),
        PbValue::List(l) => Value::Array(l.iter().map(|x| value_json(names, x, kind)).collect()),
        PbValue::Map(m) => {
            let vk = match kind {
                Kind::Message(md) => md.get_field_by_name("value").map(|f| f.kind()),
                _ => None,
            };
            Value::Object(m.iter().map(|(k, x)| (key_str(k), value_json(names, x, vk.as_ref().unwrap_or(kind)))).collect())
        }
    }
}
