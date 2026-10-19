//! Debounced JSON persistence. A module marks its store dirty on every change (`touch`); a background thread writes it
//! at most every `FLUSH_EVERY`, from a snapshot taken by the module. The capture thread never does disk IO, and the
//! store lock is only held while the snapshot is taken, never while writing.
use parking_lot::Mutex;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

const FLUSH_EVERY: Duration = Duration::from_secs(2);

type Snapshot = Box<dyn Fn() -> Value + Send + Sync>;
type Log = Arc<dyn Fn(String) + Send + Sync>;

struct Inner {
    path: PathBuf,
    dirty: AtomicBool,
    snapshot: Snapshot,
    /// serialises writes (background thread vs `flush_all` at exit)
    writing: Mutex<()>,
    log: Log,
}

impl Inner {
    fn flush(&self) {
        let _w = self.writing.lock();
        if !self.dirty.swap(false, Ordering::SeqCst) {
            return;
        }
        if let Err(e) = save_json(&self.path, &(self.snapshot)()) {
            self.dirty.store(true, Ordering::SeqCst);
            (self.log)(format!("cannot save {}: {e}", self.path.display()));
        }
    }
}

/// Every open store, so `flush_all` can write pending changes when the app exits.
static OPEN: Mutex<Vec<Weak<Inner>>> = Mutex::new(Vec::new());

pub struct Persist {
    inner: Arc<Inner>,
}

impl Persist {
    /// Load `path` (a file that does not parse is kept aside as `<name>.bak` instead of being overwritten later) and
    /// start writing it in the background with `snapshot` whenever `touch` was called.
    pub fn open(path: PathBuf, log: Log, snapshot: impl Fn() -> Value + Send + Sync + 'static) -> (Persist, Option<Value>) {
        let saved = load_or_set_aside(&path, &log);
        let inner = Arc::new(Inner { path, dirty: AtomicBool::new(false), snapshot: Box::new(snapshot), writing: Mutex::new(()), log });
        {
            let mut open = OPEN.lock();
            open.retain(|w| w.strong_count() > 0);
            open.push(Arc::downgrade(&inner));
        }
        let weak = Arc::downgrade(&inner);
        std::thread::spawn(move || loop {
            std::thread::sleep(FLUSH_EVERY);
            let Some(inner) = weak.upgrade() else { return };
            inner.flush();
        });
        (Persist { inner }, saved)
    }

    /// The store changed: write it soon.
    pub fn touch(&self) {
        self.inner.dirty.store(true, Ordering::SeqCst);
    }

    /// Write now if anything changed.
    pub fn flush(&self) {
        self.inner.flush();
    }
}

/// Write every store with pending changes (app exit / restart).
pub fn flush_all() {
    let open: Vec<Arc<Inner>> = OPEN.lock().iter().filter_map(Weak::upgrade).collect();
    for s in open {
        s.flush();
    }
}

fn load_or_set_aside(path: &Path, log: &Log) -> Option<Value> {
    let bytes = std::fs::read(path).ok()?;
    match serde_json::from_slice(&bytes) {
        Ok(v) => Some(v),
        Err(e) => {
            let bak = path.with_extension("json.bak");
            let _ = std::fs::rename(path, &bak);
            log(format!("{} is unreadable ({e}): kept as {}", path.display(), bak.display()));
            None
        }
    }
}

pub fn load_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Atomic write (unique tmp + fsync + rename) so a crash never leaves a half-written or empty file.
pub fn save_json(path: &Path, v: &Value) -> anyhow::Result<()> {
    write_atomic(path, &serde_json::to_vec(v)?)
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let tmp = path.with_extension(format!("tmp{}-{}", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed)));
    let res = (|| {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    Ok(res?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unreadable_store_is_set_aside_and_changes_are_flushed() {
        let dir = std::env::temp_dir().join(format!("loutil-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.json");
        std::fs::write(&path, b"{ not json").unwrap();
        let data = Arc::new(Mutex::new(json!({"a": 1})));
        let d = data.clone();
        let (p, saved) = Persist::open(path.clone(), Arc::new(|_| {}), move || d.lock().clone());
        assert!(saved.is_none());
        assert_eq!(std::fs::read(dir.join("s.json.bak")).unwrap(), b"{ not json");
        assert!(!path.exists()); // nothing written until something changes
        p.touch();
        flush_all();
        assert_eq!(load_json(&path), Some(json!({"a": 1})));
        *data.lock() = json!({"a": 2});
        p.flush(); // not touched: no write
        assert_eq!(load_json(&path), Some(json!({"a": 1})));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
