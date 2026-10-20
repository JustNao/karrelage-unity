//! Packet inspector: the last packets of every message type, with name editing (developer tool).
use crate::{Ctx, Interest, Message, Module};
use anyhow::{anyhow, Result};
use loutil_core::wire;
use loutil_proto::DynamicMessage;
use parking_lot::Mutex;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Packets kept per message type (recorded all the time, so memory stays bounded by the number of types).
const PER_TYPE: usize = 20;
/// Bytes of raw payload kept per packet for the hex view.
const HEX_BYTES: usize = 4096;
/// New packets are sent to the UI in batches (one event per tick instead of one per packet).
const BATCH_EVERY: Duration = Duration::from_millis(100);

/// A buffered packet: the record without its decoded tree, plus the message, so names can be re-applied
/// (a rename must show up in already captured packets).
struct Entry {
    rec: Map<String, Value>,
    msg: Option<DynamicMessage>,
    /// start of the raw payload, rendered as hex only when the packet is opened (`hex` call)
    raw: Vec<u8>,
}

/// Every packet is recorded, even with the inspector tab closed, but only the last `PER_TYPE` of each message type are
/// kept; new packets are streamed to the UI only while the tab is open (`active`).
pub struct Packets {
    ring: Mutex<HashMap<String, VecDeque<Entry>>>,
    active: AtomicBool,
    /// rendered packets not sent to the UI yet
    outbox: Arc<Mutex<Vec<Value>>>,
}

impl Default for Packets {
    fn default() -> Self {
        Self::new()
    }
}

impl Packets {
    pub fn new() -> Self {
        Self { ring: Mutex::new(HashMap::new()), active: AtomicBool::new(false), outbox: Default::default() }
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
        r
    }

    /// Buffer key: the message type, or the envelope kind for packets without one.
    fn key(rec: &Map<String, Value>) -> String {
        rec.get("type").and_then(Value::as_str).map_or_else(|| format!("?{}", rec.get("kind").and_then(Value::as_str).unwrap_or("")), str::to_owned)
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

    fn init(&self, ctx: &Ctx) {
        let (outbox, emit) = (self.outbox.clone(), ctx.emitter());
        std::thread::spawn(move || loop {
            std::thread::sleep(BATCH_EVERY);
            let batch = std::mem::take(&mut *outbox.lock());
            if !batch.is_empty() {
                emit("packets:batch", Value::Array(batch));
            }
        });
    }

    fn on_message(&self, m: &Message, ctx: &Ctx) {
        let raw = m.raw.payload[..m.raw.payload.len().min(HEX_BYTES)].to_vec();
        let entry = Entry { rec: self.record(m), msg: m.decoded.clone(), raw };
        let out = self.active.load(Ordering::Relaxed).then(|| Self::render(&entry, ctx));
        {
            let mut ring = self.ring.lock();
            let q = ring.entry(Self::key(&entry.rec)).or_default();
            if q.len() == PER_TYPE {
                q.pop_front();
            }
            q.push_back(entry);
        }
        if let Some(out) = out {
            self.outbox.lock().push(out);
        }
    }

    fn call(&self, method: &str, args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            "list" => {
                let since = args.get("since").and_then(Value::as_u64).unwrap_or(0);
                let ring = self.ring.lock();
                let mut all: Vec<&Entry> = ring.values().flatten().filter(|e| e.rec["id"].as_u64().unwrap_or(0) > since).collect();
                all.sort_by_key(|e| e.rec["id"].as_u64().unwrap_or(0));
                Ok(Value::Array(all.into_iter().map(|e| Self::render(e, ctx)).collect()))
            }
            // the inspector tab opened (on: true) or closed: new packets are streamed only while it is open
            "set_active" => {
                let on = args.get("on").and_then(Value::as_bool).unwrap_or(false);
                self.active.store(on, Ordering::Relaxed);
                if !on {
                    self.outbox.lock().clear();
                }
                Ok(json!({"active": on}))
            }
            "clear" => {
                self.ring.lock().clear();
                Ok(json!({"buffered": 0}))
            }
            "buffered" => Ok(json!(self.ring.lock().values().map(VecDeque::len).sum::<usize>())),
            // args: {id} -> hex of the start of that packet's payload
            "hex" => {
                let id = args.get("id").and_then(Value::as_u64).unwrap_or(0);
                let ring = self.ring.lock();
                Ok(ring.values().flatten().find(|e| e.rec["id"].as_u64() == Some(id)).map_or(Value::Null, |e| json!(wire::hex(&e.raw))))
            }
            // args: {id} -> writes that packet (as rendered, plus its hex) to <Downloads>/karrelage-<name>-<id>.json,
            // selects it in the Explorer and returns the path
            "export" => {
                let id = args.get("id").and_then(Value::as_u64).ok_or_else(|| anyhow!("id required"))?;
                let mut v = {
                    let ring = self.ring.lock();
                    let e = ring.values().flatten().find(|e| e.rec["id"].as_u64() == Some(id)).ok_or_else(|| anyhow!("paquet {id} plus en mémoire"))?;
                    let mut v = Self::render(e, ctx);
                    v["hex"] = json!(wire::hex(&e.raw));
                    v
                };
                v["hexTruncated"] = json!(v["len"].as_u64().unwrap_or(0) as usize > HEX_BYTES);
                let name: String =
                    v["name"].as_str().or(v["type"].as_str()).unwrap_or("paquet").chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
                let dir = std::env::var_os("USERPROFILE")
                    .map(|h| std::path::PathBuf::from(h).join("Downloads"))
                    .filter(|d| d.is_dir())
                    .unwrap_or_else(std::env::temp_dir);
                let path = dir.join(format!("karrelage-{name}-{id}.json"));
                std::fs::write(&path, serde_json::to_string_pretty(&v)?)?;
                let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
                Ok(json!(path.display().to_string()))
            }
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
