//! Data pack updates: at startup the UI asks whether the pack published on GitHub (dist/pack.json) is newer than the loaded one;
//! if so it can download dist/datapack.zip, verify it, unpack it into the user's `datapack/` folder (which wins over the bundled
//! pack when newer) and ask for a restart. Progress is streamed as `update:progress` events.
//! App updates: the latest GitHub release (tag `vX.Y.Z`) newer than this build is offered too; its NSIS installer
//! (`*-setup.exe`) is downloaded, checked against the release's sha256 digest, then run in passive update mode (`/P /UPDATE /R`:
//! no wizard, relaunches the app) while the UI quits. Progress is streamed as `update:app` events.
use crate::{Ctx, Interest, Message, Module};
use anyhow::{anyhow, bail, Context, Result};
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Where the published pack lives (raw files of the repository). Override with KARRELAGE_PACK_URL (tests, forks).
const DEFAULT_BASE: &str = "https://raw.githubusercontent.com/JustNao/karrelage-unity/main/dist";

fn base_url() -> String {
    std::env::var("KARRELAGE_PACK_URL").unwrap_or_else(|_| DEFAULT_BASE.to_string()).trim_end_matches('/').to_string()
}

/// Latest release of the app. Override with KARRELAGE_RELEASE_URL (tests, forks).
const DEFAULT_RELEASE: &str = "https://api.github.com/repos/JustNao/karrelage-unity/releases/latest";

/// The override is honoured in debug builds only: the release names an installer that gets run.
fn release_url() -> String {
    std::env::var("KARRELAGE_RELEASE_URL").ok().filter(|_| cfg!(debug_assertions)).unwrap_or_else(|| DEFAULT_RELEASE.to_string())
}

/// dist/pack.json
#[derive(Debug, Deserialize, Clone)]
pub struct Remote {
    pub generated: u64,
    pub size: u64,
    pub sha256: String,
    pub file: String,
}

/// A newer app release and its Windows installer.
#[derive(Debug, Clone)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub page: String,
    pub url: String,
    pub name: String,
    pub size: u64,
    /// hex sha256 of the installer (GitHub's asset `digest`); a release without one is never installed
    pub sha256: Option<String>,
}

pub struct Update {
    remote: Mutex<Option<Remote>>,
    busy: Arc<AtomicBool>,
    release: Mutex<Option<Release>>,
    /// the verified installer, ready to run
    installer: Arc<Mutex<Option<PathBuf>>>,
}

impl Default for Update {
    fn default() -> Self {
        Self::new()
    }
}

impl Update {
    pub fn new() -> Self {
        Self { remote: Mutex::new(None), busy: Arc::new(AtomicBool::new(false)), release: Mutex::new(None), installer: Arc::new(Mutex::new(None)) }
    }
}

fn agent() -> ureq::Agent {
    crate::sys::http().build()
}

pub fn fetch_remote(base: &str) -> Result<Remote> {
    let resp = agent().get(&format!("{base}/pack.json")).call().context("pack.json")?;
    Ok(serde_json::from_reader(resp.into_reader())?)
}

/// Download, verify and unpack the pack described by `remote` into `<dir>/datapack`.
/// `progress(phase, done, total)` with phase = "download" | "extract" | "done".
pub fn install_pack(base: &str, remote: &Remote, dir: &Path, progress: &dyn Fn(&str, u64, u64)) -> Result<()> {
    let resp = agent().get(&format!("{base}/{}", remote.file)).call().context("téléchargement du pack")?;
    let total = resp.header("Content-Length").and_then(|v| v.parse().ok()).unwrap_or(remote.size).min(remote.size.max(1));
    let mut reader = resp.into_reader();
    let (mut bytes, mut hasher, mut buf) = (Vec::with_capacity(total as usize), Sha256::new(), [0u8; 64 * 1024]);
    let mut last_pct = u64::MAX;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        if bytes.len() + n > remote.size as usize {
            bail!("pack plus gros que prévu ({} octets annoncés)", remote.size);
        }
        bytes.extend_from_slice(&buf[..n]);
        hasher.update(&buf[..n]);
        let pct = bytes.len() as u64 * 100 / total.max(1);
        if pct != last_pct {
            last_pct = pct;
            progress("download", bytes.len() as u64, total);
        }
    }
    let digest = loutil_core::wire::hex(&hasher.finalize());
    if bytes.len() as u64 != remote.size || !digest.eq_ignore_ascii_case(&remote.sha256) {
        bail!("le fichier téléchargé est corrompu (taille ou somme de contrôle incorrecte)");
    }

    let tmp = dir.join("datapack.new");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).context("archive illisible")?;
    let n = zip.len() as u64;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        let Some(name) = f.enclosed_name().and_then(|p| p.file_name().map(|n| n.to_owned())) else { continue };
        if f.is_dir() {
            continue;
        }
        let mut out = std::fs::File::create(tmp.join(name))?;
        std::io::copy(&mut f, &mut out)?;
        out.flush()?;
        progress("extract", i as u64 + 1, n);
    }
    // what we unpacked must be the pack we were promised
    let m: Value = serde_json::from_slice(&std::fs::read(tmp.join("manifest.json")).context("manifest.json absent")?)?;
    if m["generated"].as_u64() != Some(remote.generated) || !tmp.join("protocol.desc").exists() {
        let _ = std::fs::remove_dir_all(&tmp);
        bail!("le pack téléchargé est incomplet");
    }
    let dest = dir.join("datapack");
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&tmp, &dest)?;
    progress("done", n, n);
    Ok(())
}

