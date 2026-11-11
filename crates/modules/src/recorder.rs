//! Recorder (developer tool): stores every packet of a category locally for data analysis, one JSONL file per recording under
//! `<app data>/recorder/<category>/<start, unix ms>.jsonl`. Every category is off until the UI turns it on (`set_category`,
//! not persisted here: the setting lives in the UI, which also turns it off when developer mode is off).
//!
//! The capture thread only picks messages by name and hands bytes over (never blocking: a full queue drops and counts). The
//! writer thread decodes, serialises and writes; it only exists while a category is on.
//!
//! A line = the record of `pipeline/sniff.py` (`id t srv dir len kind uid type name conf decoded hex`, with the whole payload
//! in `hex`), so the existing capture replays read these files. The first line is `{"header": ...}` (game, pack and protocol
//! build); `{"dropped": n}` marks a gap of n records lost to a full queue.
use crate::kit::{arg_str, unknown};
use crate::packets::{Entry, Packets};
use crate::{Ctx, Interest, Message, Module};
use anyhow::{anyhow, Result};
use loutil_core::wire;
use loutil_data::Data;
use loutil_proto::Proto;
use parking_lot::Mutex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Arc;
use std::thread::JoinHandle;

const ID: &str = "recorder";
/// Records waiting for the writer (about 1 KB each on the captures). A fight sends tens of messages per second at most.
const QUEUE: usize = 8192;
/// Bumped when the shape of the files changes.
const FORMAT: u32 = 1;

/// What belongs to a category, by clear name. A message of a category goes to the open file of its server connection (`srv`).
///
/// Adding a category = one entry in [`CATEGORIES`], a default in `settings.record` and an entry in `lib/stores/recorder.svelte.ts`.
/// One with `names` only (`within: false`) gets one file per recording: those messages, whenever they come.
pub struct Category {
    pub id: &'static str,
    /// Always start a new file (closing the one open).
    starts: &'static [&'static str],
    /// Always recorded; opens a file when none is (recording switched on, or the app started, in the middle of it).
    names: &'static [&'static str],
    /// Recorded when a file is open, which they then close.
    ends: &'static [&'static str],
    /// Also record everything else while a file is open, unnamed messages included...
    within: bool,
    /// ... except the messages starting with one of these.
    skip: &'static [&'static str],
    /// When not empty: a file closed without one of these is deleted (a recording opened by a message that many other
    /// things send too).
    keep: &'static [&'static str],
}

/// A fight = from its start to its end, whatever comes between: names alone (`Fight*`, `GameActionFight*`) find only 53 % of
/// the messages of work/capture_poison.jsonl, missing sequences, spell modifiers, moves and every type without a clear name
/// (18 %). Fight-named messages out of a fight (`FightOptionUpdateEvent`,
/// `AreaFightModifierUpdateEvent`: the map's other fights) stay out. A fight whose end never came (disconnection, server
/// stream lost: 3 of the captures' 12 fights) closes when the client asks for its map again, which it never did during the 9 others.
const FIGHT: Category = Category {
    id: "fight",
    starts: &["FightStartingEvent"],
    names: &[
        "FightJoinEvent",
        "FightJoinRunningEvent",
        "FightSpectatorJoinEvent",
        "FightSynchronizeEvent",
        "GameActionFightEvent",
        "FightTurnEvent",
        "FightNewRoundEvent",
        "FightBoostAddEvent",
        "FightBoostRemoveEvent",
        "FightRefreshCharacterStatsEvent",
    ],
    ends: &["FightEndEvent", "ContextDestroyEvent", "MapInformationRequest"],
    within: true,
    // the player's conversations and party stay out of a data set; the rest is connection keep-alive
    skip: &["Chat", "Channel", "Party", "Ping", "Pong", "BasicLatency"],
    keep: &[],
};

