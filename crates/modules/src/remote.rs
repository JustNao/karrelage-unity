//! Sync with a self-hosted Karrelage server (`crates/server`, Paramètres « Serveur de synchronisation »): every `interval`
//! seconds one signed request pushes what each topic has new and pulls what the others sent since the last cursor. Topics
//! (`prices` today) register at startup; a feature adds one to share its data the same way. Runs on its own thread, started
//! the first time the sync is switched on (the capture thread never waits on the network).
//!
//! Wire format and signature: see `crates/server/src/lib.rs` and `auth.rs` (kept identical here: the end to end test checks it).
use crate::kit::{arg_str, unknown, Store};
use crate::sys::{http, unix_secs_f64};
use crate::{Ctx, Interest, Module};
use anyhow::{anyhow, bail, Result};
use parking_lot::{Condvar, Mutex};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

/// Bodies larger than this are sent gzipped.
const GZIP_OVER: usize = 1024;
/// Shortest interval between two syncs (seconds).
const MIN_INTERVAL: u64 = 60;

/// Data shared through the server, per game server.
pub trait Topic: Send + Sync {
    fn name(&self) -> &'static str;
    /// The game server whose data is shown now (the one synced).
    fn server(&self) -> i64;
    /// Rows to push (taken out: given back with `restore` when the sync fails).
    fn take_pending(&self, server: i64) -> Vec<Value>;
    fn restore(&self, server: i64, rows: Vec<Value>);
    /// The others' rows, newer than the cursor.
    fn apply(&self, server: i64, rows: Vec<Value>);
}

#[derive(Clone, Default, PartialEq)]
struct Config {
    enabled: bool,
    url: String,
    password: String,
    interval: u64,
}

#[derive(Default)]
struct Inner {
    cfg: Mutex<Config>,
    topics: Mutex<Vec<Arc<dyn Topic>>>,
    /// "<url>|<server>|<topic>" -> last seq received
    cursors: Store<BTreeMap<String, i64>>,
    /// true = sync now (configuration changed, « sync_now »)
    kick: Mutex<bool>,
    wake: Condvar,
    status: Mutex<Value>,
    started: OnceLock<()>,
    emit: OnceLock<Arc<dyn Fn(&str, Value) + Send + Sync>>,
    log: OnceLock<Arc<dyn Fn(String) + Send + Sync>>,
}

#[derive(Default, Clone)]
pub struct Remote(Arc<Inner>);

impl Remote {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_topic(&self, t: Arc<dyn Topic>) {
        self.0.topics.lock().push(t);
    }

    /// Sync as soon as possible (the thread wakes up).
    pub fn kick(&self) {
        *self.0.kick.lock() = true;
        self.0.wake.notify_all();
    }

    fn start(&self) {
        self.0.started.get_or_init(|| {
            let me = self.clone();
            std::thread::Builder::new().name("remote-sync".into()).spawn(move || me.run()).ok();
        });
    }

    fn run(&self) {
        let mut last: Option<Instant> = None;
        loop {
            let cfg = self.0.cfg.lock().clone();
            let interval = Duration::from_secs(cfg.interval.max(MIN_INTERVAL));
            let due = last.is_none_or(|l| l.elapsed() >= interval);
            let kicked = std::mem::take(&mut *self.0.kick.lock());
            if cfg.enabled && !cfg.url.is_empty() && (due || kicked) {
                self.sync_once(&cfg);
                last = Some(Instant::now());
                continue;
            }
            let mut k = self.0.kick.lock();
            if !*k {
                let wait = if cfg.enabled { last.map_or(Duration::ZERO, |l| interval.saturating_sub(l.elapsed())) } else { Duration::from_secs(3600) };
                self.0.wake.wait_for(&mut k, wait.max(Duration::from_millis(50)));
            }
        }
    }

    fn cursor_key(url: &str, server: i64, topic: &str) -> String {
        format!("{}|{server}|{topic}", url.trim_end_matches('/'))
    }

