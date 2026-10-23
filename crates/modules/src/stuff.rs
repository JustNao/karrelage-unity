//! Stuff: equipment builds imported from Dofusbook links. The build (slots -> game item ids, base characteristics, scrolls,
//! exo bonuses) is stored locally; item / stat icons are downloaded once and cached on disk (the webview CSP forbids remote
//! images, so they are handed to the UI as `data:` URLs). Item stats and set bonuses come from the data pack, not from Dofusbook.
use crate::store::write_atomic;
use crate::sys::{http, no_window, system_tool, unix_millis, unix_secs};
use crate::{Ctx, Interest, Module, Persist};
use anyhow::{anyhow, bail, Context, Result};
use parking_lot::Mutex;
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

const API: &str = "https://www.dofusbook.net/api/stuffs/dofus/public";
const ITEM_ICON: &str = "https://www.dofusbook.net/static/dist/items"; // <picture>-100.webp (also has the recent items Ankama's CDN lacks)
const STAT_ICON: &str = "https://www.dofusbook.net/assets/img/elements";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/152.0.0.0 Safari/537.36";

/// Slots of a build, as named by Dofusbook's `stuffItem`.
const SLOTS: [&str; 17] = ["ar", "bo", "am", "ce", "br", "ch", "ca", "a1", "a2", "d1", "d2", "d3", "d4", "d5", "d6", "fa", "mo"];
/// Stat icons downloaded ahead of time (Dofusbook element codes), reusable by other tabs.
/// (Same codes as `PANELS` in app/src/lib/stuff.ts, checked against a Dofusbook sheet.)
const STAT_CODES: &[&str] = &[
    "vi", "pv", "sa", "fo", "in", "ch", "ag", "pu", "pa", "pm", "po", "ic", "ii", "cc", "pp", "so", "ta", "fu", "epa", "epm", "rpa", "rpm", "pd", "dmg", "dnf",
    "dtf", "dff", "def", "daf", "dc", "dp", "dw", "ds", "dm", "dd", "rn", "rt", "rf", "re", "ra", "rc", "rp", "rnp", "rtp", "rfp", "rep", "rap", "rm", "rd",
    "rw",
];

pub struct Stuff {
    builds: Arc<Mutex<Vec<Value>>>,
    persist: OnceLock<Persist>,
}

impl Default for Stuff {
    fn default() -> Self {
        Self::new()
    }
}

impl Stuff {
    pub fn new() -> Self {
        Self { builds: Default::default(), persist: OnceLock::new() }
    }

    fn path(ctx: &Ctx) -> PathBuf {
        ctx.module_dir("stuff").join("builds.json")
    }

    /// Builds change only on user actions: written right away (outside the lock).
    fn save(&self) {
        if let Some(p) = self.persist.get() {
            p.touch();
            p.flush();
        }
    }
}

fn agent(redirects: u32) -> ureq::Agent {
    http().redirects(redirects).user_agent(UA).build()
}

/// Dofusbook build id from a link: `.../equipement/17833105-db/objets`, `17833105-slug`, a bare id, or a `d-bk.net/fr/d/<short>` short link.
fn parse_id(input: &str) -> Result<i64> {
    let s = input.trim();
    let after = |marker: &str| s.find(marker).map(|i| &s[i + marker.len()..]);
    let digits = |t: &str| -> Option<i64> {
        let d: String = t.chars().take_while(char::is_ascii_digit).collect();
        (d.len() >= 3).then(|| d.parse().ok()).flatten()
    };
    if let Some(id) = after("/equipement/").and_then(digits) {
        return Ok(id);
    }
    if let Some(short) = short_link(s) {
        let resp = agent(0).get(&short).call().context("lien court Dofusbook")?;
        let loc = resp.header("Location").ok_or_else(|| anyhow!("lien court sans redirection"))?.to_string();
        return after_equipement(&loc);
    }
    digits(s).ok_or_else(|| anyhow!("lien Dofusbook non reconnu"))
}

