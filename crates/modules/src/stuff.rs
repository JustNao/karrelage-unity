//! Stuff: equipment builds imported from Dofusbook links. The build (slots -> game item ids, base characteristics, scrolls,
//! exo bonuses) is stored locally; item / stat icons are downloaded once and cached on disk (the webview CSP forbids remote
//! images, so they are handed to the UI as `data:` URLs). Item stats and set bonuses come from the data pack, not from Dofusbook.
//!
//! Players' builds: the equipment tab of another player's card (click a player -> profile) becomes a build of its own
//! (`player` set, one per character, the latest inspection replacing the previous one), with the exact rolls of each item.
use crate::auction::{parse_effect, stat_effects, ObjectEffect};
use crate::kit::{arg_str, Role, Store};
use crate::store::write_atomic;
use crate::sys::{http, no_window, system_tool, unix_millis, unix_secs};
use crate::{Ctx, Interest, Message, Module};
use anyhow::{anyhow, bail, Context, Result};
use loutil_proto::util::{int, messages};
use loutil_proto::{DynamicMessage, PbValue};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, VecDeque};

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

pub const CARD_VIEW_REQUEST: &str = "CardViewRequest";
pub const CARD_INFO: &str = "CardInfoResponse";
pub const CARD_VIEW: &str = "CardViewResponse";
/// Worn position (as in the inventory, see app/src/lib/dofusbookExport.ts) -> build slot.
const POSITION_SLOTS: [(i64, &str); 17] = [
    (0, "am"),
    (1, "ar"),
    (2, "a1"),
    (3, "ce"),
    (4, "a2"),
    (5, "bo"),
    (6, "ch"),
    (7, "ca"),
    (8, "fa"),
    (9, "d1"),
    (10, "d2"),
    (11, "d3"),
    (12, "d4"),
    (13, "d5"),
    (14, "d6"),
    (15, "br"),
    (16, "mo"),
];
/// Highest level that gives characteristics (above: Omega levels).
const MAX_LEVEL: i64 = 200;
/// Fields of a build the user sets: kept when the player is inspected again.
const USER_FIELDS: [&str; 7] = ["name", "targets", "chosen", "ignored", "owned", "own_uid", "fm"];

/// bindings.json `card`: the player card messages (see its `_doc`).
#[derive(Deserialize)]
struct CardBind {
    view_character: String,
    info_success: String,
    info: String,
    /// the character's two int64 (its id and another): the player is kept under both (the view request gives the id)
    info_ids: Vec<String>,
    info_name: String,
    /// its plain int32 (breed, level, another), told apart by value: see `Stuff::breed_and_level`
    info_ints: Vec<String>,
    view_success: String,
    equipment: String,
    unauthorized: String,
    items: String,
    gid: String,
    effects: String,
}

/// A character seen in a CardInfoResponse.
#[derive(Clone)]
struct Player {
    name: String,
    breed: i64,
    level: i64,
}

/// Which of the card's plain int32 (`CardBind::info_ints`) is the breed: votes per field, for the build whose field names
/// are `fields` (`breed.json`). The other one besides the level is small too (1, 3, 5 seen): both often look like a breed.
#[derive(Default)]
struct BreedVotes {
    fields: Vec<String>,
    votes: Vec<u32>,
}

/// The first value of every repeated int list (two values or more) under `m`: the looks' skins start with the breed's base
/// skin (unless a costume replaces it). Single values (scales: 48, 52...) are left out, they could pass for a skin.
fn list_heads(m: &DynamicMessage, out: &mut std::collections::HashSet<i64>) {
    for (_, v) in m.fields() {
        match v {
            PbValue::List(l) if l.len() >= 2 => match l[0].as_message() {
                Some(_) => l.iter().filter_map(PbValue::as_message).for_each(|x| list_heads(x, out)),
                None => out.extend(loutil_proto::util::as_i64(&l[0])),
            },
            PbValue::List(l) => l.iter().filter_map(PbValue::as_message).for_each(|x| list_heads(x, out)),
            PbValue::Message(x) => list_heads(x, out),
            _ => {}
        }
    }
}

#[derive(Default)]
struct Cards {
    players: HashMap<i64, Player>,
    /// Recent CardViewRequests: (envelope uid, character). Responses carry no character id but the request's uid (checked
    /// on a capture).
    requested: VecDeque<(u64, i64)>,
    /// Character of the last CardInfoResponse (fallback when the request was not seen).
    last_info: Option<i64>,
}