    /// One push + pull for the current game server; the result goes to `remote:status`.
    fn sync_once(&self, cfg: &Config) {
        let topics: Vec<Arc<dyn Topic>> = self.0.topics.lock().clone();
        let Some(server) = topics.first().map(|t| t.server()).filter(|s| *s > 0) else { return };
        let mut push = serde_json::Map::new();
        let mut cursors = serde_json::Map::new();
        let mut taken = vec![];
        for t in &topics {
            let rows = t.take_pending(server);
            cursors.insert(t.name().into(), json!(self.0.cursors.lock().get(&Self::cursor_key(&cfg.url, server, t.name())).copied().unwrap_or(0)));
            if !rows.is_empty() {
                push.insert(t.name().into(), Value::Array(rows.clone()));
            }
            taken.push((t.clone(), rows));
        }
        let sent: usize = taken.iter().map(|(_, r)| r.len()).sum();
        let body = json!({"server": server, "cursors": cursors, "push": push});
        let res = request(cfg, "POST", "/v1/sync", body.to_string().as_bytes());
        let status = match res {
            Ok((v, up, down)) => {
                let mut received = 0;
                for (t, _) in &taken {
                    let rows: Vec<Value> = v["changes"][t.name()].as_array().cloned().unwrap_or_default();
                    received += rows.len();
                    if !rows.is_empty() {
                        t.apply(server, rows);
                    }
                    if let Some(c) = v["cursors"][t.name()].as_i64() {
                        self.0.cursors.lock().insert(Self::cursor_key(&cfg.url, server, t.name()), c);
                    }
                }
                self.0.cursors.touch();
                json!({"ok": true, "last": unix_secs_f64(), "server": server, "sent": sent, "received": received, "bytes_up": up, "bytes_down": down})
            }
            Err(e) => {
                for (t, rows) in taken {
                    if !rows.is_empty() {
                        t.restore(server, rows);
                    }
                }
                if let Some(log) = self.0.log.get() {
                    log(format!("remote: sync failed: {e}"));
                }
                json!({"ok": false, "last": unix_secs_f64(), "server": server, "error": e.to_string()})
            }
        };
        *self.0.status.lock() = status.clone();
        if let Some(emit) = self.0.emit.get() {
            emit("remote:status", status);
        }
    }
}