/// A Dofusbook short link (`[https://][www.]d-bk.net/...`) as the https URL to request, else None. Only that host is ever
/// requested: the input is typed or pasted by the user, it must not make the app fetch any other address (LAN included).
fn short_link(s: &str) -> Option<String> {
    let lower = s.to_ascii_lowercase();
    let rest = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://")).unwrap_or(&lower);
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (host, path) = rest.split_at(end);
    (matches!(host, "d-bk.net" | "www.d-bk.net") && path.len() > 1).then(|| format!("https://d-bk.net{}", &s[s.len() - path.len()..]))
}

fn after_equipement(url: &str) -> Result<i64> {
    let i = url.find("/equipement/").ok_or_else(|| anyhow!("lien Dofusbook non reconnu"))? + "/equipement/".len();
    let d: String = url[i..].chars().take_while(char::is_ascii_digit).collect();
    d.parse().map_err(|_| anyhow!("lien Dofusbook non reconnu"))
}

/// GET with browser-like headers. Cloudflare rejects some clients by their TLS fingerprint (HTTP 403 for ureq / rustls where a browser
/// or curl passes), so on 403 the request is retried through the system `curl`, which ships with Windows 10+.
fn get_bytes(url: &str) -> Result<Vec<u8>> {
    let referer = if url.contains("dofusbook.net") { "https://www.dofusbook.net/" } else { "" };
    let mut req = agent(5).get(url).set("Accept", "application/json, image/*");
    if !referer.is_empty() {
        req = req.set("Referer", referer);
    }
    let via_ureq = req.call();
    match via_ureq {
        Ok(resp) => {
            let mut b = vec![];
            std::io::Read::read_to_end(&mut resp.into_reader(), &mut b)?;
            Ok(b)
        }
        Err(ureq::Error::Status(404, _)) => bail!("introuvable (404)"),
        Err(ureq::Error::Status(403, _)) => {
            let mut cmd = std::process::Command::new(system_tool("System32/curl.exe"));
            cmd.args(["-sS", "-L", "--fail", "--max-time", "25", "-A", UA, "-H", "Accept: application/json, image/*"]);
            if !referer.is_empty() {
                cmd.args(["-H", &format!("Referer: {referer}")]);
            }
            cmd.arg(url);
            no_window(&mut cmd);
            let out = cmd.output().context("curl introuvable")?;
            if !out.status.success() {
                bail!("accès refusé par Dofusbook ({})", String::from_utf8_lossy(&out.stderr).trim());
            }
            Ok(out.stdout)
        }
        Err(e) => bail!("{e}"),
    }
}

fn fetch_build(id: i64) -> Result<Value> {
    let bytes = get_bytes(&format!("{API}/{id}")).map_err(|e| anyhow!("Dofusbook injoignable: {e}"))?;
    serde_json::from_slice(&bytes).context("réponse Dofusbook illisible (stuff privé ?)")
}

