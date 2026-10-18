//! Packet inspector: ring buffer of every decoded message, with name editing (developer tool).
use crate::{Ctx, Interest, Message, Module};
use anyhow::{anyhow, Result};
use loutil_core::wire;
use serde_json::{json, Map, Value};
use std::collections::VecDeque;
use std::sync::Mutex;

const RING: usize = 5000;

pub struct Packets {
    ring: Mutex<VecDeque<Value>>,
}

impl Packets {
    pub fn new() -> Self {
        Self { ring: Mutex::new(VecDeque::with_capacity(RING)) }
    }

    fn record(&self, m: &Message, ctx: &Ctx) -> Value {
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
                let decoded = m.decoded.as_ref().map(|d| ctx.proto.to_json(d));
                if decoded.is_none() {
                    r.insert("body".into(), if e.value.is_empty() { json!({}) } else { wire::to_obj(&e.value, 0) });
                }
                r.insert("decoded".into(), decoded.unwrap_or(Value::Null));
                if let Some(f) = &m.fmt {
                    r.insert("fmt".into(), f.clone());
                }
            }
            None => {
                r.insert("unparsed".into(), wire::hex(&m.raw.payload[..m.raw.payload.len().min(64)]).into());
            }
        }
        r.insert("hex".into(), wire::hex(&m.raw.payload[..m.raw.payload.len().min(4096)]).into());
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

    fn on_message(&self, m: &Message, ctx: &Ctx) {
        let rec = self.record(m, ctx);
        {
            let mut ring = self.ring.lock().unwrap();
            if ring.len() == RING {
                ring.pop_front();
            }
            ring.push_back(rec.clone());
        }
        ctx.emit("packets", "packet", rec);
    }

    fn call(&self, method: &str, args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            "list" => {
                let since = args.get("since").and_then(Value::as_u64).unwrap_or(0);
                let ring = self.ring.lock().unwrap();
                Ok(Value::Array(ring.iter().filter(|r| r["id"].as_u64().unwrap_or(0) > since).cloned().collect()))
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
