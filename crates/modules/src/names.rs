//! Developer tool: are the message names of the loaded data pack compatible with the game that is installed / running?
//! Static checks on the pack, a fingerprint comparison with the installed game files (sizes only, nothing is read), a live
//! check of the message types seen on the wire, and a button that recomputes the names (pipeline/update_names.py).
use crate::auction::AH_LIST;
use crate::prices::AVG_PRICES;
use crate::{Ctx, Interest, Message, Module};
use anyhow::{anyhow, Result};
use loutil_proto::{Kind, MessageDescriptor};
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Where Ankama installs the game; only file sizes are looked at.
fn game_dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Ankama").join("Dofus-dofus3"))
}

/// Repo root when running from a development checkout (the pipeline lives there), else None.
fn repo_root() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.join("pipeline").join("update_names.py").exists().then_some(root)
}

/// Output of the last pipeline run, kept here so the tab can be closed and reopened while it runs.
#[derive(Default)]
struct Job {
    mode: &'static str,
    /// raw output lines, "@@STEP" progress lines included (capped)
    lines: Vec<String>,
    done: Option<bool>,
}

const JOB_LINES: usize = 2000;

/// Observes message types only while the tab is open (type names and counters, never payloads).
pub struct NamesTab {
    active: AtomicBool,
    running: Arc<AtomicBool>,
    job: Arc<Mutex<Job>>,
    seen: Mutex<BTreeMap<String, u64>>,
}

impl Default for NamesTab {
    fn default() -> Self {
        Self::new()
    }
}

impl NamesTab {
    pub fn new() -> Self {
        Self { active: AtomicBool::new(false), running: Default::default(), job: Default::default(), seen: Mutex::new(BTreeMap::new()) }
    }
}

fn sub_msg(md: &MessageDescriptor, field: &str) -> Option<MessageDescriptor> {
    match md.get_field_by_name(field)?.kind() {
        Kind::Message(m) => Some(m),
        _ => None,
    }
}

fn check(role: &str, ok: bool, detail: impl Into<String>) -> Value {
    json!({"role": role, "ok": ok, "detail": detail.into()})
}

fn fields_check(role: &str, md: &MessageDescriptor, names: Vec<String>, extra_missing: Vec<String>) -> Value {
    let mut missing: Vec<String> = names.into_iter().filter(|n| md.get_field_by_name(n).is_none()).collect();
    missing.extend(extra_missing);
    check(role, missing.is_empty(), if missing.is_empty() { "ok".to_string() } else { format!("introuvables : {}", missing.join(", ")) })
}

/// bindings.json roles and the clear names the modules rely on, resolved against the descriptor set.
fn binding_checks(ctx: &Ctx) -> Vec<Value> {
    let b = &ctx.data.bindings;
    let s = |v: &Value| v.as_str().unwrap_or_default().to_string();
    let list_of = |v: &Value| -> Vec<String> { v.as_array().into_iter().flatten().map(s).collect() };
    let mut out = vec![];

    for (clear, role) in [(AH_LIST, "Message liste HDV"), (AVG_PRICES, "Message prix moyens")] {
        match ctx.proto.obf_for_clear(clear) {
            Some(obf) if ctx.proto.message(&obf).is_some() => out.push(check(role, true, format!("{clear} → {obf}"))),
            Some(obf) => out.push(check(role, false, format!("{clear} → {obf}, absent des schémas"))),
            None => out.push(check(role, false, format!("aucun nom pour {clear}"))),
        }
    }

    // auction list: list message -> item message -> effect message
    let list_name = s(&b["ah_list"]["message"]);
    let list_md = ctx.proto.message(&list_name);
    out.push(check("bindings.ah_list.message", list_md.is_some(), list_name));
    if let Some(list) = &list_md {
        let mut want = vec![s(&b["ah_list"]["items"])];
        want.extend(list_of(&b["ah_list"]["ints"]));
        out.push(fields_check("bindings.ah_list (champs)", list, want, vec![]));
        match sub_msg(list, &s(&b["ah_list"]["items"])) {
            Some(item) => {
                let mut want = vec![s(&b["ah_item"]["prices"]), s(&b["ah_item"]["effects"])];
                want.extend(list_of(&b["ah_item"]["ints"]));
                out.push(fields_check("bindings.ah_item (champs)", &item, want, vec![]));
                match sub_msg(&item, &s(&b["ah_item"]["effects"])) {
                    Some(eff) => {
                        let oneof = s(&b["object_effect"]["oneof"]);
                        let extra = if eff.oneofs().any(|o| o.name() == oneof) { vec![] } else { vec![format!("oneof {oneof}")] };
                        out.push(fields_check("bindings.object_effect (champs)", &eff, list_of(&b["object_effect"]["action"]), extra));
                    }
                    None => out.push(check("bindings.object_effect", false, "message d'effet introuvable")),
                }
            }
            None => out.push(check("bindings.ah_item", false, "message d'objet introuvable")),
        }
    }
    // average prices
    let avg = ctx.proto.obf_for_clear(AVG_PRICES).and_then(|o| ctx.proto.message(&o));
    match avg.as_ref().and_then(|m| sub_msg(m, &s(&b["avg_prices"]["items"]))) {
        Some(entry) => out.push(fields_check("bindings.avg_prices (champs)", &entry, vec![s(&b["avg_prices"]["gid"]), s(&b["avg_prices"]["price"])], vec![])),
        None => out.push(check("bindings.avg_prices (champs)", false, "liste d'entrées introuvable")),
    }
    out
}