/// Reduce the API answer to what Karrelage needs. Slot -> {gid, picture, name, level, db_id}.
fn trim_build(api: &Value) -> Result<Value> {
    let stuff = &api["stuff"];
    let items = api["items"].as_array().ok_or_else(|| anyhow!("réponse sans objets"))?;
    let mut slots = Map::new();
    for slot in SLOTS {
        let Some(db_id) = stuff["stuffItem"][slot].as_i64() else { continue };
        let Some(it) = items.iter().find(|i| i["id"].as_i64() == Some(db_id)) else { continue };
        slots.insert(slot.to_string(), json!({"db_id": db_id, "gid": it["official"], "picture": it["picture"], "name": it["name"], "level": it["level"]}));
    }
    let nums = |v: &Value| -> Value {
        Value::Object(v.as_object().map(|o| o.iter().filter(|(_, x)| x.is_number()).map(|(k, x)| (k.clone(), x.clone())).collect()).unwrap_or_default())
    };
    Ok(json!({
        "dofusbook_id": stuff["id"],
        "dofusbook_name": stuff["name"],
        "level": stuff["character_level"],
        "class": stuff["character_class"],
        "avatar": stuff["avatar"],
        "carac": nums(&stuff["stuffCarac"]),
        "fm": nums(&stuff["stuffFm"]["fm"]),
        "slots": slots,
    }))
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len() * 4 / 3 + 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Cached PNG as a data URL; downloads it on first use. `kind` = "item" (key = picture id) | "stat" (key = Dofusbook code).
fn icon(ctx: &Ctx, kind: &str, key: &str) -> Result<String> {
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        bail!("clé d'icône invalide");
    }
    let (dir, ext, url) = match kind {
        "item" => ("items", "webp", format!("{ITEM_ICON}/{key}-100.webp")),
        "stat" => ("stats", "png", format!("{STAT_ICON}/{key}.png")),
        _ => bail!("type d'icône inconnu"),
    };
    let file = ctx.module_dir("stuff").join("icons").join(dir).join(format!("{key}.{ext}"));
    let bytes = match std::fs::read(&file) {
        Ok(b) => b,
        Err(_) => {
            let b = get_bytes(&url).map_err(|e| anyhow!("icône {kind}/{key}: {e}"))?;
            write_atomic(&file, &b)?; // a crash mid-write must not cache a truncated icon forever
            b
        }
    };
    Ok(format!("data:image/{};base64,{}", if ext == "webp" { "webp" } else { "png" }, base64(&bytes)))
}

fn effect_json(ctx: &Ctx, e: &loutil_data::DefaultEffect) -> Value {
    json!({"a": e.a, "min": e.min, "max": e.max, "stat": ctx.data.stat_of(e.a), "label": ctx.data.effect_label(e.a),
           "group": ctx.data.effects.get(&e.a).map(|r| r.group.as_str()), "density": ctx.data.effects.get(&e.a).map(|r| r.density)})
}