#[derive(Default)]
pub struct Stuff {
    builds: Store<Vec<Value>>,
    breed: Store<BreedVotes>,
    bind: Role<CardBind>,
    effect_bind: Role<ObjectEffect>,
    cards: Mutex<Cards>,
}

impl Stuff {
    pub fn new() -> Self {
        Self::default()
    }

    /// (breed, level) from the character's plain int32, whose fields cannot be told apart after an update. The breed is the
    /// one that is a breed whose base skin the card's look shows (a vote for that field), else the field voted for, else
    /// the first that is a breed; the level is the largest other one (Omega levels go past 200).
    fn breed_and_level(&self, ctx: &Ctx, info: &DynamicMessage, ints: &[i64]) -> (i64, i64) {
        let breeds = &ctx.data.breeds;
        let is_breed = |v: i64| if breeds.is_empty() { (1..=20).contains(&v) } else { breeds.contains_key(&v) };
        let mut heads = std::collections::HashSet::new();
        list_heads(info, &mut heads);
        let by_skin: Vec<usize> = (0..ints.len()).filter(|&i| breeds.get(&ints[i]).is_some_and(|br| br.skins.iter().any(|s| heads.contains(s)))).collect();
        let mut v = self.breed.lock();
        let at = match by_skin[..] {
            [i] => {
                if v.votes.len() == ints.len() {
                    v.votes[i] += 1;
                    drop(v);
                    self.breed.touch();
                }
                Some(i)
            }
            _ => {
                let voted = (0..v.votes.len()).filter(|&i| v.votes[i] > 0).max_by_key(|&i| v.votes[i]);
                voted.filter(|&i| ints.get(i).is_some_and(|x| is_breed(*x))).or_else(|| ints.iter().position(|x| is_breed(*x)))
            }
        };
        let Some(at) = at else { return (0, ints.iter().copied().max().unwrap_or(0)) };
        let level = ints.iter().enumerate().filter(|&(i, _)| i != at).map(|(_, x)| *x).max().unwrap_or(0);
        (ints[at], level)
    }

    /// Builds change only on user actions: written right away (outside the lock).
    fn save(&self) {
        self.builds.flush();
    }

    /// The equipment of a player card as a build (None when the card is another view, or private).
    fn player_build(&self, ctx: &Ctx, b: &CardBind, eb: &ObjectEffect, msg: &DynamicMessage, uid: Option<u64>, t: f64) -> Option<Value> {
        let Some(success) = sub(msg, &b.view_success) else {
            ctx.log("stuff: fiche de joueur refusée par le serveur");
            return None;
        };
        let cards = self.cards.lock();
        // the request with this uid, else (request not seen) the latest one, else the last card opened
        let asked = uid.and_then(|u| cards.requested.iter().rev().find(|(r, _)| *r == u)).or(cards.requested.back()).map(|(_, c)| *c);
        let Some(id) = asked.or(cards.last_info) else {
            ctx.log("stuff: fiche de joueur reçue sans sa demande (personnage inconnu)");
            return None;
        };
        let player = cards.players.get(&id).cloned();
        drop(cards);
        if success.has_field_by_name(&b.unauthorized) {
            ctx.log(format!("stuff: équipement privé (personnage {id})"));
            return None;
        }
        let equipment = sub(&success, &b.equipment)?; // another tab of the card
        let (mut slots, mut rolls) = (Map::new(), Map::new());
        if let Some(PbValue::Map(items)) = equipment.get_field_by_name(&b.items).as_deref() {
            for (pos, v) in items {
                let (Some(pos), Some(it)) = (pos.as_i32(), v.as_message()) else { continue };
                let Some(&(_, slot)) = POSITION_SLOTS.iter().find(|(p, _)| *p == pos as i64) else { continue };
                let gid = int(it, &b.gid);
                let Some(info) = ctx.data.items.get(&gid) else { continue };
                let stats = stat_effects(messages(it, &b.effects).iter().map(|e| parse_effect(&ctx.data, eb, e)).collect());
                slots.insert(slot.into(), json!({"db_id": 0, "gid": gid, "picture": info.icon_id, "name": ctx.data.item_name(gid), "level": info.level}));
                // shaped like an auction lot (as inventory items are), so the UI counts it with the lot helpers
                rolls.insert(slot.into(), json!({"uid": -(pos as i64) - 1, "gid": gid, "prices": [], "effects": ctx.data.compare_effects(stats, Some(gid))}));
            }
        }
        let name = player.as_ref().map(|p| p.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| format!("Joueur {id}"));
        // the card's level counts Omega levels (360 seen): cosmetic past 200, where the characteristics stop
        let level = player.as_ref().map(|p| p.level).filter(|l| *l > 0).unwrap_or(MAX_LEVEL).clamp(1, MAX_LEVEL);
        Some(json!({
            "id": format!("player-{id}"), "player": {"id": id, "name": name, "seen": t, "level": level}, "name": name, "url": "",
            "level": level, "class": player.as_ref().map(|p| p.breed).unwrap_or(0),
            "fetched": unix_secs(), "carac": {}, "fm": {}, "slots": slots, "rolls": rolls,
            "targets": {}, "chosen": {}, "ignored": [], "owned": [],
        }))
    }

