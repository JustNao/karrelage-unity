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

/// Whether the foreground window belongs to Dofus.exe (false elsewhere than Windows, or when it can't be told).
pub fn game_focused() -> bool {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
        let mut pid = 0u32;
        GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if pid == 0 || h.is_null() {
            return false;
        }
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) != 0;
        CloseHandle(h);
        ok && String::from_utf16_lossy(&buf[..len as usize]).rsplit('\\').next().is_some_and(|n| n.eq_ignore_ascii_case("Dofus.exe"))
    }
    #[cfg(not(windows))]
    false
}

pub fn unix_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

pub fn unix_secs_f64() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

pub fn unix_millis() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis())
}