/// A workshop session (forgemagie, crafts): neither the craft window's opening nor the item put in are named in 3.7, so a file
/// opens at every use of an interactive (the workshop) and holds everything until the exchange is left or the map changes; one
/// without a craft result (harvest, zaap, door, the window left unused) is deleted when it closes.
const FORGEMAGIE: Category = Category {
    id: "forgemagie",
    starts: &["InteractiveUseRequest"],
    names: &["ExchangeCraftResultEvent"],
    ends: &["ExchangeLeaveEvent", "DialogLeaveEvent", "MapInformationRequest"],
    within: true,
    skip: &["Chat", "Channel", "Party", "Ping", "Pong", "BasicLatency"],
    keep: &["ExchangeCraftResultEvent"],
};

/// An auction house visit, from opening its buy or sell window to leaving it, with everything between (unnamed messages
/// included): what a purchase sends back (`ExchangeBidHouseBuyResultEvent`, listing rows changed, inventory, kamas) was never
/// seen on a capture. A listing or purchase message with no file open (recording switched on at the auction house) opens one.
const HDV: Category = Category {
    id: "hdv",
    starts: &["ExchangeBidBuyerStartedEvent", "ExchangeBidSellerStartedEvent"],
    names: &[
        "ExchangeTypesExchangerDescriptionForUserEvent",
        "ExchangeTypesItemsExchangerDescriptionForUserEvent",
        "ExchangeBidHouseBuyRequest",
        "ExchangeBidHouseBuyResultEvent",
        "ExchangeBidHouseInListAddedEvent",
        "ExchangeBidHouseInListUpdatedEvent",
        "ExchangeBidHouseInListRemovedEvent",
        "ExchangeBidHouseGenericItemRemovedEvent",
        "ExchangeBidHouseItemAddedEvent",
    ],
    ends: &["ExchangeLeaveEvent", "DialogLeaveEvent", "MapInformationRequest"],
    within: true,
    skip: &["Chat", "Channel", "Party", "Ping", "Pong", "BasicLatency"],
    keep: &[],
};

pub const CATEGORIES: &[Category] = &[FIGHT, FORGEMAGIE, HDV];

/// What a message does to its category's file.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Step {
    Ignore,
    /// Written to a new file.
    Begin,
    Write,
    /// Written, then the file is closed.
    End,
}

impl Category {
    fn step(&self, open: bool, name: &str) -> Step {
        let has = |list: &[&str]| list.contains(&name);
        if has(self.starts) {
            Step::Begin
        } else if open && has(self.ends) {
            Step::End
        } else if has(self.names) {
            if open {
                Step::Write
            } else {
                Step::Begin
            }
        } else if open && self.within && !self.skip.iter().any(|p| name.starts_with(p)) {
            Step::Write
        } else {
            Step::Ignore
        }
    }
}

/// One record on its way to the writer.
struct Job {
    cat: usize,
    srv: String,
    step: Step,
    t: f64,
    entry: Entry,
    /// A message that keeps its file (`Category::keep`).
    keep: bool,
}

/// Counters of the session, per category.
struct Stats {
    written: Vec<AtomicU64>,
    dropped: Vec<AtomicU64>,
    error: Mutex<Option<String>>,
}

/// A writer thread and what the capture thread needs to feed it.
struct Run {
    on: Vec<bool>,
    /// Per category, the servers with a file open.
    open: Vec<HashSet<String>>,
    tx: SyncSender<Job>,
    writer: Option<JoinHandle<()>>,
}

pub struct Recorder {
    /// Which categories the UI turned on; also serialises the changes.
    enabled: Mutex<Vec<bool>>,
    active: AtomicBool,
    run: Mutex<Option<Run>>,
    stats: Arc<Stats>,
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

impl Recorder {
    pub fn new() -> Self {
        let counters = || CATEGORIES.iter().map(|_| AtomicU64::new(0)).collect();
        Self {
            enabled: Mutex::new(vec![false; CATEGORIES.len()]),
            active: AtomicBool::new(false),
            run: Mutex::new(None),
            stats: Arc::new(Stats { written: counters(), dropped: counters(), error: Mutex::new(None) }),
        }
    }

