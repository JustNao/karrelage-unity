//! Packet inspector: ring buffer of every decoded message, with name editing (developer tool).
use crate::{Ctx, Interest, Message, Module};
use anyhow::{anyhow, Result};
use loutil_core::wire;
use loutil_proto::DynamicMessage;
use serde_json::{json, Map, Value};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

const RING: usize = 5000;

/// A buffered packet: the record without its decoded tree, plus the message, so names can be re-applied
/// (a rename must show up in already captured packets).
struct Entry {
    rec: Map<String, Value>,
    msg: Option<DynamicMessage>,
}

/// Raw traffic is only kept while the inspector tab is open (`active`): the app runs all the time, so nothing
/// accumulates (or lingers in memory) otherwise.
pub struct Packets {
    ring: Mutex<VecDeque<Entry>>,
    active: AtomicBool,
}

impl Packets {
    pub fn new() -> Self {
        Self { ring: Mutex::new(VecDeque::new()), active: AtomicBool::new(false) }
    }

    fn record(&self, m: &Message) -> Map<String, Value> {
        let mut r = Map::new();
        r.insert("id".into(), m.id.into());
        r.insert("t".into(), ((m.raw.t * 1000.0).round() / 1000.0).into());
        r.insert("srv".into(), m.raw.srv.clone().into());
        r.insert("dir".into(), m.raw.dir.into());
        r.insert("len".into(), m.raw.payload.len().into());
        match &m.env {
            Some(e) => {
                r.insert("kind".into(), e.kind.into());
                r.insert("uid".into(), e.uid.map_or(Value::Null, Value::from));
                r.insert("type".into(), m.type_name.clone().map_or(Value::Null, Value::from));
                r.insert("name".into(), m.name.clone().into());
                r.insert("conf".into(), m.conf.clone().map_or(Value::Null, Value::from));
                if m.decoded.is_none() {
                    r.insert("body".into(), if e.value.is_empty() { json!({}) } else { wire::to_obj(&e.value, 0) });
                }
                if let Some(f) = &m.fmt {
                    r.insert("fmt".into(), f.clone());
                }
            }
            None => {
                r.insert("unparsed".into(), wire::hex(&m.raw.payload[..m.raw.payload.len().min(64)]).into());
            }
        }
        r.insert("hex".into(), wire::hex(&m.raw.payload[..m.raw.payload.len().min(4096)]).into());
        r
    }

    /// Record as sent to the UI, with names as they are *now*.
    fn render(entry: &Entry, ctx: &Ctx) -> Value {
        let mut r = entry.rec.clone();
        if let Some(t) = r.get("type").and_then(Value::as_str) {
            let (name, conf) = ctx.proto.message_name(t);
            r.insert("name".into(), name.into());
            r.insert("conf".into(), conf.map_or(Value::Null, Value::from));
        }
        r.insert("decoded".into(), entry.msg.as_ref().map_or(Value::Null, |d| ctx.proto.to_json(d)));
        Value::Object(r)
    }
}

impl Module for Packets {
    fn id(&self) -> &'static str {
        "packets"
    }

    fn interest(&self) -> Interest {
        Interest::All
    }

    fn wants(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    fn on_message(&self, m: &Message, ctx: &Ctx) {
        let entry = Entry { rec: self.record(m), msg: m.decoded.clone() };
        let out = Self::render(&entry, ctx);
        {
            let mut ring = self.ring.lock().unwrap();
            if ring.len() == RING {
                ring.pop_front();
            }
            ring.push_back(entry);
        }
        ctx.emit("packets", "packet", out);
    }

    fn call(&self, method: &str, args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            "list" => {
                let since = args.get("since").and_then(Value::as_u64).unwrap_or(0);
                let ring = self.ring.lock().unwrap();
                Ok(Value::Array(ring.iter().filter(|e| e.rec["id"].as_u64().unwrap_or(0) > since).map(|e| Self::render(e, ctx)).collect()))
            }
            // the inspector tab opened (on: true) or closed: closing drops everything buffered
            "set_active" => {
                let on = args.get("on").and_then(Value::as_bool).unwrap_or(false);
                self.active.store(on, Ordering::Relaxed);
                if !on {
                    *self.ring.lock().unwrap() = VecDeque::new();
                }
                Ok(json!({"active": on}))
            }
            "clear" => {
                self.ring.lock().unwrap().clear();
                Ok(json!({"buffered": 0}))
            }
            "buffered" => Ok(json!(self.ring.lock().unwrap().len())),
            // confirm/edit a decoded name; persisted in the user overrides file
            "rename" => {
                let msg = args.get("msg").and_then(Value::as_str).ok_or_else(|| anyhow!("msg required"))?;
                let field = args.get("field").and_then(Value::as_str).filter(|s| !s.is_empty());
                let name = args.get("name").and_then(Value::as_str).unwrap_or("").trim();
                ctx.proto.set_override(msg, field, name)?;
                Ok(json!({"ok": true}))
            }
            "overrides" => Ok(ctx.proto.overrides()),
            _ => Err(anyhow!("packets: unknown method {method}")),
        }
    }
}
