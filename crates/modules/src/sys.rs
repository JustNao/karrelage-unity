//! Small OS / network helpers shared by the modules.
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// HTTP client defaults shared by the modules (callers add redirects / user agent).
pub fn http() -> ureq::AgentBuilder {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(8)).timeout_read(Duration::from_secs(20))
}

/// Child processes must not flash a console window (CREATE_NO_WINDOW).
pub fn no_window(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(cmd, 0x0800_0000);
    cmd
}

/// A program shipped with Windows, by its absolute path under %SystemRoot% (`rel` = "System32/curl.exe", "explorer.exe"), so
/// a same-named file in the app folder or on PATH is never picked; elsewhere, or when it is missing, the bare name.
pub fn system_tool(rel: &str) -> PathBuf {
    let abs = std::env::var_os("SystemRoot").map(|r| PathBuf::from(r).join(rel)).filter(|p| cfg!(windows) && p.is_file());
    abs.unwrap_or_else(|| PathBuf::from(rel.rsplit('/').next().unwrap_or(rel).trim_end_matches(".exe")))
}

pub fn unix_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

pub fn unix_millis() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis())
}