    /// Store a player's build, replacing that player's previous one (the user's settings on it kept).
    fn upsert(&self, mut b: Value) -> Value {
        let mut bs = self.builds.lock();
        if let Some(old) = bs.iter_mut().find(|x| x["id"] == b["id"]) {
            for k in USER_FIELDS {
                if let Some(v) = old.get(k) {
                    b[k] = v.clone();
                }
            }
            *old = b.clone();
        } else {
            bs.push(b.clone());
        }
        drop(bs);
        self.builds.touch();
        b
    }
}

/// Singular message field `name` of `m`, when set.
fn sub(m: &DynamicMessage, name: &str) -> Option<DynamicMessage> {
    if !m.has_field_by_name(name) {
        return None;
    }
    m.get_field_by_name(name)?.as_message().cloned()
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
        Interest::Types(&[CARD_VIEW_REQUEST, CARD_INFO, CARD_VIEW])
    }

    fn on_message(&self, m: &Message, ctx: &Ctx) {
        let (Some(b), Some(eb), Some(msg)) = (self.bind.get(ctx, "card"), self.effect_bind.get(ctx, "object_effect"), m.decoded.as_ref()) else { return };
        match m.name.as_str() {
            CARD_VIEW_REQUEST => {
                let mut c = self.cards.lock();
                c.requested.push_back((m.env.as_ref().and_then(|e| e.uid).unwrap_or(0), int(msg, &b.view_character)));
                if c.requested.len() > 32 {
                    c.requested.pop_front();
                }
            }
            CARD_INFO => {
                let Some(info) = sub(msg, &b.info_success).and_then(|s| sub(&s, &b.info)) else { return };
                let ids: Vec<i64> = b.info_ids.iter().map(|f| int(&info, f)).filter(|id| *id != 0).collect();
                let name = info.get_field_by_name(&b.info_name).and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default();
                let (breed, level) = self.breed_and_level(ctx, &info, &b.info_ints.iter().map(|f| int(&info, f)).collect::<Vec<_>>());
                let mut c = self.cards.lock();
                // the asked character, when its request was seen first; else every id (the view request will name one)
                let asked: Vec<i64> = ids.iter().copied().filter(|id| c.requested.iter().any(|(_, r)| r == id)).collect();
                for id in if asked.is_empty() { &ids } else { &asked } {
                    c.players.insert(*id, Player { name: name.clone(), breed, level });
                }
                c.last_info = asked.first().or(ids.first()).copied();
            }
            CARD_VIEW => {
                if let Some(build) = self.player_build(ctx, b, eb, msg, m.env.as_ref().and_then(|e| e.uid), m.raw.t) {
                    ctx.log(format!(
                        "stuff: build de {} lu ({} objets)",
                        build["name"].as_str().unwrap_or("?"),
                        build["slots"].as_object().map_or(0, |s| s.len())
                    ));
                    ctx.emit("stuff", "player", self.upsert(build));
                }
            }
            _ => {}
        }
    }

    fn init(&self, ctx: &Ctx) {
        if let Some(Value::Array(saved)) = self.builds.open(ctx, "stuff", "builds.json", |b| json!(b)) {
            *self.builds.lock() = saved;
        }
        let saved = self.breed.open(ctx, "stuff", "breed.json", |v| json!({"fields": v.fields, "votes": v.votes}));
        if let Some(b) = self.bind.get(ctx, "card") {
            let mut v = self.breed.lock();
            v.fields = b.info_ints.clone();
            v.votes = vec![0; b.info_ints.len()];
            // the votes of another build (other field names) are dropped
            if let Some(saved) = saved.filter(|s| s["fields"] == json!(b.info_ints)) {
                if let Ok(x) = serde_json::from_value::<Vec<u32>>(saved["votes"].clone()) {
                    if x.len() == b.info_ints.len() {
                        v.votes = x;
                    }
                }
            }
        }
    }

    fn call(&self, method: &str, args: Value, ctx: &Ctx) -> Result<Value> {
        let id_arg = || arg_str(&args, "id").map(str::to_owned);
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
            _ => Err(crate::kit::unknown("stuff", method)),
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
        let Some((hub, _)) = crate::kit::testkit::hub("loutil-stuff") else { return };
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
        let (hub, _) = crate::kit::testkit::hub("loutil-stuff-icons").expect("data pack");
        assert!(icon(&hub.ctx, "item", "9272").unwrap().starts_with("data:image/webp;base64,"));
        assert!(icon(&hub.ctx, "item", "17657").is_ok());
        assert!(icon(&hub.ctx, "stat", "vi").unwrap().len() > 100);
    }

    /// Synthetic player card: CardInfoResponse (name), CardViewRequest (character), CardViewResponse (equipment with one
    /// rolled item), then a second inspection of the same player replacing the build but keeping its user settings.
    #[test]
    fn inspected_player_becomes_a_build() {
        use crate::kit::testkit;
        use loutil_proto::{MapKey, MessageDescriptor};
        let Some((hub, proto)) = testkit::hub("loutil-stuff-card") else { return };
        let ctx = &hub.ctx;
        let b: CardBind = serde_json::from_value(ctx.data.bindings["card"].clone()).unwrap();
        let eb: ObjectEffect = serde_json::from_value(ctx.data.bindings["object_effect"].clone()).unwrap();
        let md = |clear: &str| proto.message(&proto.obf_for_clear(clear).unwrap_or_else(|| panic!("{clear} named"))).unwrap();
        let field = |m: &MessageDescriptor, f: &str| m.get_field_by_name(f).unwrap_or_else(|| panic!("field {f}")).kind().as_message().unwrap().clone();
        let (info_md, view_md) = (md(CARD_INFO), md(CARD_VIEW));

        let mut player = DynamicMessage::new(field(&field(&info_md, &b.info_success), &b.info));
        player.set_field_by_name(&b.info_ids[1], PbValue::I64(4242));
        player.set_field_by_name(&b.info_name, PbValue::String("Toto".into()));
        let mut success = DynamicMessage::new(field(&info_md, &b.info_success));
        success.set_field_by_name(&b.info, PbValue::Message(player));
        let mut info = DynamicMessage::new(info_md);
        info.set_field_by_name(&b.info_success, PbValue::Message(success));
        let mut req = DynamicMessage::new(md(CARD_VIEW_REQUEST));
        req.set_field_by_name(&b.view_character, PbValue::I64(4242));

        // a hat (position 6) with its first effect rolled at its minimum
        let gid = *ctx
            .data
            .items
            .keys()
            .find(|g| ctx.data.is_equipment(**g) && ctx.data.items[g].effects.first().is_some_and(|e| e.min < e.max && e.a == 125))
            .unwrap();
        let first = ctx.data.items[&gid].effects[0].clone();
        let (succ_md, equip_md) = (field(&view_md, &b.view_success), field(&field(&view_md, &b.view_success), &b.equipment));
        let item_md = equip_md.get_field_by_name(&b.items).unwrap().kind().as_message().unwrap().map_entry_value_field().kind().as_message().unwrap().clone();
        let fx_md = field(&item_md, &b.effects);
        // the effect's value: the oneof's integer member
        let value_f = fx_md.oneofs().find(|o| o.name() == eb.oneof).unwrap().fields().find(|f| matches!(f.kind(), loutil_proto::Kind::Int32)).unwrap();
        let view = |gid: i64| {
            let mut fx = DynamicMessage::new(fx_md.clone());
            fx.set_field_by_name(&eb.action[0], PbValue::I32(first.a as i32));
            fx.set_field(&value_f, PbValue::I32(first.min as i32));
            let mut it = DynamicMessage::new(item_md.clone());
            it.set_field_by_name(&b.gid, PbValue::I32(gid as i32));
            it.set_field_by_name(&b.effects, PbValue::List(vec![PbValue::Message(fx)]));
            let mut eq = DynamicMessage::new(equip_md.clone());
            eq.set_field_by_name(&b.items, PbValue::Map([(MapKey::I32(6), PbValue::Message(it))].into_iter().collect()));
            let mut s = DynamicMessage::new(succ_md.clone());
            s.set_field_by_name(&b.equipment, PbValue::Message(eq));
            let mut v = DynamicMessage::new(view_md.clone());
            v.set_field_by_name(&b.view_success, PbValue::Message(s));
            v
        };

        let stuff = Stuff::new();
        let send = |name: &str, d: DynamicMessage| stuff.on_message(&testkit::msg(name, Some(d), 7.0), ctx);
        send(CARD_INFO, info);
        send(CARD_VIEW_REQUEST, req);
        send(CARD_VIEW, view(gid));
        let list = stuff.call("list", Value::Null, ctx).unwrap();
        let bs = list.as_array().unwrap();
        assert_eq!(bs.len(), 1);
        assert_eq!((bs[0]["id"].clone(), bs[0]["name"].clone(), bs[0]["player"]["id"].clone()), (json!("player-4242"), json!("Toto"), json!(4242)));
        assert_eq!(bs[0]["slots"]["ch"]["gid"], json!(gid));
        let roll = &bs[0]["rolls"]["ch"]["effects"][0];
        assert_eq!((roll["action"].clone(), roll["hi"].clone()), (json!(first.a), json!(first.min)), "exact roll kept: {roll}");

        stuff.call("save_state", json!({"id": "player-4242", "patch": {"targets": {"1": 12}}}), ctx).unwrap();
        send(CARD_VIEW, view(gid));
        let list = stuff.call("list", Value::Null, ctx).unwrap();
        assert_eq!(list.as_array().unwrap().len(), 1, "the latest inspection replaces the previous one");
        assert_eq!(list[0]["targets"], json!({"1": 12}), "user settings kept");
    }

    /// Dev check on a capture: `CARD_CAPTURE=<pipeline/sniff.py .jsonl with player cards opened> cargo test -p loutil-modules
    /// real_cards -- --nocapture` replays it and prints each inspected player's name / breed / level.
    #[test]
    fn real_cards() {
        let Ok(rec) = std::env::var("CARD_CAPTURE") else { return };
        let Some((hub, _)) = crate::kit::testkit::hub("loutil-stuff-real-cards") else { return };
        let hex = |s: &str| (0..s.len() / 2).map(|i| u8::from_str_radix(s.get(2 * i..2 * i + 2)?, 16).ok()).collect::<Option<Vec<u8>>>();
        for line in std::fs::read_to_string(rec).unwrap().lines() {
            let Ok(r) = serde_json::from_str::<Value>(line) else { continue };
            let (Some(h), Some(len)) = (r["hex"].as_str(), r["len"].as_u64()) else { continue };
            let Some(payload) = hex(h).filter(|p| p.len() >= len as usize) else { continue }; // truncated record
            let dir = if r["dir"] == "C->S" { "C->S" } else { "S->C" };
            hub.handle(loutil_core::RawMessage { t: r["t"].as_f64().unwrap_or(0.0), srv: "x".into(), dir, payload });
        }
        for b in hub.call("stuff", "list", Value::Null).unwrap().as_array().unwrap().iter().filter(|b| !b["player"].is_null()) {
            eprintln!("{} breed {} level {}", b["player"]["name"], b["class"], b["player"]["level"]);
        }
    }

    /// Dev check on a capture: `[CARD_INFO=<CardInfoResponse record .json>] CARD_REQ=<CardViewRequest hex> CARD_VIEW=<CardViewResponse record .json> cargo test -p loutil-modules
    /// real_card -- --nocapture` runs both payloads through the hub and prints the build.
    #[test]
    fn real_card() {
        let (Ok(req), Ok(view)) = (std::env::var("CARD_REQ"), std::env::var("CARD_VIEW")) else { return };
        let Some((hub, _)) = crate::kit::testkit::hub("loutil-stuff-real-card") else { return };
        let hex = |s: &str| (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect::<Vec<u8>>();
        let rec: Value = serde_json::from_slice(&std::fs::read(view).unwrap()).unwrap();
        let raw = |dir: &'static str, payload: Vec<u8>| loutil_core::RawMessage { t: 1.0, srv: "x".into(), dir, payload };
        if let Ok(info) = std::env::var("CARD_INFO") {
            let rec: Value = serde_json::from_slice(&std::fs::read(info).unwrap()).unwrap();
            hub.handle(raw("S->C", hex(rec["hex"].as_str().unwrap())));
        }
        hub.handle(raw("C->S", hex(&req)));
        hub.handle(raw("S->C", hex(rec["hex"].as_str().unwrap())));
        let list = hub.call("stuff", "list", Value::Null).unwrap();
        println!("{}", serde_json::to_string_pretty(&list).unwrap());
        assert_eq!(list.as_array().unwrap().len(), 1);
    }

    #[test]
    fn base64_matches_reference() {
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"Man"), "TWFu");
    }
}
