//! Building blocks every module uses the same way (see "Shared building blocks" in CLAUDE.md):
//! - [`Role`]: build-specific field names from the data pack's bindings.json, deserialised once.
//! - [`Store`]: a module's state behind a mutex, persisted in the background under `<app data>/<module>/<file>`.
//! - [`arg_i64`] / [`arg_str`] / [`unknown`]: arguments and errors of `Module::call`.
use crate::{Ctx, Persist};
use anyhow::{anyhow, Result};
use parking_lot::{Mutex, MutexGuard};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::sync::{Arc, OnceLock};

/// Obfuscated field names of one bindings.json role, read once (None when the pack lacks the role or it does not parse:
/// an older pack still loads, the feature just stays idle).
///
/// ```ignore
/// buy: Role<AhBuy>,                          // field
/// let Some(b) = self.buy.get(ctx, "ah_buy") else { return };
/// ```
pub struct Role<T>(OnceLock<Option<T>>);

impl<T> Default for Role<T> {
    fn default() -> Self {
        Self(OnceLock::new())
    }
}

impl<T: DeserializeOwned> Role<T> {
    pub const fn new() -> Self {
        Self(OnceLock::new())
    }

    /// The role `name` of bindings.json.
    pub fn get(&self, ctx: &Ctx, name: &str) -> Option<&T> {
        self.0.get_or_init(|| serde_json::from_value(ctx.data.bindings.get(name)?.clone()).ok()).as_ref()
    }

    /// The whole bindings.json document as `T` (structs naming several roles as fields).
    pub fn root(&self, ctx: &Ctx) -> Option<&T> {
        self.0.get_or_init(|| serde_json::from_value(ctx.data.bindings.clone()).ok()).as_ref()
    }
}

/// A module's state, persisted with [`Persist`] (debounced background writes, unreadable file set aside).
///
/// ```ignore
/// store: Store<BTreeMap<i64, Value>>,                                   // field, `Store::default()`
/// let saved = self.store.open(ctx, "prices", "prices.json", |s| json!(s)); // in `init`
/// self.store.update(|s| s.insert(gid, entry));                          // change + write soon
/// ```
pub struct Store<T> {
    data: Arc<Mutex<T>>,
    persist: OnceLock<Persist>,
}

impl<T: Default> Default for Store<T> {
    fn default() -> Self {
        Self { data: Default::default(), persist: OnceLock::new() }
    }
}

impl<T: Send + 'static> Store<T> {
    /// Start persisting to `<app data>/<module>/<file>` with `to_json`; returns what was saved there (the caller fills the state
    /// from it). Before `open` (unit tests) nothing is written.
    pub fn open(&self, ctx: &Ctx, module: &str, file: &str, to_json: impl Fn(&T) -> Value + Send + Sync + 'static) -> Option<Value> {
        let data = self.data.clone();
        let (persist, saved) = Persist::open(ctx.module_dir(module).join(file), ctx.logger(), move || to_json(&data.lock()));
        let _ = self.persist.set(persist);
        saved
    }

    /// Read (or change, then call [`Store::touch`]). Never held while writing to disk.
    pub fn lock(&self) -> MutexGuard<'_, T> {
        self.data.lock()
    }

    /// Change the state and have it written soon.
    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let r = f(&mut self.data.lock());
        self.touch();
        r
    }

    /// The state changed: write it soon.
    pub fn touch(&self) {
        if let Some(p) = self.persist.get() {
            p.touch();
        }
    }

    /// Write now (changes made by the user, which must survive a crash right after).
    pub fn flush(&self) {
        if let Some(p) = self.persist.get() {
            p.touch();
            p.flush();
        }
    }
}

/// Integer argument `key` of a `call` (error when missing).
pub fn arg_i64(args: &Value, key: &str) -> Result<i64> {
    args[key].as_i64().ok_or_else(|| anyhow!("argument {key} manquant"))
}

/// String argument `key` of a `call` (error when missing).
pub fn arg_str<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args[key].as_str().ok_or_else(|| anyhow!("argument {key} manquant"))
}

/// Error for a `call` method the module does not have.
pub fn unknown(module: &str, method: &str) -> anyhow::Error {
    anyhow!("{module}: unknown method {method}")
}

#[cfg(test)]
pub(crate) mod testkit {
    //! Test setup shared by the modules: a hub over the generated data pack and synthetic messages.
    use crate::{Hub, Message};
    use loutil_core::RawMessage;
    use loutil_data::Data;
    use loutil_proto::{DynamicMessage, Proto};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    /// The generated data pack (`python pipeline/export_pack.py`, or dist/datapack.zip unpacked); None = the test is skipped.
    pub fn pack() -> Option<PathBuf> {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../datapack");
        p.join("manifest.json").exists().then_some(p)
    }

    /// A fresh user dir in the temp folder.
    pub fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// A hub over the data pack with a fresh user dir `<temp>/<name>` (events and logs dropped); None without a pack.
    pub fn hub(name: &str) -> Option<(Hub, Arc<Proto>)> {
        let pack = pack()?;
        let dir = temp_dir(name);
        let data = Arc::new(Data::load(&pack, "fr").unwrap());
        let proto = Arc::new(Proto::load(&pack, dir.join("o.json")).unwrap());
        Some((Hub::new(data, proto.clone(), dir, Arc::new(|_, _| {}), Arc::new(|_| {})), proto))
    }

    /// A message as the hub would hand it to a module (already decoded, no envelope).
    pub fn msg(name: &str, decoded: Option<DynamicMessage>, t: f64) -> Message {
        Message {
            id: 1,
            raw: RawMessage { t, srv: "x".into(), dir: "S->C", payload: vec![] },
            env: None,
            type_name: None,
            name: name.into(),
            conf: None,
            decoded,
            fmt: None,
        }
    }
}