/// "v1.10.0" / "1.10.0-beta" -> [1, 10, 0] (numeric parts only, so 1.10 > 1.9).
fn version_key(v: &str) -> Vec<u64> {
    v.trim().trim_start_matches(['v', 'V']).split(['-', '+']).next().unwrap_or("").split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

pub fn newer(remote: &str, local: &str) -> bool {
    let (mut r, mut l) = (version_key(remote), version_key(local));
    let n = r.len().max(l.len());
    r.resize(n, 0);
    l.resize(n, 0);
    r > l
}

/// The latest published release, if it is newer than `local` and has a Windows installer.
pub fn fetch_release(url: &str, local: &str) -> Result<Option<Release>> {
    let resp = agent().get(url).set("Accept", "application/vnd.github+json").set("User-Agent", "karrelage").call().context("dernière version")?;
    let r: Value = serde_json::from_reader(resp.into_reader())?;
    let tag = r["tag_name"].as_str().unwrap_or_default();
    if r["draft"].as_bool() == Some(true) || r["prerelease"].as_bool() == Some(true) || !newer(tag, local) {
        return Ok(None);
    }
    let Some(asset) = r["assets"].as_array().into_iter().flatten().find(|a| a["name"].as_str().is_some_and(|n| n.ends_with("-setup.exe"))) else {
        return Ok(None);
    };
    Ok(Some(Release {
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        notes: r["body"].as_str().unwrap_or_default().to_string(),
        page: r["html_url"].as_str().unwrap_or_default().to_string(),
        url: asset["browser_download_url"].as_str().unwrap_or_default().to_string(),
        name: asset["name"].as_str().unwrap_or_default().to_string(),
        size: asset["size"].as_u64().unwrap_or(0),
        sha256: asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(str::to_string),
    }))
}

/// Download the installer of `rel` into `<dir>/app-update/`, checking its size and digest. Returns its path.
pub fn download_release(rel: &Release, dir: &Path, progress: &dyn Fn(u64, u64)) -> Result<PathBuf> {
    // the installer gets run: without the release's checksum to compare it with, it is refused
    let Some(expected) = rel.sha256.as_deref() else { bail!("la version publiée n'indique pas de somme de contrôle : installez-la à la main") };
    let name = Path::new(&rel.name).file_name().ok_or_else(|| anyhow!("nom d'installeur invalide"))?;
    let resp = agent().get(&rel.url).set("User-Agent", "karrelage").call().context("téléchargement de l'installeur")?;
    let total = resp.header("Content-Length").and_then(|v| v.parse().ok()).unwrap_or(rel.size).max(1);
    let mut reader = resp.into_reader();
    let (mut bytes, mut hasher, mut buf) = (Vec::with_capacity(total as usize), Sha256::new(), [0u8; 64 * 1024]);
    let mut last_pct = u64::MAX;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        if rel.size > 0 && bytes.len() + n > rel.size as usize {
            bail!("installeur plus gros que prévu ({} octets annoncés)", rel.size);
        }
        bytes.extend_from_slice(&buf[..n]);
        hasher.update(&buf[..n]);
        let pct = bytes.len() as u64 * 100 / total;
        if pct != last_pct {
            last_pct = pct;
            progress(bytes.len() as u64, total);
        }
    }
    let digest = loutil_core::wire::hex(&hasher.finalize());
    if (rel.size > 0 && bytes.len() as u64 != rel.size) || !digest.eq_ignore_ascii_case(expected) {
        bail!("l'installeur téléchargé est corrompu (taille ou somme de contrôle incorrecte)");
    }
    let out = dir.join("app-update");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out)?;
    let path = out.join(name);
    std::fs::write(&path, &bytes)?;
    Ok(path)
}