    /// The capture thread's part: pick, copy the bytes, hand over.
    fn route(&self, m: &Message) {
        let mut run = self.run.lock();
        let Some(run) = run.as_mut() else { return };
        for (cat, c) in CATEGORIES.iter().enumerate() {
            if !run.on[cat] {
                continue;
            }
            let open = run.open[cat].contains(&m.raw.srv);
            let step = c.step(open, &m.name);
            match step {
                Step::Ignore => continue,
                Step::End => {
                    run.open[cat].remove(&m.raw.srv);
                }
                Step::Begin if !open => {
                    run.open[cat].insert(m.raw.srv.clone());
                }
                Step::Begin | Step::Write => {}
            }
            let keep = c.keep.is_empty() || c.keep.contains(&m.name.as_str());
            let job = Job { cat, srv: m.raw.srv.clone(), step, t: m.raw.t, entry: Entry::of(m, usize::MAX), keep };
            if run.tx.try_send(job).is_err() {
                self.stats.dropped[cat].fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Turn a category on or off. The writer is restarted with the new set (a file open in another category ends here and
    /// goes on in a new one); it has written everything queued when this returns.
    fn set_category(&self, cat: usize, on: bool, ctx: &Ctx) {
        let mut enabled = self.enabled.lock();
        if enabled[cat] == on {
            return;
        }
        enabled[cat] = on;
        self.active.store(false, Ordering::Relaxed);
        if let Some(Run { tx, writer, .. }) = self.run.lock().take() {
            drop(tx);
            if let Some(w) = writer {
                let _ = w.join();
            }
        }
        if enabled.iter().any(|on| *on) {
            let (tx, rx) = sync_channel(QUEUE);
            let w = Writer::new(ctx.module_dir(ID), ctx, self.stats.clone());
            let writer = Some(std::thread::spawn(move || w.run(rx)));
            *self.run.lock() = Some(Run { on: enabled.clone(), open: vec![HashSet::new(); CATEGORIES.len()], tx, writer });
            self.active.store(true, Ordering::Relaxed);
        }
        ctx.emit(ID, "changed", Value::Null);
    }

    fn status(&self, ctx: &Ctx) -> Value {
        let enabled = self.enabled.lock().clone();
        let dir = ctx.module_dir(ID);
        let categories: Vec<Value> = CATEGORIES
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let (files, bytes) = folder_size(&dir.join(c.id));
                json!({
                    "id": c.id,
                    "on": enabled[i],
                    "files": files,
                    "bytes": bytes,
                    "written": self.stats.written[i].load(Ordering::Relaxed),
                    "dropped": self.stats.dropped[i].load(Ordering::Relaxed),
                })
            })
            .collect();
        json!({"dir": dir.display().to_string(), "categories": categories, "error": *self.stats.error.lock()})
    }
}

/// Number of recordings of a folder and their total size.
fn folder_size(dir: &Path) -> (u64, u64) {
    let sizes = std::fs::read_dir(dir).into_iter().flatten().flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file());
    sizes.fold((0, 0), |(n, total), m| (n + 1, total + m.len()))
}

/// Identity of the build a file was recorded with: what its bytes need to be read again after an update. `desc` =
/// `pipeline/builds.py`'s fingerprint, naming its snapshot in `pipeline/reference/builds/`. Computed on the writer thread.
fn header(cat: &str, data: &Data) -> Value {
    let game = data.manifest.game.clone().unwrap_or_default();
    let desc = std::fs::read(data.dir.join("protocol.desc")).map(|b| wire::hex(&Sha256::digest(&b)[..8])).ok();
    json!({"header": {
        "format": FORMAT,
        "category": cat,
        "app": env!("CARGO_PKG_VERSION"),
        "game": game.version,
        "game_installed": loutil_data::installed_game_version(),
        "assembly": game.assembly,
        "metadata": game.metadata,
        "desc": desc,
        "pack": data.manifest.pack,
        "pack_generated": data.manifest.generated,
    }})
}

/// A file being written.
struct Recording {
    out: BufWriter<File>,
    path: PathBuf,
    /// Holds a record its category keeps files for (always, for a category without `keep`).
    kept: bool,
}

impl Recording {
    /// Flushed, or deleted when nothing kept it.
    fn close(mut self) -> std::io::Result<()> {
        self.out.flush()?;
        if !self.kept {
            drop(self.out);
            std::fs::remove_file(&self.path)?;
        }
        Ok(())
    }
}

/// The background half: decodes the records and appends them to their files.
struct Writer {
    dir: PathBuf,
    data: Arc<Data>,
    proto: Arc<Proto>,
    stats: Arc<Stats>,
    emit: Arc<dyn Fn(&str, Value) + Send + Sync>,
    log: Arc<dyn Fn(String) + Send + Sync>,
    files: HashMap<(usize, String), Recording>,
    /// Drops already marked in a file, per category.
    marked: Vec<u64>,
}

impl Writer {
    fn new(dir: PathBuf, ctx: &Ctx, stats: Arc<Stats>) -> Self {
        let marked = stats.dropped.iter().map(|d| d.load(Ordering::Relaxed)).collect();
        Self { dir, data: ctx.data.clone(), proto: ctx.proto.clone(), stats, emit: ctx.emitter(), log: ctx.logger(), files: HashMap::new(), marked }
    }