impl Module for Stuff {
    fn id(&self) -> &'static str {
        "stuff"
    }

    fn interest(&self) -> Interest {
        Interest::Types(&[])
    }

    fn init(&self, ctx: &Ctx) {
        let builds = self.builds.clone();
        let (persist, saved) = Persist::open(Self::path(ctx), ctx.logger(), move || Value::Array(builds.lock().clone()));
        let _ = self.persist.set(persist);
        if let Some(Value::Array(saved)) = saved {
            *self.builds.lock() = saved;
        }
    }

    fn call(&self, method: &str, args: Value, ctx: &Ctx) -> Result<Value> {
        let id_arg = || args["id"].as_str().map(str::to_owned).ok_or_else(|| anyhow!("stuff: id requis"));
        match method {
            "list" => Ok(Value::Array(self.builds.lock().clone())),
            // args: {url}
            "import" => {
                let url = args["url"].as_str().unwrap_or_default();
                let db_id = parse_id(url)?;
                let mut b = trim_build(&fetch_build(db_id)?)?;
                let name = b["dofusbook_name"].as_str().unwrap_or("Stuff").to_string();
                b["id"] = json!(format!("{db_id}-{}", unix_millis()));
                b["name"] = json!(name);
                b["url"] = json!(format!("https://www.dofusbook.net/fr/equipement/{db_id}-db/objets"));
                b["fetched"] = json!(unix_secs());
                b["targets"] = json!({});
                b["chosen"] = json!({});
                b["ignored"] = json!([]);
                b["owned"] = json!([]);
                self.builds.lock().push(b.clone());
                self.save();
                Ok(b)
            }
            // args: {id, name}
            "rename" => {
                let id = id_arg()?;
                let name = args["name"].as_str().unwrap_or_default().trim().to_string();
                if name.is_empty() {
                    bail!("nom vide");
                }
                let mut bs = self.builds.lock();
                let b = bs.iter_mut().find(|b| b["id"] == id).ok_or_else(|| anyhow!("stuff inconnu"))?;
                b["name"] = json!(name);
                drop(bs);
                self.save();
                Ok(json!({"ok": true}))
            }
            "delete" => {
                let id = id_arg()?;
                self.builds.lock().retain(|b| b["id"] != id);
                self.save();
                Ok(json!({"ok": true}))
            }
            // re-download the build from Dofusbook, keeping name / targets / choices
            "refresh" => {
                let id = id_arg()?;
                let db_id = {
                    let bs = self.builds.lock();
                    bs.iter().find(|b| b["id"] == id).and_then(|b| b["dofusbook_id"].as_i64()).ok_or_else(|| anyhow!("stuff inconnu"))?
                };
                let fresh = trim_build(&fetch_build(db_id)?)?;
                let mut bs = self.builds.lock();
                let b = bs.iter_mut().find(|b| b["id"] == id).ok_or_else(|| anyhow!("stuff inconnu"))?;
                for (k, v) in fresh.as_object().into_iter().flatten() {
                    b[k.as_str()] = v.clone();
                }
                b["fetched"] = json!(unix_secs());
                let out = b.clone();
                drop(bs);
                self.save();
                Ok(out)
            }
            // args: {id, patch: {targets?, chosen?, ignored?}}
            "save_state" => {
                let id = id_arg()?;
                let mut bs = self.builds.lock();
                let b = bs.iter_mut().find(|b| b["id"] == id).ok_or_else(|| anyhow!("stuff inconnu"))?;
                for k in ["targets", "chosen", "ignored", "owned", "own_uid", "fm"] {
                    if let Some(v) = args["patch"].get(k) {
                        b[k] = v.clone();
                    }
                }
                drop(bs);
                self.save();
                Ok(json!({"ok": true}))
            }
            // args: {gids: [..]} -> {items: {gid: {...}}, sets: {setId: {...}}}: stats and sets from the data pack
            "item_info" => {
                let (mut items, mut sets) = (Map::new(), Map::new());
                for g in args["gids"].as_array().into_iter().flatten().filter_map(Value::as_i64) {
                    let Some(it) = ctx.data.items.get(&g) else { continue };
                    items.insert(
                        g.to_string(),
                        json!({"name": ctx.data.item_name(g), "level": it.level, "type_id": it.type_id, "type": ctx.data.type_name(Some(it.type_id)),
                               "set_id": it.set_id, "icon_id": it.icon_id, "effects": it.effects.iter().map(|e| effect_json(ctx, e)).collect::<Vec<_>>()}),
                    );
                    if let Some((sid, set)) = it.set_id.and_then(|s| ctx.data.itemsets.get(&s).map(|x| (s, x))) {
                        sets.entry(sid.to_string()).or_insert_with(|| {
                            json!({"name": set.name_id.and_then(|n| ctx.data.text(n)), "items": set.items,
                                   "effects": set.effects.iter().map(|l| l.iter().map(|e| effect_json(ctx, e)).collect::<Vec<_>>()).collect::<Vec<_>>()})
                        });
                    }
                }
                Ok(json!({"items": items, "sets": sets}))
            }
            // args: {item: [picture ids], stat: [codes]} -> {"item:<id>": dataUrl, "stat:<code>": dataUrl}; failures are skipped
            "icons" => {
                let mut out = Map::new();
                for kind in ["item", "stat"] {
                    for k in args[kind].as_array().into_iter().flatten() {
                        let key = k.as_str().map(str::to_owned).or_else(|| k.as_i64().map(|n| n.to_string())).unwrap_or_default();
                        match icon(ctx, kind, &key) {
                            Ok(u) => {
                                out.insert(format!("{kind}:{key}"), json!(u));
                            }
                            Err(e) => ctx.log(format!("stuff: {e}")),
                        }
                    }
                }
                Ok(Value::Object(out))
            }
            // download every known stat icon (cached) so other tabs can reuse them
            "stat_icons" => {
                let mut out = Map::new();
                for c in STAT_CODES {
                    if let Ok(u) = icon(ctx, "stat", c) {
                        out.insert(c.to_string(), json!(u));
                    }
                }
                Ok(Value::Object(out))
            }
            _ => Err(anyhow!("stuff: méthode inconnue {method}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_links() {
        assert_eq!(parse_id("https://www.dofusbook.net/desktop/fr/equipement/17833105-db/objets").unwrap(), 17833105);
        assert_eq!(parse_id("https://www.dofusbook.net/fr/equipement/17833105-steamer-feu").unwrap(), 17833105);
        assert_eq!(parse_id("17833105").unwrap(), 17833105);
        assert!(parse_id("hello").is_err());
    }

    #[test]
    fn short_links_only_reach_dofusbook() {
        assert_eq!(short_link("https://d-bk.net/fr/d/1CpCj").as_deref(), Some("https://d-bk.net/fr/d/1CpCj"));
        assert_eq!(short_link("D-BK.net/fr/d/1CpCj").as_deref(), Some("https://d-bk.net/fr/d/1CpCj"));
        assert_eq!(short_link("http://www.d-bk.net/fr/d/x").as_deref(), Some("https://d-bk.net/fr/d/x"));
        for bad in ["http://192.168.1.1/x?d-bk.net/", "https://d-bk.net.evil.com/x", "https://evil.com/d-bk.net/x", "https://user@d-bk.net/x", "d-bk.net"] {
            assert_eq!(short_link(bad), None, "{bad}");
        }
    }

    /// Dev helper: `STUFF_API=<api.json> STUFF_OUT=<dir> cargo test -p loutil-modules dump_sample` writes build.json + info.json for offline checks of lib/stuff.ts.
    #[test]
    fn dump_sample() {
        let (Ok(api), Ok(out)) = (std::env::var("STUFF_API"), std::env::var("STUFF_OUT")) else { return };
        let pack = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../datapack");
        let dir = std::env::temp_dir().join("loutil-stuff");
        let data = std::sync::Arc::new(loutil_data::Data::load(&pack, "fr").unwrap());
        let proto = std::sync::Arc::new(loutil_proto::Proto::load(&pack, dir.join("o.json")).unwrap());
        let hub = crate::Hub::new(data, proto, dir, std::sync::Arc::new(|_, _| {}), std::sync::Arc::new(|_| {}));
        let b = trim_build(&serde_json::from_slice(&std::fs::read(api).unwrap()).unwrap()).unwrap();
        let gids: Vec<Value> = b["slots"].as_object().unwrap().values().map(|s| s["gid"].clone()).collect();
        let info = Stuff::new().call("item_info", json!({"gids": gids}), &hub.ctx).unwrap();
        std::fs::write(format!("{out}/build.json"), b.to_string()).unwrap();
        std::fs::write(format!("{out}/info.json"), info.to_string()).unwrap();
    }

    /// Needs the network: `cargo test -p loutil-modules live_import -- --ignored`.
    #[test]
    #[ignore]
    fn live_import() {
        assert_eq!(parse_id("https://d-bk.net/fr/d/1CpCj").unwrap(), 17833105);
        let b = trim_build(&fetch_build(17833105).unwrap()).unwrap();
        assert_eq!(b["slots"].as_object().unwrap().len(), 16);
        assert_eq!(b["slots"]["a1"]["gid"], 14092);
        let dir = std::env::temp_dir().join("loutil-stuff-icons");
        let pack = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../datapack");
        let data = std::sync::Arc::new(loutil_data::Data::load(&pack, "fr").unwrap());
        let proto = std::sync::Arc::new(loutil_proto::Proto::load(&pack, dir.join("o.json")).unwrap());
        let hub = crate::Hub::new(data, proto, dir, std::sync::Arc::new(|_, _| {}), std::sync::Arc::new(|_| {}));
        assert!(icon(&hub.ctx, "item", "9272").unwrap().starts_with("data:image/webp;base64,"));
        assert!(icon(&hub.ctx, "item", "17657").is_ok());
        assert!(icon(&hub.ctx, "stat", "vi").unwrap().len() > 100);
    }

    #[test]
    fn base64_matches_reference() {
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"Man"), "TWFu");
    }
}