/// Pack vs installed game: do the sizes of GameAssembly.dll / global-metadata.dat match the build the protos came from?
fn game_check(ctx: &Ctx) -> Value {
    let Some(fp) = ctx.data.manifest.game.as_ref().filter(|f| f.assembly.is_some() || f.metadata.is_some()) else {
        return json!({"state": "unknown", "detail": "le pack ne contient pas d'empreinte du jeu : régénérez-le (export_pack.py)"});
    };
    let Some(dir) = game_dir().filter(|d| d.exists()) else {
        return json!({"state": "unknown", "detail": "dossier du jeu introuvable"});
    };
    let size = |p: PathBuf| std::fs::metadata(p).ok().map(|m| m.len());
    let cur_a = size(dir.join("GameAssembly.dll"));
    let cur_m = size(dir.join("Dofus_Data/il2cpp_data/Metadata/global-metadata.dat"));
    let cur_v = loutil_data::installed_game_version();
    let same_v = fp.version.is_none() || cur_v.is_none() || fp.version == cur_v;
    let same = same_v && (fp.assembly.is_none() || fp.assembly == cur_a) && (fp.metadata.is_none() || fp.metadata == cur_m);
    let v = |x: &Option<String>| x.clone().unwrap_or_else(|| "?".into());
    json!({
        "state": if same { "match" } else { "changed" },
        "detail": if same {
            format!("le jeu installé ({}) correspond au pack", v(&cur_v))
        } else {
            format!("le jeu installé ({}) a changé depuis la génération du pack ({})", v(&cur_v), v(&fp.version))
        },
        "pack": {"assembly": fp.assembly, "metadata": fp.metadata, "version": fp.version}, "game": {"assembly": cur_a, "metadata": cur_m, "version": cur_v},
    })
}

fn spawn_pipeline(root: PathBuf, mode: &'static str, emit: Arc<dyn Fn(&str, Value) + Send + Sync>, running: Arc<AtomicBool>, job: Arc<Mutex<Job>>) {
    *job.lock() = Job { mode, ..Default::default() };
    // every line is both recorded (for a tab reopened later) and streamed to an open tab
    let emit_log = {
        let (emit, job) = (emit.clone(), job.clone());
        Arc::new(move |l: String| {
            {
                let mut j = job.lock();
                if j.lines.len() >= JOB_LINES {
                    j.lines.drain(..JOB_LINES / 4);
                }
                j.lines.push(l.clone());
            }
            emit("names:log", json!(l));
        })
    };
    std::thread::spawn(move || {
        let mut cmd = Command::new("python");
        cmd.arg("pipeline/update_names.py").current_dir(&root).env("PYTHONUTF8", "1").env("PYTHONUNBUFFERED", "1");
        match mode {
            "full" => drop(cmd.arg("--from-game")),
            "repack" => drop(cmd.arg("--repack")),
            _ => {}
        }
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        crate::sys::no_window(&mut cmd);
        let ok = match cmd.spawn() {
            Ok(mut child) => {
                let mut readers = vec![];
                if let Some(o) = child.stdout.take() {
                    let e = emit_log.clone();
                    readers.push(std::thread::spawn(move || BufReader::new(o).lines().map_while(Result::ok).for_each(|l| e(l))));
                }
                if let Some(o) = child.stderr.take() {
                    let e = emit_log.clone();
                    readers.push(std::thread::spawn(move || BufReader::new(o).lines().map_while(Result::ok).for_each(|l| e(l))));
                }
                for r in readers {
                    let _ = r.join();
                }
                child.wait().map(|s| s.success()).unwrap_or(false)
            }
            Err(e) => {
                emit_log(format!("impossible de lancer python : {e}"));
                false
            }
        };
        job.lock().done = Some(ok);
        running.store(false, Ordering::SeqCst);
        emit("names:done", json!({"ok": ok}));
    });
}

