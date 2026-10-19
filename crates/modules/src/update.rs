//! Data pack updates: at startup the UI asks whether the pack published on GitHub (dist/pack.json) is newer than the loaded one;
//! if so it can download dist/datapack.zip, verify it, unpack it into the user's `datapack/` folder (which wins over the bundled
//! pack when newer) and ask for a restart. Progress is streamed as `update:progress` events.
use crate::{Ctx, Interest, Message, Module};
use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

/// Where the published pack lives (raw files of the repository). Override with KARRELAGE_PACK_URL (tests, forks).
const DEFAULT_BASE: &str = "https://raw.githubusercontent.com/JustNao/karrelage-unity/main/dist";

fn base_url() -> String {
    std::env::var("KARRELAGE_PACK_URL").unwrap_or_else(|_| DEFAULT_BASE.to_string()).trim_end_matches('/').to_string()
}

/// dist/pack.json
#[derive(Debug, Deserialize, Clone)]
pub struct Remote {
    pub generated: u64,
    pub size: u64,
    pub sha256: String,
    pub file: String,
}

pub struct Update {
    remote: Mutex<Option<Remote>>,
    busy: Arc<AtomicBool>,
}

impl Update {
    pub fn new() -> Self {
        Self { remote: Mutex::new(None), busy: Arc::new(AtomicBool::new(false)) }
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(8)).timeout_read(Duration::from_secs(20)).build()
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
    let digest: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
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

    fn sha(b: &[u8]) -> String {
        Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
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