/// hex(HMAC-SHA256(key, msg)), written out (the crate's sha2 has no matching `hmac` release).
fn hmac_sha256(key: &[u8], msg: &[u8]) -> String {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let pad = |b: u8| k.iter().map(|x| x ^ b).collect::<Vec<u8>>();
    let inner = Sha256::new().chain_update(pad(0x36)).chain_update(msg).finalize();
    hex(&Sha256::new().chain_update(pad(0x5c)).chain_update(inner).finalize())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The request signature (crates/server/src/auth.rs `sign`): the password never travels.
fn sign(password: &str, time: i64, method: &str, endpoint: &str, body: &[u8]) -> String {
    hmac_sha256(password.as_bytes(), format!("{time}\n{method}{endpoint}\n{}", hex(&Sha256::digest(body))).as_bytes())
}

/// An http(s) address typed by the user: only those two schemes, no spaces.
fn check_url(url: &str) -> Result<String> {
    let u = url.trim().trim_end_matches('/');
    let rest = u.strip_prefix("https://").or_else(|| u.strip_prefix("http://")).ok_or_else(|| anyhow!("adresse en http:// ou https:// attendue"))?;
    if rest.is_empty() || rest.contains(char::is_whitespace) {
        bail!("adresse invalide");
    }
    Ok(u.to_string())
}

/// A signed request: (answer, bytes sent, bytes received). Errors are worded for the settings' status line.
fn request(cfg: &Config, method: &str, endpoint: &str, body: &[u8]) -> Result<(Value, usize, usize)> {
    let url = format!("{}{endpoint}", check_url(&cfg.url)?);
    let time = unix_secs_f64() as i64;
    let mut req = http()
        .build()
        .request(method, &url)
        .set("X-Karrelage-Time", &time.to_string())
        .set("X-Karrelage-Sig", &sign(&cfg.password, time, method, endpoint, body));
    let payload = if body.len() > GZIP_OVER {
        req = req.set("Content-Encoding", "gzip");
        let mut e = flate2::write::GzEncoder::new(vec![], flate2::Compression::default());
        e.write_all(body)?;
        e.finish()?
    } else {
        body.to_vec()
    };
    let res = if method == "GET" { req.call() } else { req.set("Content-Type", "application/json").send_bytes(&payload) };
    let res = match res {
        Ok(r) => r,
        Err(ureq::Error::Status(401, r)) => {
            let why = r.into_string().unwrap_or_default();
            bail!(if why.contains("clock") { "horloge de ce PC décalée de plus de 5 min" } else { "mot de passe refusé" })
        }
        Err(ureq::Error::Status(code, r)) => bail!("le serveur répond {code} {}", r.into_string().unwrap_or_default()),
        Err(e) => bail!("serveur injoignable ({e})"),
    };
    let down = res.header("Content-Length").and_then(|l| l.parse().ok());
    let mut text = String::new();
    res.into_reader().take(64 << 20).read_to_string(&mut text)?;
    let v: Value = serde_json::from_str(&text).map_err(|_| anyhow!("réponse illisible : est-ce bien un serveur Karrelage ?"))?;
    Ok((v, payload.len(), down.unwrap_or(text.len())))
}

impl Module for Remote {
    fn id(&self) -> &'static str {
        "remote"
    }

    fn interest(&self) -> Interest {
        Interest::Types(&[])
    }

    fn wants(&self) -> bool {
        false
    }

    fn init(&self, ctx: &Ctx) {
        let _ = self.0.emit.set(ctx.emitter());
        let _ = self.0.log.set(ctx.logger());
        if let Some(Value::Object(saved)) = self.0.cursors.open(ctx, "remote", "state.json", |c| json!(c)) {
            *self.0.cursors.lock() = saved.into_iter().filter_map(|(k, v)| Some((k, v.as_i64()?))).collect();
        }
    }

    fn call(&self, method: &str, args: Value, _ctx: &Ctx) -> Result<Value> {
        match method {
            // args: {enabled, url, password, interval (s)}: Paramètres « Serveur de synchronisation »; syncs at once when it changed
            "configure" => {
                let cfg = Config {
                    enabled: args["enabled"].as_bool().unwrap_or(false),
                    url: args["url"].as_str().unwrap_or_default().trim().trim_end_matches('/').to_string(),
                    password: args["password"].as_str().unwrap_or_default().to_string(),
                    interval: args["interval"].as_u64().unwrap_or(MIN_INTERVAL).max(MIN_INTERVAL),
                };
                let changed = *self.0.cfg.lock() != cfg;
                let on = cfg.enabled;
                *self.0.cfg.lock() = cfg;
                if on {
                    self.start();
                }
                if changed {
                    self.kick();
                }
                Ok(json!({"ok": true}))
            }
            // args: {url, password} -> {ok, version, ms}: checks the address and the password (signed ping)
            "test" => {
                let cfg = Config {
                    enabled: true,
                    url: arg_str(&args, "url")?.to_string(),
                    password: arg_str(&args, "password")?.to_string(),
                    interval: MIN_INTERVAL,
                };
                let t = Instant::now();
                let (v, _, _) = request(&cfg, "GET", "/v1/ping", b"")?;
                if v["ok"] != true {
                    bail!("réponse inattendue du serveur");
                }
                Ok(json!({"ok": true, "version": v["version"], "ms": t.elapsed().as_millis() as u64}))
            }
            "sync_now" => {
                self.kick();
                Ok(json!({"ok": true}))
            }
            // -> the last sync's {ok, last, sent, received, bytes_up, bytes_down, error?} (null before the first)
            "status" => Ok(self.0.status.lock().clone()),
            _ => Err(unknown("remote", method)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_matches_the_server() {
        // RFC 4231 test case 2
        assert_eq!(hmac_sha256(b"Jefe", b"what do ya want for nothing?"), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
        assert_eq!(sign("pw", 1000, "POST", "/v1/sync", b"{}"), karrelage_server::auth::sign(b"pw", 1000, "POST", "/v1/sync", b"{}"));
        assert!(check_url("ftp://x").is_err());
        assert_eq!(check_url(" http://1.2.3.4:8787/ ").unwrap(), "http://1.2.3.4:8787");
    }

    /// The real server on a free port; its address.
    fn serve(password: &str) -> String {
        let state = karrelage_server::AppState::new(password, karrelage_server::db::Db::memory().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                tx.send(l.local_addr().unwrap()).unwrap();
                axum::serve(l, karrelage_server::app(state)).await.unwrap();
            });
        });
        format!("http://{}", rx.recv().unwrap())
    }

    /// A price read in the auction house by one client reaches another through the server (and a wrong password is refused).
    #[test]
    fn prices_reach_another_client() {
        use crate::kit::testkit::{hub, msg};
        use crate::prices::{Prices, DEFAULT_SERVER};
        let Some((hub_a, _)) = hub("loutil-remote-a") else { return };
        let (hub_b, _) = hub("loutil-remote-b").unwrap();
        let url = serve("secret-pw");
        let client = |h: &crate::Hub| {
            let r = Remote::new();
            let p = Prices::with_remote(&r);
            r.init(&h.ctx);
            p.init(&h.ctx);
            (r, p)
        };
        let ((ra, pa), (rb, pb)) = (client(&hub_a), client(&hub_b));
        let cfg = |pw: &str| Config { enabled: true, url: url.clone(), password: pw.into(), interval: 60 };
        assert!(ra.call("test", json!({"url": url, "password": "nope"}), &hub_a.ctx).unwrap_err().to_string().contains("mot de passe"));
        assert_eq!(ra.call("test", json!({"url": url, "password": "secret-pw"}), &hub_a.ctx).unwrap()["ok"], true);

        // A reads Rune Tac (20819) at 137 K, then syncs
        let mut m = msg("ExchangeBidHouseItemsEvent", None, unix_secs_f64() - 10.0);
        m.fmt = Some(json!({"kind": "auction_items", "gid": 20819, "rows": [{"uid": 1, "gid": 20819, "prices": [{"qty": 1, "price": 137}]}]}));
        pa.on_message(&m, &hub_a.ctx);
        ra.sync_once(&cfg("secret-pw"));
        assert_eq!(ra.0.status.lock()["sent"], 1);
        // B gets it, marked remote
        rb.sync_once(&cfg("secret-pw"));
        assert_eq!(rb.0.status.lock()["received"], 1);
        let list = pb.call("list", Value::Null, &hub_b.ctx).unwrap();
        let e = list.as_array().unwrap().iter().find(|e| e["gid"] == 20819).cloned().unwrap();
        assert_eq!((e["unit"].clone(), e["unit_src"].clone()), (json!(137), json!("remote")));
        // nothing new: nothing sent or received; the prices of another game server stay apart
        rb.sync_once(&cfg("secret-pw"));
        let st = rb.0.status.lock().clone();
        assert_eq!((st["sent"].clone(), st["received"].clone()), (json!(0), json!(0)));
        pb.call("set_server", json!({"id": 291}), &hub_b.ctx).unwrap();
        assert!(pb.call("list", Value::Null, &hub_b.ctx).unwrap().as_array().unwrap().is_empty());
        pb.call("set_server", json!({"id": DEFAULT_SERVER}), &hub_b.ctx).unwrap();
        // a failed sync keeps the pending prices for the next one
        pa.on_message(&m, &hub_a.ctx);
        ra.sync_once(&cfg("wrong-pw"));
        assert_eq!(ra.0.status.lock()["ok"], false);
        ra.sync_once(&cfg("secret-pw"));
        assert_eq!(ra.0.status.lock()["sent"], 1);
    }
}