impl Module for NamesTab {
    fn id(&self) -> &'static str {
        "names"
    }

    fn interest(&self) -> Interest {
        Interest::All
    }

    fn wants(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    fn on_message(&self, m: &Message, _ctx: &Ctx) {
        if let Some(t) = &m.type_name {
            *self.seen.lock().entry(t.clone()).or_default() += 1;
        }
    }

    fn call(&self, method: &str, args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            // the tab opened / closed: observation only runs while it is open
            "set_active" => {
                let on = args.get("on").and_then(Value::as_bool).unwrap_or(false);
                self.active.store(on, Ordering::Relaxed);
                if !on {
                    self.seen.lock().clear();
                }
                Ok(json!({"active": on}))
            }
            // output of the current / last pipeline run (mode is "" when nothing ran yet)
            "job" => {
                let j = self.job.lock();
                Ok(json!({"mode": j.mode, "lines": j.lines, "done": j.done, "running": self.running.load(Ordering::SeqCst)}))
            }
            "reset" => {
                self.seen.lock().clear();
                Ok(json!({}))
            }
            "check" => {
                let bindings = binding_checks(ctx);
                let game = game_check(ctx);
                let named = ctx.proto.named();
                let stale: Vec<&str> = named.iter().filter(|(obf, ..)| ctx.proto.message(obf).is_none()).map(|(o, ..)| o.as_str()).collect();
                let seen = self.seen.lock();
                let unknown: Vec<(&String, &u64)> = seen.iter().filter(|(t, _)| ctx.proto.message(t).is_none()).collect();
                let unnamed = seen.keys().filter(|t| ctx.proto.message(t).is_some() && ctx.proto.message_name(t).1.is_none()).count();
                let total: u64 = seen.values().sum();
                let unknown_msgs: u64 = unknown.iter().map(|(_, c)| **c).sum();
                let ratio = if total > 0 { unknown_msgs as f64 / total as f64 } else { 0.0 };
                let live_enough = seen.len() >= 15;

                let bad = bindings.iter().any(|c| c["ok"] == false) || game["state"] == "changed" || (live_enough && ratio >= 0.2);
                let warn = !bad && (game["state"] == "unknown" || !stale.is_empty() || (live_enough && ratio > 0.02) || !live_enough);
                Ok(json!({
                    "verdict": if bad { "incompatible" } else if warn { "uncertain" } else { "compatible" },
                    "pack": {"generated": ctx.data.manifest.generated, "messages": ctx.proto.message_count(), "named": named.len(), "dir": ctx.data.dir.display().to_string()},
                    "game": game, "bindings": bindings,
                    "stale": {"count": stale.len(), "examples": stale.iter().take(8).collect::<Vec<_>>()},
                    "live": {
                        "types": seen.len(), "messages": total, "unknownTypes": unknown.len(), "unknownMessages": unknown_msgs,
                        "ratio": ratio, "enough": live_enough, "unnamedTypes": unnamed,
                        "unknown": unknown.iter().take(10).map(|(t, c)| json!({"type": t, "count": c})).collect::<Vec<_>>(),
                    },
                    "canCompute": repo_root().is_some(), "running": self.running.load(Ordering::Relaxed),
                }))
            }
            // args: {mode: "names" | "full" | "repack"}; runs pipeline/update_names.py, streaming its output as names:log events
            "compute" => {
                let root =
                    repo_root().ok_or_else(|| anyhow!("disponible seulement depuis le dépôt de développement (pipeline/update_names.py introuvable)"))?;
                if self.running.swap(true, Ordering::SeqCst) {
                    return Err(anyhow!("un calcul est déjà en cours"));
                }
                let mode = match args.get("mode").and_then(Value::as_str) {
                    Some("full") => "full",
                    Some("repack") => "repack",
                    _ => "names",
                };
                spawn_pipeline(root, mode, ctx.emitter(), self.running.clone(), self.job.clone());
                Ok(json!({"started": true, "mode": mode}))
            }
            _ => Err(anyhow!("names: unknown method {method}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loutil_data::Data;
    use loutil_proto::Proto;

    /// The freshly exported pack must be self-consistent: every binding and required name resolves.
    #[test]
    fn current_pack_passes_the_static_checks() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let pack = root.join("datapack");
        if !pack.join("manifest.json").exists() {
            return;
        }
        let dir = std::env::temp_dir().join("loutil-names-check");
        let _ = std::fs::remove_dir_all(&dir);
        let data = Arc::new(Data::load(&pack, "fr").unwrap());
        let proto = Arc::new(Proto::load(&pack, dir.join("o.json")).unwrap());
        let hub = crate::Hub::new(data, proto, dir, Arc::new(|_, _| {}), Arc::new(|_| {}));
        let r = hub.call("names", "check", Value::Null).unwrap();
        let failed: Vec<&Value> = r["bindings"].as_array().unwrap().iter().filter(|c| c["ok"] == false).collect();
        assert!(failed.is_empty(), "{failed:?}");
        assert_ne!(r["verdict"], "incompatible", "{r}");
    }
}