    /// Until the queue's sender is gone. Files are flushed whenever the queue runs empty: at most what is queued is lost in a crash.
    fn run(mut self, rx: Receiver<Job>) {
        while let Ok(job) = rx.recv() {
            self.handle(job);
            while let Ok(job) = rx.try_recv() {
                self.handle(job);
            }
            let flushed: Result<Vec<()>, _> = self.files.values_mut().map(|f| f.out.flush()).collect();
            if let Err(e) = flushed {
                self.failed(e);
            }
        }
    }

    fn handle(&mut self, job: Job) {
        let step = job.step;
        if let Err(e) = self.write(job) {
            self.failed(e);
        }
        if matches!(step, Step::Begin | Step::End) {
            (self.emit)("recorder:changed", Value::Null);
        }
    }

    fn failed(&self, e: std::io::Error) {
        let msg = e.to_string();
        let mut error = self.stats.error.lock();
        if error.as_deref() != Some(&msg) {
            (self.log)(format!("recorder: {msg}"));
        }
        *error = Some(msg);
    }

    fn write(&mut self, job: Job) -> std::io::Result<()> {
        let key = (job.cat, job.srv);
        if job.step == Step::Begin {
            if let Some(f) = self.files.remove(&key) {
                f.close()?;
            }
        }
        if !self.files.contains_key(&key) {
            let file = self.create(job.cat, job.t)?;
            self.files.insert(key.clone(), file);
        }
        let dropped = self.stats.dropped[job.cat].load(Ordering::Relaxed);
        let lost = dropped - self.marked[job.cat];
        self.marked[job.cat] = dropped;
        let mut rec = Packets::render(&job.entry, &self.proto);
        rec["hex"] = json!(wire::hex(&job.entry.raw));
        if let Some(r) = rec.as_object_mut() {
            r.remove("fmt"); // what another module showed in the inspector
        }
        let Some(file) = self.files.get_mut(&key) else { return Ok(()) };
        file.kept |= job.keep;
        if lost > 0 {
            write_line(&mut file.out, &json!({"dropped": lost}))?;
        }
        write_line(&mut file.out, &rec)?;
        self.stats.written[job.cat].fetch_add(1, Ordering::Relaxed);
        if job.step == Step::End {
            if let Some(f) = self.files.remove(&key) {
                f.close()?;
            }
        }
        Ok(())
    }

    /// A new file, its name being the first record's time (append-only, even if two recordings meet on the same millisecond).
    fn create(&self, cat: usize, t: f64) -> std::io::Result<Recording> {
        let id = CATEGORIES[cat].id;
        let dir = self.dir.join(id);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.jsonl", (t * 1000.0) as u64));
        let file = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
        let mut out = BufWriter::new(file);
        write_line(&mut out, &header(id, &self.data))?;
        Ok(Recording { out, path, kept: CATEGORIES[cat].keep.is_empty() })
    }
}

/// The recordings still open end with the writer (category switched off, app closed): flushed, or deleted when nothing kept them.
impl Drop for Writer {
    fn drop(&mut self) {
        for (_, f) in std::mem::take(&mut self.files) {
            if let Err(e) = f.close() {
                self.failed(e);
            }
        }
    }
}

fn write_line(out: &mut impl Write, v: &Value) -> std::io::Result<()> {
    serde_json::to_writer(&mut *out, v)?;
    out.write_all(b"\n")
}

impl Module for Recorder {
    fn id(&self) -> &'static str {
        ID
    }

