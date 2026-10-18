//! Thin Tauri shell: loads the data pack, wires capture -> hub -> UI events, exposes a generic module API.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use loutil_core::{capture::Capture, pcap::Pcap};
use loutil_data::{read_manifest, Data};
use loutil_modules::Hub;
use loutil_proto::Proto;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

/// Synthetic AH packet for the build the data pack was generated from (pipeline/make_fixtures.py).
const DEMO_AH: &[u8] = include_bytes!("../../../fixtures/ah_demo.bin");
const LANG: &str = "fr";

struct Core {
    hub: Arc<Hub>,
    capture: Mutex<Capture>,
    pack: PathBuf,
}

struct AppState {
    core: Option<Core>,
    error: Option<String>,
    log: Arc<Mutex<VecDeque<String>>>,
}

/// Newest pack wins: a pack dropped in %APPDATA%/<app>/datapack beats the bundled one.
fn pick_pack(app: &AppHandle, user_dir: &Path) -> Option<PathBuf> {
    let mut cands: Vec<PathBuf> = vec![user_dir.join("datapack")];
    if let Ok(r) = app.path().resource_dir() {
        cands.push(r.join("datapack"));
    }
    cands.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../datapack")); // dev checkout
    cands.into_iter().filter_map(|d| Some((read_manifest(&d).ok()?.generated, d))).max_by_key(|x| x.0).map(|x| x.1)
}

fn build_core(app: &AppHandle, log: Arc<Mutex<VecDeque<String>>>) -> anyhow::Result<Core> {
    let user_dir = app.path().app_data_dir()?;
    let pack = pick_pack(app, &user_dir).ok_or_else(|| anyhow::anyhow!("no data pack found (run pipeline/export_pack.py, or drop one in {}\\datapack)", user_dir.display()))?;
    let data = Arc::new(Data::load(&pack, LANG)?);
    let proto = Arc::new(Proto::load(&pack, user_dir.join("overrides.json"))?);

    let log_fn: Arc<dyn Fn(String) + Send + Sync> = {
        let (app, log) = (app.clone(), log);
        Arc::new(move |line| {
            let mut l = log.lock().unwrap();
            if l.len() == 50 {
                l.pop_front();
            }
            l.push_back(line.clone());
            let _ = app.emit("app:log", line);
        })
    };
    let emit: Arc<dyn Fn(&str, Value) + Send + Sync> = {
        let app = app.clone();
        Arc::new(move |name, payload| {
            let _ = app.emit(name, payload);
        })
    };
    let hub = Arc::new(Hub::new(data, proto, user_dir, emit, log_fn.clone()));
    let capture = {
        let hub = hub.clone();
        Capture::new("Dofus.exe", Arc::new(move |raw| hub.handle(raw)), log_fn)
    };
    Ok(Core { hub, capture: Mutex::new(capture), pack })
}

fn core(s: &AppState) -> Result<&Core, String> {
    s.core.as_ref().ok_or_else(|| s.error.clone().unwrap_or_else(|| "not initialised".into()))
}

fn status_of(s: &AppState) -> Value {
    let log: Vec<String> = s.log.lock().unwrap().iter().rev().take(5).rev().cloned().collect();
    let npcap = Pcap::load().err().map(|e| e.to_string());
    match &s.core {
        Some(c) => {
            let cap = c.capture.lock().unwrap();
            json!({
                "ready": true, "on": cap.running(), "tracking": cap.tracking(), "npcapError": npcap, "log": log,
                "modules": c.hub.module_ids(), "pack": c.pack.display().to_string(),
                "packGenerated": c.hub.ctx.data.manifest.generated,
                "mapped": c.hub.ctx.proto.mapped_count(),
            })
        }
        None => json!({"ready": false, "error": s.error, "npcapError": npcap, "log": log}),
    }
}

#[tauri::command]
fn status(s: State<AppState>) -> Value {
    status_of(&s)
}

#[tauri::command]
fn module_call(s: State<AppState>, module: String, method: String, args: Option<Value>) -> Result<Value, String> {
    core(&s)?.hub.call(&module, &method, args.unwrap_or(Value::Null)).map_err(|e| e.to_string())
}

#[tauri::command]
fn capture_set(app: AppHandle, s: State<AppState>, on: bool) -> Result<Value, String> {
    let c = core(&s)?;
    {
        let mut cap = c.capture.lock().unwrap();
        if on {
            cap.start().map_err(|e| e.to_string())?;
        } else {
            cap.stop();
        }
    }
    let st = status_of(&s);
    let _ = app.emit("app:status", st.clone());
    Ok(st)
}

/// Developer helper: push the bundled synthetic AH packet through the whole pipeline.
#[tauri::command]
fn dev_inject(s: State<AppState>) -> Result<Value, String> {
    core(&s)?.capture.lock().unwrap().inject("demo", "S->C", DEMO_AH.to_vec());
    Ok(status_of(&s))
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let log = Arc::new(Mutex::new(VecDeque::new()));
            let (core, error) = match build_core(app.handle(), log.clone()) {
                Ok(c) => (Some(c), None),
                Err(e) => (None, Some(format!("{e:#}"))),
            };
            app.manage(AppState { core, error, log });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![status, module_call, capture_set, dev_inject])
        .run(tauri::generate_context!())
        .expect("error while running Loutil");
}
