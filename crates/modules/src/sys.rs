//! Small OS / network helpers shared by the modules.
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

pub fn unix_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

pub fn unix_millis() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis())
}
