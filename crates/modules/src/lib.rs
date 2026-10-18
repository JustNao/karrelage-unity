//! Feature modules + the hub. A module declares which messages it wants (by clear name), reacts in `on_message`,
//! may render messages into a shared "formatted" view (`format`), and exposes request/response `call`s to the UI.
//!
//! Adding a feature = one file here implementing `Module` + registering it in `all()` + a UI folder under app/ui/src/modules.
pub mod auction;
pub mod names;
pub mod packets;
pub mod prices;
pub mod update;

use anyhow::{anyhow, Result};
use loutil_core::wire::{self, Envelope};
use loutil_core::RawMessage;
use loutil_data::Data;
use loutil_proto::{DynamicMessage, Proto};
use serde_json::Value;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Which messages a module wants, matched against the clear name (or the obfuscated type if unnamed).
pub enum Interest {
    All,
    Types(&'static [&'static str]),
}

impl Interest {
    fn matches(&self, m: &Message) -> bool {
        match self {
            Interest::All => true,
            Interest::Types(t) => t.iter().any(|x| *x == m.name || Some(*x) == m.type_name.as_deref()),
        }
    }
}

/// One message seen on the wire, decoded as far as the schema allows.
pub struct Message {
    pub id: u64,
    pub raw: RawMessage,
    pub env: Option<Envelope>,
    /// Obfuscated type name from the envelope (`type.ankama.com/<this>`).
    pub type_name: Option<String>,
    /// Clear name when known, else the obfuscated type name.
    pub name: String,
    pub conf: Option<String>,
    pub decoded: Option<DynamicMessage>,
    /// Filled by the first module whose `format` understands the message.
    pub fmt: Option<Value>,
}

/// Shared services handed to modules.
pub struct Ctx {
    pub data: Arc<Data>,
    pub proto: Arc<Proto>,
    /// Per-user writable folder (stores, overrides).
    pub dir: PathBuf,
    emit: Arc<dyn Fn(&str, Value) + Send + Sync>,
    log: Arc<dyn Fn(String) + Send + Sync>,
}

impl Ctx {
    /// Send an event to the UI as "<module>:<event>".
    pub fn emit(&self, module: &str, event: &str, payload: Value) {
        (self.emit)(&format!("{module}:{event}"), payload);
    }

    /// Owned handle to the event emitter, for background threads ("<module>:<event>" names).
    pub fn emitter(&self) -> Arc<dyn Fn(&str, Value) + Send + Sync> {
        self.emit.clone()
    }

    pub fn log(&self, line: impl Into<String>) {
        (self.log)(line.into());
    }

    /// Folder owned by one module inside the user dir.
    pub fn module_dir(&self, module: &str) -> PathBuf {
        self.dir.join(module)
    }
}

pub trait Module: Send + Sync {
    fn id(&self) -> &'static str;
    fn interest(&self) -> Interest;
    /// False = skip this module entirely (no decoding done on its behalf, nothing stored). Default: always on.
    fn wants(&self) -> bool {
        true
    }
    fn init(&self, _ctx: &Ctx) {}
    /// Render a message into a UI-friendly structure (shown in the packet inspector's formatted column).
    fn format(&self, _m: &Message, _ctx: &Ctx) -> Option<Value> {
        None
    }
    fn on_message(&self, _m: &Message, _ctx: &Ctx) {}
    /// Request/response API used by the UI through the generic `module_call` command.
    fn call(&self, method: &str, args: Value, ctx: &Ctx) -> Result<Value>;
}

/// Every module of the app. Register new ones here.
pub fn all() -> Vec<Box<dyn Module>> {
    vec![Box::new(packets::Packets::new()), Box::new(auction::Auction::new()), Box::new(prices::Prices::new()), Box::new(names::NamesTab::new()), Box::new(update::Update::new())]
}

pub struct Hub {
    pub ctx: Ctx,
    modules: Vec<Box<dyn Module>>,
    seq: AtomicU64,
}

impl Hub {
    pub fn new(
        data: Arc<Data>,
        proto: Arc<Proto>,
        dir: PathBuf,
        emit: Arc<dyn Fn(&str, Value) + Send + Sync>,
        log: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Self {
        let hub = Hub { ctx: Ctx { data, proto, dir, emit, log }, modules: all(), seq: AtomicU64::new(0) };
        for m in &hub.modules {
            m.init(&hub.ctx);
        }
        hub
    }

    pub fn handle(&self, raw: RawMessage) {
        let env = wire::parse_envelope(&raw.payload);
        let type_name = env.as_ref().and_then(|e| e.type_url.as_deref()).map(|u| u.rsplit('/').next().unwrap_or(u).to_string());
        let (name, conf) = match &type_name {
            Some(t) => self.ctx.proto.message_name(t),
            None => (String::new(), None),
        };
        let mut msg = Message { id: self.seq.fetch_add(1, Ordering::Relaxed) + 1, raw, env, type_name, name, conf, decoded: None, fmt: None };
        let interested: Vec<&Box<dyn Module>> = self.modules.iter().filter(|m| m.wants() && m.interest().matches(&msg)).collect();
        if interested.is_empty() {
            return;
        }
        if let (Some(t), Some(e)) = (&msg.type_name, &msg.env) {
            msg.decoded = self.ctx.proto.decode(t, &e.value);
        }
        for m in &interested {
            // never let a module break capture
            match catch_unwind(AssertUnwindSafe(|| m.format(&msg, &self.ctx))) {
                Ok(Some(f)) => {
                    msg.fmt = Some(f);
                    break;
                }
                Ok(None) => {}
                Err(_) => self.ctx.log(format!("module {} panicked in format", m.id())),
            }
        }
        for m in interested {
            if catch_unwind(AssertUnwindSafe(|| m.on_message(&msg, &self.ctx))).is_err() {
                self.ctx.log(format!("module {} panicked in on_message", m.id()));
            }
        }
    }

    pub fn call(&self, module: &str, method: &str, args: Value) -> Result<Value> {
        let m = self.modules.iter().find(|m| m.id() == module).ok_or_else(|| anyhow!("unknown module {module}"))?;
        m.call(method, args, &self.ctx)
    }

    pub fn module_ids(&self) -> Vec<&'static str> {
        self.modules.iter().map(|m| m.id()).collect()
    }
}

pub fn load_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Atomic write (tmp + rename) so a crash never leaves a half-written store.
pub fn save_json(path: &Path, v: &Value) -> Result<()> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec(v)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