impl Module for Update {
    fn id(&self) -> &'static str {
        "update"
    }

    fn interest(&self) -> Interest {
        Interest::Types(&[])
    }

    fn on_message(&self, _m: &Message, _ctx: &Ctx) {}

    fn call(&self, method: &str, _args: Value, ctx: &Ctx) -> Result<Value> {
        match method {
            // is the published pack newer than the loaded one? (never an error for the UI: offline just means "no update")
            "check" => {
                let local = ctx.data.manifest.generated;
                Ok(match fetch_remote(&base_url()) {
                    Ok(r) => {
                        let available = r.generated > local;
                        let out = json!({"available": available, "local": local, "remote": r.generated, "size": r.size});
                        *self.remote.lock() = Some(r);
                        out
                    }
                    Err(e) => json!({"available": false, "local": local, "error": format!("{e:#}")}),
                })
            }
            "install" => {
                let remote = self.remote.lock().clone().ok_or_else(|| anyhow!("aucune mise à jour connue : vérifiez d'abord"))?;
                if self.busy.swap(true, Ordering::SeqCst) {
                    return Err(anyhow!("une mise à jour est déjà en cours"));
                }
                let (emit, busy, dir) = (ctx.emitter(), self.busy.clone(), ctx.dir.clone());
                std::thread::spawn(move || {
                    let report = |phase: &str, done: u64, total: u64| emit("update:progress", json!({"phase": phase, "done": done, "total": total}));
                    let res = install_pack(&base_url(), &remote, &dir, &report);
                    busy.store(false, Ordering::SeqCst);
                    if let Err(e) = res {
                        emit("update:progress", json!({"phase": "error", "message": format!("{e:#}")}));
                    }
                });
                Ok(json!({"started": true}))
            }
            // is a newer app release published? (offline = no update)
            "app_check" => {
                let local = env!("CARGO_PKG_VERSION");
                Ok(match fetch_release(&release_url(), local) {
                    Ok(Some(r)) => {
                        let out = json!({"available": true, "local": local, "version": r.version, "notes": r.notes, "page": r.page, "size": r.size});
                        *self.release.lock() = Some(r);
                        out
                    }
                    Ok(None) => json!({"available": false, "local": local}),
                    Err(e) => json!({"available": false, "local": local, "error": format!("{e:#}")}),
                })
            }
            // download + verify the installer; `update:app` {phase: download | ready | error}
            "app_download" => {
                let rel = self.release.lock().clone().ok_or_else(|| anyhow!("aucune nouvelle version connue : vérifiez d'abord"))?;
                if self.busy.swap(true, Ordering::SeqCst) {
                    return Err(anyhow!("une mise à jour est déjà en cours"));
                }
                let (emit, busy, dir, installer) = (ctx.emitter(), self.busy.clone(), ctx.dir.clone(), self.installer.clone());
                std::thread::spawn(move || {
                    let res = download_release(&rel, &dir, &|done, total| emit("update:app", json!({"phase": "download", "done": done, "total": total})));
                    busy.store(false, Ordering::SeqCst);
                    match res {
                        Ok(p) => {
                            *installer.lock() = Some(p);
                            emit("update:app", json!({"phase": "ready"}));
                        }
                        Err(e) => emit("update:app", json!({"phase": "error", "message": format!("{e:#}")})),
                    }
                });
                Ok(json!({"started": true}))
            }
            // start the downloaded installer (passive, relaunches the app once installed); the UI quits right after
            "app_install" => {
                let path = self.installer.lock().clone().ok_or_else(|| anyhow!("l'installeur n'est pas téléchargé"))?;
                crate::store::flush_all();
                std::process::Command::new(&path).args(["/P", "/UPDATE", "/R"]).spawn().context("lancement de l'installeur")?;
                Ok(json!({"started": true}))
            }
            _ => Err(anyhow!("update: unknown method {method}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;
    use std::net::TcpListener;
    use zip::write::SimpleFileOptions;

    /// Serve `routes` (path -> body) on a local port for `n` requests.
    fn serve(routes: Vec<(&'static str, Vec<u8>)>, n: usize) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            for _ in 0..n {
                let (mut s, _) = l.accept().unwrap();
                let mut line = String::new();
                std::io::BufReader::new(s.try_clone().unwrap()).read_line(&mut line).unwrap();
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                let body = routes.iter().find(|(p, _)| *p == path).map(|(_, b)| b.clone());
                let head = if body.is_some() { "200 OK" } else { "404 Not Found" };
                let body = body.unwrap_or_default();
                let _ = write!(s, "HTTP/1.1 {head}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = s.write_all(&body);
            }
        });
        format!("http://{addr}")
    }

    fn pack_zip(generated: u64) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(Cursor::new(vec![]));
        let o = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        w.start_file("manifest.json", o).unwrap();
        w.write_all(format!("{{\"pack\":1,\"generated\":{generated}}}").as_bytes()).unwrap();
        w.start_file("protocol.desc", o).unwrap();
        w.write_all(&[1, 2, 3, 4]).unwrap();
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn compares_versions() {
        assert!(newer("v1.10.0", "1.9.3") && newer("1.6.1", "1.6") && !newer("v1.6.0", "1.6.0") && !newer("1.5.9", "1.6.0"));
    }

    #[test]
    fn finds_and_downloads_a_newer_release() {
        let exe = b"MZ fake installer".to_vec();
        let rel = |tag: &str, digest: &str, url: &str| {
            json!({"tag_name": tag, "draft": false, "prerelease": false, "body": "notes", "html_url": "page", "assets": [
                {"name": "Karrelage_9.0.0_x64-setup.exe", "size": exe.len(), "digest": digest, "browser_download_url": url}]})
            .to_string()
            .into_bytes()
        };
        let api = serve(vec![("/latest", rel("v0.1.0", "", ""))], 1);
        assert!(fetch_release(&format!("{api}/latest"), "1.0.0").unwrap().is_none());

        let files = serve(vec![("/setup.exe", exe.clone()), ("/setup.exe", exe.clone())], 2);
        let api = serve(vec![("/latest", rel("v9.0.0", &format!("sha256:{}", sha(&exe)), &format!("{files}/setup.exe")))], 1);
        let r = fetch_release(&format!("{api}/latest"), "1.0.0").unwrap().unwrap();
        assert_eq!((r.version.as_str(), r.size), ("9.0.0", exe.len() as u64));
        let dir = std::env::temp_dir().join("karrelage-app-update-test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = download_release(&r, &dir, &|_, _| {}).unwrap();
        assert_eq!(std::fs::read(path).unwrap(), exe);

        let unchecked = Release { sha256: None, ..r.clone() };
        assert!(download_release(&unchecked, &dir, &|_, _| {}).unwrap_err().to_string().contains("somme de contrôle"));
        let bad = Release { sha256: Some("0".repeat(64)), ..r };
        assert!(download_release(&bad, &dir, &|_, _| {}).unwrap_err().to_string().contains("corrompu"));
    }

    fn sha(b: &[u8]) -> String {
        loutil_core::wire::hex(&Sha256::digest(b))
    }

    #[test]
    fn downloads_verifies_and_unpacks() {
        let zip = pack_zip(4242);
        let meta = json!({"pack": 1, "generated": 4242, "size": zip.len(), "sha256": sha(&zip), "file": "datapack.zip"}).to_string().into_bytes();
        let base = serve(vec![("/pack.json", meta), ("/datapack.zip", zip)], 2);
        let remote = fetch_remote(&base).unwrap();
        assert_eq!(remote.generated, 4242);

        let dir = std::env::temp_dir().join("karrelage-update-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("datapack")).unwrap();
        std::fs::write(dir.join("datapack/old.json"), "x").unwrap(); // replaced wholesale
        let phases = Mutex::new(vec![]);
        install_pack(&base, &remote, &dir, &|p, _, _| phases.lock().push(p.to_string())).unwrap();
        assert!(dir.join("datapack/protocol.desc").exists() && !dir.join("datapack/old.json").exists() && !dir.join("datapack.new").exists());
        let p = phases.lock();
        assert!(p.contains(&"download".to_string()) && p.contains(&"extract".to_string()) && p.last().unwrap() == "done");
    }

    #[test]
    fn rejects_a_corrupted_download() {
        let zip = pack_zip(1);
        let base = serve(vec![("/datapack.zip", zip.clone())], 1);
        let remote = Remote { generated: 1, size: zip.len() as u64, sha256: "0".repeat(64), file: "datapack.zip".into() };
        let dir = std::env::temp_dir().join("karrelage-update-test-bad");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let err = install_pack(&base, &remote, &dir, &|_, _, _| {}).unwrap_err();
        assert!(err.to_string().contains("corrompu"), "{err}");
        assert!(!dir.join("datapack").exists());
    }

    /// Network test: the latest real release is seen as newer than 0.0.1 and its installer has a digest.
    #[test]
    #[ignore]
    fn live_github_release_is_found() {
        let r = fetch_release(DEFAULT_RELEASE, "0.0.1").unwrap().unwrap();
        assert!(r.name.ends_with("-setup.exe") && r.sha256.is_some() && r.size > 0, "{r:?}");
    }

    /// Network test against the real repository: `cargo test -p loutil-modules -- --ignored live_github`.
    #[test]
    #[ignore]
    fn live_github_pack_is_reachable_and_valid() {
        let remote = fetch_remote(&base_url()).unwrap();
        let dir = std::env::temp_dir().join("karrelage-update-live");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        install_pack(&base_url(), &remote, &dir, &|_, _, _| {}).unwrap();
        assert!(dir.join("datapack/items.json").exists());
    }
}
