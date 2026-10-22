//! Thin Tauri shell: loads the data pack, wires capture -> hub -> UI events, exposes a generic module API.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use loutil_core::{capture::Capture, pcap::Pcap};
use loutil_data::{read_manifest, Data};
use loutil_modules::Hub;
use loutil_proto::Proto;
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tauri::{AppHandle, Emitter, Manager, State};

const LANG: &str = "fr";

struct Core {
    hub: Arc<Hub>,
    capture: Mutex<Capture>,
    pack: PathBuf,
}

/// `core` / `error` are filled once by the loader thread started in `setup` (the window shows the splash meanwhile).
struct AppState {
    core: OnceLock<Core>,
    error: OnceLock<String>,
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
    let pack = pick_pack(app, &user_dir)
        .ok_or_else(|| anyhow::anyhow!("no data pack found (run pipeline/export_pack.py, or drop one in {}\\datapack)", user_dir.display()))?;
    let data = Arc::new(Data::load(&pack, LANG)?);
    let proto = Arc::new(Proto::load(&pack, user_dir.join("overrides.json"))?);

    let log_fn: Arc<dyn Fn(String) + Send + Sync> = {
        let (app, log) = (app.clone(), log);
        Arc::new(move |line| {
            let mut l = log.lock();
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
    s.core.get().ok_or_else(|| s.error.get().cloned().unwrap_or_else(|| "not initialised".into()))
}

fn status_of(s: &AppState) -> Value {
    let log: Vec<String> = s.log.lock().iter().rev().take(5).rev().cloned().collect();
    let npcap = Pcap::load().err().map(|e| e.to_string());
    match s.core.get() {
        Some(c) => {
            let cap = c.capture.lock();
            json!({
                "ready": true, "on": cap.running(), "tracking": cap.tracking(), "npcapError": npcap, "log": log,
                "modules": c.hub.module_ids(), "pack": c.pack.display().to_string(),
                "packGenerated": c.hub.ctx.data.manifest.generated,
                "mapped": c.hub.ctx.proto.mapped_count(),
            })
        }
        None => json!({"ready": false, "loading": s.error.get().is_none(), "error": s.error.get(), "npcapError": npcap, "log": log}),
    }
}

#[tauri::command]
fn status(s: State<AppState>) -> Value {
    status_of(&s)
}

/// Run blocking work (module calls do disk and network IO: Dofusbook import, icon downloads, update check) on the
/// blocking pool, so it ties up neither the UI thread nor an async worker.
async fn blocking<T: Send + 'static>(app: AppHandle, f: impl FnOnce(&AppState) -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>())).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn module_call(app: AppHandle, module: String, method: String, args: Option<Value>) -> Result<Value, String> {
    blocking(app, move |s| core(s)?.hub.call(&module, &method, args.unwrap_or(Value::Null)).map_err(|e| e.to_string())).await
}

/// Starting a capture loads wpcap.dll and opens every adapter: kept off the UI thread too.
#[tauri::command]
async fn capture_set(app: AppHandle, on: bool) -> Result<Value, String> {
    let handle = app.clone();
    let st = blocking(app, move |s| {
        let c = core(s)?;
        {
            let mut cap = c.capture.lock();
            if on {
                cap.start().map_err(|e| e.to_string())?;
            } else {
                cap.stop();
            }
        }
        Ok(status_of(s))
    })
    .await?;
    let _ = handle.emit("app:status", st.clone());
    Ok(st)
}

/// The main window starts hidden behind the frameless splash window; the frontend calls this once it has loaded.
#[tauri::command]
fn splash_done(app: AppHandle) {
    show_main(&app)
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if !w.is_visible().unwrap_or(true) {
            let _ = w.maximize();
            let _ = w.show();
            let _ = w.set_focus();
        }
    }
    if let Some(s) = app.get_webview_window("splash") {
        let _ = s.close();
    }
}

/// Relaunch the app (after a data pack update).
#[tauri::command]
fn restart_app(app: AppHandle) {
    loutil_modules::flush_all();
    app.restart()
}

/// Quit so the app installer started by `update.app_install` can replace the files (it relaunches the app itself).
#[tauri::command]
fn quit_app(app: AppHandle) {
    loutil_modules::flush_all();
    app.exit(0)
}

/// Data folders (roaming: stores, overrides, packs; local: the webview profile with the UI preferences) used by versions that
/// were still called "loutil": moved once to the new identifier's folders. Runs before any window creates its profile.
fn migrate_legacy_dirs() {
    const OLD: &str = "fr.dofus.loutil.packets";
    const NEW: &str = "fr.dofus.karrelage";
    for var in ["APPDATA", "LOCALAPPDATA"] {
        let Some(base) = std::env::var_os(var).map(PathBuf::from) else { continue };
        let (old, new) = (base.join(OLD), base.join(NEW));
        if old.exists() && !new.exists() {
            let _ = std::fs::rename(&old, &new); // fails harmlessly while an old instance still holds files
        }
    }
}

fn main() {
    migrate_legacy_dirs();
    // the state is registered before any window exists: the webview can call `status` before `setup` has run (release builds load
    // the page faster than setup completes), and a command on unmanaged state fails
    let log = Arc::new(Mutex::new(VecDeque::new()));
    tauri::Builder::default()
        .manage(AppState { core: OnceLock::new(), error: OnceLock::new(), log: log.clone() })
        .setup(move |app| {
            // load the data pack off the UI thread: blocking `setup` left the window unpainted (black + white webview) meanwhile
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let s = handle.state::<AppState>();
                match build_core(&handle, log) {
                    Ok(c) => drop(s.core.set(c)),
                    Err(e) => drop(s.error.set(format!("{e:#}"))),
                }
                let _ = handle.emit("app:status", status_of(&s));
                // safety net: show the main window even if the frontend never calls splash_done (script error)
                std::thread::sleep(std::time::Duration::from_secs(20));
                show_main(&handle);
            });
            Ok(())
        })
        // the floating scan list window must not keep the app alive once the main window is gone
        .on_window_event(|w, e| {
            if w.label() == "main" && matches!(e, tauri::WindowEvent::Destroyed) {
                w.app_handle().exit(0);
            }
        })
        .invoke_handler(tauri::generate_handler![status, module_call, capture_set, splash_done, restart_app, quit_app])
        .build(tauri::generate_context!())
        .expect("error while running Karrelage")
        .run(|_, e| {
            // stores are written in the background: write what is pending before the process ends
            if let tauri::RunEvent::Exit = e {
                loutil_modules::flush_all();
            }
        });
}