    fn interest(&self) -> Interest {
        Interest::All
    }

    /// Off (the default): the hub doesn't even hand it the messages.
    fn wants(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    /// Decoded on the writer thread.
    fn needs_decoded(&self) -> bool {
        false
    }

    fn on_message(&self, m: &Message, _ctx: &Ctx) {
        self.route(m);
    }

    fn call(&self, method: &str, args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            "status" => Ok(self.status(ctx)),
            // args: {id, on}
            "set_category" => {
                let id = arg_str(&args, "id")?;
                let cat = CATEGORIES.iter().position(|c| c.id == id).ok_or_else(|| anyhow!("{ID}: unknown category {id}"))?;
                self.set_category(cat, args["on"].as_bool().unwrap_or(false), ctx);
                Ok(self.status(ctx))
            }
            "open_folder" => {
                let dir = ctx.module_dir(ID);
                std::fs::create_dir_all(&dir)?;
                std::process::Command::new(crate::sys::system_tool("explorer.exe")).arg(&dir).spawn()?;
                Ok(Value::Null)
            }
            _ => Err(unknown(ID, method)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::testkit;
    use crate::Hub;
    use loutil_core::RawMessage;

    /// The frame of an event of type `ty`, without content.
    fn frame(ty: &str) -> Vec<u8> {
        let wrap = |tag: u8, b: Vec<u8>| [vec![tag, b.len() as u8], b].concat();
        wrap(0x0a, wrap(0x12, wrap(0x0a, format!("type.ankama.com/{ty}").into_bytes())))
    }

    fn lines(file: &Path) -> Vec<Value> {
        std::fs::read_to_string(file).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }

    /// The recordings of `cat`, oldest first.
    fn recordings(hub: &Hub, cat: &str) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(hub.ctx.module_dir(ID).join(cat)).into_iter().flatten().flatten().map(|e| e.path()).collect();
        files.sort();
        files
    }

    fn set(hub: &Hub, on: bool) {
        hub.call(ID, "set_category", json!({"id": "fight", "on": on})).unwrap();
    }

    #[test]
    fn fight_window() {
        let step = |open, name| FIGHT.step(open, name);
        assert_eq!(step(false, "FightStartingEvent"), Step::Begin);
        assert_eq!(step(true, "FightStartingEvent"), Step::Begin, "an abandoned fight ends at the next one");
        assert_eq!(step(false, "GameActionFightEvent"), Step::Begin, "switched on during a fight");
        assert_eq!(step(true, "GameActionFightEvent"), Step::Write);
        assert_eq!(step(true, "jsy"), Step::Write, "unnamed messages of a fight");
        assert_eq!(step(false, "jsy"), Step::Ignore);
        assert_eq!(step(false, "FightOptionUpdateEvent"), Step::Ignore, "another fight of the map");
        assert_eq!(step(true, "ChatChannelMessageEvent"), Step::Ignore);
        assert_eq!(step(true, "PartyMemberUpdateLightEvent"), Step::Ignore);
        assert_eq!(step(true, "FightEndEvent"), Step::End);
        assert_eq!(step(false, "FightEndEvent"), Step::Ignore);
        assert_eq!(step(true, "ContextDestroyEvent"), Step::End);
        assert_eq!(step(false, "ContextDestroyEvent"), Step::Ignore);
        assert_eq!(step(true, "MapInformationRequest"), Step::End, "the end was lost, the client is back on the map");
        assert_eq!(step(false, "MapInformationRequest"), Step::Ignore);
    }

    /// Workshop sessions: every interactive used opens a file, only those with a craft result stay.
    #[test]
    fn workshop_sessions_kept_with_a_craft_result() {
        let Some((hub, proto)) = testkit::hub("loutil-recorder-forgemagie") else { return };
        let obf = |name: &str| proto.obf_for_clear(name).unwrap_or_else(|| panic!("{name} in the pack"));
        let send = |ty: &str, t: f64, dir| hub.handle(RawMessage { t, srv: "x:5555".into(), dir, payload: frame(ty) });
        hub.call(ID, "set_category", json!({"id": "forgemagie", "on": true})).unwrap();
        send(&obf("InteractiveUseRequest"), 1.0, "C->S"); // a harvest
        send("zzz", 2.0, "S->C");
        send(&obf("InteractiveUseRequest"), 10.0, "C->S"); // the workshop
        send("zzz", 11.0, "S->C"); // window opened, item put in: unnamed
        send(&obf("ChatChannelMessageEvent"), 12.0, "S->C");
        send(&obf("ExchangeCraftResultEvent"), 13.0, "S->C");
        send(&obf("ExchangeLeaveEvent"), 14.0, "S->C");
        send("zzz", 15.0, "S->C"); // after the window
        send(&obf("InteractiveUseRequest"), 20.0, "C->S"); // a zaap
        send(&obf("MapInformationRequest"), 21.0, "C->S");
        send(&obf("InteractiveUseRequest"), 30.0, "C->S"); // left open when switched off
        hub.call(ID, "set_category", json!({"id": "forgemagie", "on": false})).unwrap();

        let files = recordings(&hub, "forgemagie");
        assert_eq!(files.iter().map(|f| f.file_stem().unwrap().to_str().unwrap()).collect::<Vec<_>>(), ["10000"]);
        let names: Vec<String> = lines(&files[0])[1..].iter().map(|r| r["name"].as_str().unwrap().to_string()).collect();
        assert_eq!(names, ["InteractiveUseRequest", "zzz", "ExchangeCraftResultEvent", "ExchangeLeaveEvent"]);
    }

    /// The kind of category to come (`movement`): given messages, whenever they come, in one file.
    #[test]
    fn names_only_category() {
        const LISTINGS: Category = Category {
            id: "listings",
            starts: &[],
            names: &["ExchangeTypesItemsExchangerDescriptionForUserEvent"],
            ends: &[],
            within: false,
            skip: &[],
            keep: &[],
        };
        let listed = LISTINGS.names[0];
        assert_eq!(LISTINGS.step(false, listed), Step::Begin);
        assert_eq!(LISTINGS.step(true, listed), Step::Write);
        assert_eq!(LISTINGS.step(true, "GameActionFightEvent"), Step::Ignore);
    }

    /// An auction house visit: opened by its window (or a listing / purchase when switched on there), everything until it is left.
    #[test]
    fn hdv_window() {
        let step = |open, name| HDV.step(open, name);
        assert_eq!(step(false, "ExchangeBidBuyerStartedEvent"), Step::Begin);
        assert_eq!(step(true, "ExchangeBidSellerStartedEvent"), Step::Begin, "buy and sell windows: a file each");
        assert_eq!(step(false, "ExchangeBidHouseBuyResultEvent"), Step::Begin, "switched on at the auction house");
        assert_eq!(step(true, "ExchangeBidHouseInListUpdatedEvent"), Step::Write);
        assert_eq!(step(true, "jsy"), Step::Write, "unnamed messages of the visit (inventory, kamas...)");
        assert_eq!(step(false, "jsy"), Step::Ignore);
        assert_eq!(step(true, "ChatChannelMessageEvent"), Step::Ignore);
        assert_eq!(step(true, "ExchangeLeaveEvent"), Step::End);
        assert_eq!(step(false, "ExchangeLeaveEvent"), Step::Ignore);
        assert_eq!(step(true, "MapInformationRequest"), Step::End);
        // every name exists in the current pack (a wrong one would never open a file)
        let Some((_, proto)) = testkit::hub("loutil-recorder-hdv") else { return };
        for n in HDV.starts.iter().chain(HDV.names).chain(HDV.ends) {
            assert!(proto.obf_for_clear(n).is_some(), "{n} not in the pack");
        }
    }

    /// A writer not keeping up costs records, counted, never a wait on the capture thread.
    #[test]
    fn full_queue_drops_without_blocking() {
        let rec = Recorder::new();
        let (tx, _rx) = sync_channel(1);
        *rec.run.lock() =
            Some(Run { on: CATEGORIES.iter().map(|c| c.id == "fight").collect(), open: vec![HashSet::new(); CATEGORIES.len()], tx, writer: None });
        for _ in 0..4 {
            rec.route(&testkit::msg("GameActionFightEvent", None, 1.0));
        }
        assert_eq!(rec.stats.dropped[0].load(Ordering::Relaxed), 3);
    }

    /// Off by default: the module isn't handed anything, so nothing is decoded on its behalf or written.
    #[test]
    fn off_by_default() {
        assert!(!Recorder::new().wants());
        assert!(!Recorder::new().needs_decoded());
        let Some((hub, proto)) = testkit::hub("loutil-recorder-off") else { return };
        let start = proto.obf_for_clear("FightStartingEvent").unwrap();
        hub.handle(RawMessage { t: 1.0, srv: "x".into(), dir: "S->C", payload: frame(&start) });
        let status = hub.call(ID, "status", Value::Null).unwrap();
        assert_eq!(status["categories"][0]["on"], false);
        assert_eq!(status["categories"][0]["written"], 0);
        assert!(!hub.ctx.module_dir(ID).exists(), "no folder, no file");
        // nor after being switched off: what comes then is not recorded
        set(&hub, true);
        set(&hub, false);
        hub.handle(RawMessage { t: 2.0, srv: "x".into(), dir: "S->C", payload: frame(&start) });
        assert!(recordings(&hub, "fight").is_empty());
    }

    /// Synthetic fights through the hub: one file each, with what belongs to them and only that.
    #[test]
    fn records_one_file_per_fight() {
        let Some((hub, proto)) = testkit::hub("loutil-recorder-fights") else { return };
        let obf = |name: &str| proto.obf_for_clear(name).unwrap_or_else(|| panic!("{name} in the pack"));
        let send = |ty: &str, t: f64, dir| hub.handle(RawMessage { t, srv: "x:5555".into(), dir, payload: frame(ty) });
        set(&hub, true);
        send(&obf("GameActionFightEvent"), 1.0, "S->C"); // switched on during a fight: recorded as its own
        send(&obf("FightEndEvent"), 2.0, "S->C");
        send("zzz", 3.0, "S->C"); // between two fights
        send(&obf("FightStartingEvent"), 10.0, "S->C");
        send("zzz", 11.0, "S->C"); // a message nobody could name
        send(&obf("ChatChannelMessageEvent"), 12.0, "S->C");
        send(&obf("FightTurnFinishRequest"), 13.0, "C->S");
        send(&obf("FightEndEvent"), 14.0, "S->C");
        set(&hub, false);

        let files = recordings(&hub, "fight");
        assert_eq!(files.iter().map(|f| f.file_stem().unwrap().to_str().unwrap()).collect::<Vec<_>>(), ["1000", "10000"]);
        let fight = lines(&files[1]);
        let names: Vec<&str> = fight[1..].iter().map(|r| r["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["FightStartingEvent", "zzz", "FightTurnFinishRequest", "FightEndEvent"]);

        let h = &fight[0]["header"];
        assert_eq!(h["format"], FORMAT);
        assert_eq!(h["category"], "fight");
        assert_eq!(h["pack"], hub.ctx.data.manifest.pack);
        assert_eq!(h["desc"].as_str().unwrap().len(), 16);

        // the fields of a capture of pipeline/sniff.py, whole payload included
        let turn = &fight[3];
        assert_eq!(turn["dir"], "C->S");
        assert_eq!(turn["t"], 13.0);
        assert_eq!(turn["srv"], "x:5555");
        assert_eq!(turn["kind"], "request");
        assert_eq!(turn["type"], obf("FightTurnFinishRequest").as_str());
        assert_eq!(turn["hex"], wire::hex(&frame(&obf("FightTurnFinishRequest"))).as_str());
        assert_eq!(turn["len"], frame(&obf("FightTurnFinishRequest")).len());
        assert!(turn["decoded"].is_object());

        let status = hub.call(ID, "status", Value::Null).unwrap();
        let fights = &status["categories"][0];
        assert_eq!((fights["files"].as_u64(), fights["written"].as_u64(), fights["dropped"].as_u64()), (Some(2), Some(6), Some(0)));
        assert!(status["error"].is_null());
    }

    /// A gap is marked in the file where it happened.
    #[test]
    fn dropped_records_are_marked() {
        let Some((hub, proto)) = testkit::hub("loutil-recorder-gap") else { return };
        let rec = Recorder::new();
        let mut w = Writer::new(hub.ctx.module_dir(ID), &hub.ctx, rec.stats.clone());
        rec.stats.dropped[0].store(3, Ordering::Relaxed);
        let raw = RawMessage { t: 5.0, srv: "x".into(), dir: "S->C", payload: frame(&proto.obf_for_clear("FightEndEvent").unwrap()) };
        let mut m = testkit::msg("FightEndEvent", None, 5.0);
        m.raw = raw;
        w.handle(Job { cat: 0, srv: "x".into(), step: Step::Write, t: 5.0, entry: Entry::of(&m, usize::MAX), keep: true });
        drop(w);
        let file = lines(&recordings(&hub, "fight")[0]);
        assert_eq!(file[1]["dropped"], 3);
        assert!(file[2]["hex"].is_string());
    }

    /// Real fights replayed (work/capture_*.jsonl, local only, skipped without them). Run with --nocapture for what a fight weighs.
    #[test]
    fn replays_recorded_fights() {
        let Some(pack) = testkit::pack() else { return };
        // capture, fights with their end recorded, fights whose server stream the capture lost before their end
        for (name, ended, lost) in [("capture_harebourg", 4, 1), ("capture_bosses", 5, 1), ("capture_poison", 0, 1)] {
            let rec = pack.join(format!("../work/{name}.jsonl"));
            if !rec.exists() {
                continue;
            }
            let (hub, _) = testkit::hub(&format!("loutil-recorder-{name}")).unwrap();
            set(&hub, true);
            let hex = |s: &str| (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect::<Vec<u8>>();
            for line in std::fs::read_to_string(&rec).unwrap().lines() {
                let r: Value = serde_json::from_str(line).unwrap();
                let (Some(h), Some(len)) = (r["hex"].as_str(), r["len"].as_u64()) else { continue };
                if h.len() / 2 < len as usize {
                    continue; // truncated record (> 4 KB)
                }
                let srv = r["srv"].as_str().unwrap_or("x").to_string();
                let dir = if r["dir"] == "C->S" { "C->S" } else { "S->C" };
                hub.handle(RawMessage { t: r["t"].as_f64().unwrap_or(0.0), srv, dir, payload: hex(h) });
            }
            set(&hub, false);
            let status = hub.call(ID, "status", Value::Null).unwrap();
            assert_eq!(status["categories"][0]["dropped"], 0, "{name}");

            let files = recordings(&hub, "fight");
            let (mut closed, mut unfinished) = (0, 0);
            for f in &files {
                let lines = lines(f);
                let size = std::fs::metadata(f).unwrap().len();
                let names: Vec<&str> = lines[1..].iter().map(|r| r["name"].as_str().unwrap_or("")).collect();
                let last = names.last().copied().unwrap_or("");
                let secs = lines.last().unwrap()["t"].as_f64().unwrap() - lines[1]["t"].as_f64().unwrap();
                eprintln!("{name}: {} records, {size} bytes, {secs:.0} s, ends with {last}", names.len());
                closed += usize::from(last == "FightEndEvent");
                unfinished += usize::from(last == "MapInformationRequest");
                assert!(lines[0]["header"].is_object(), "{name}: header first");
                assert!(names.iter().all(|n| !n.starts_with("Chat") && !n.starts_with("Party")), "{name}: no conversations");
                assert!(lines[1..].iter().all(|r| r["hex"].is_string() && r["t"].is_number() && r["len"].is_number()), "{name}");
            }
            assert_eq!((closed, unfinished, files.len()), (ended, lost, ended + lost), "{name}");
        }
    }
}
