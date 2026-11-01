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

/// Whether process `pid` runs Dofus.exe.
#[cfg(windows)]
fn is_dofus(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    // SAFETY: plain queries on a process handle opened and closed here
    unsafe {
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
}

/// Whether the foreground window belongs to Dofus.exe (false elsewhere than Windows, or when it can't be told).
pub fn game_focused() -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
        let mut pid = 0u32;
        // SAFETY: reads the foreground window's process id
        unsafe { GetWindowThreadProcessId(GetForegroundWindow(), &mut pid) };
        is_dofus(pid)
    }
    #[cfg(not(windows))]
    false
}

/// The titles of the visible Dofus.exe windows (one per game client; empty elsewhere than Windows).
pub fn game_window_titles() -> Vec<String> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{HWND, LPARAM};
        use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible};
        // (window, pid) of every visible top-level window with a title
        unsafe extern "system" fn each(w: HWND, out: LPARAM) -> i32 {
            // SAFETY: `out` is the Vec passed to EnumWindows below, alive for the whole enumeration
            unsafe {
                let mut pid = 0u32;
                if IsWindowVisible(w) != 0 {
                    GetWindowThreadProcessId(w, &mut pid);
                    let mut buf = [0u16; 256];
                    let n = GetWindowTextW(w, buf.as_mut_ptr(), buf.len() as i32);
                    if n > 0 {
                        (*(out as *mut Vec<(u32, String)>)).push((pid, String::from_utf16_lossy(&buf[..n as usize])));
                    }
                }
                1
            }
        }
        let mut all: Vec<(u32, String)> = vec![];
        // SAFETY: the callback only pushes into `all`
        unsafe { EnumWindows(Some(each), &mut all as *mut _ as LPARAM) };
        let mut dofus = std::collections::BTreeMap::new();
        all.into_iter().filter(|(pid, _)| *dofus.entry(*pid).or_insert_with(|| is_dofus(*pid))).map(|(_, t)| t).collect()
    }
    #[cfg(not(windows))]
    vec![]
}

/// The character of a Dofus window title "Character - Class - Version" (seen: "Everstone - Steamer - 3.7.12.12 - Release";
/// None before a character is chosen: "Dofus").
pub fn character_of_title(title: &str) -> Option<String> {
    let parts: Vec<&str> = title.split(" - ").map(str::trim).collect();
    (parts.len() >= 3 && !parts[0].is_empty() && parts[2..].iter().any(|p| p.chars().any(|c| c.is_ascii_digit()))).then(|| parts[0].to_owned())
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

#[cfg(test)]
mod tests {
    use super::character_of_title;

    #[test]
    fn reads_the_character_of_a_window_title() {
        assert_eq!(character_of_title("Hinae - Osamodas - 3.7.12.12").as_deref(), Some("Hinae"));
        assert_eq!(character_of_title("Jean-Kévin - Sacrieur - 3.7.12.12").as_deref(), Some("Jean-Kévin"));
        assert_eq!(character_of_title("Everstone - Steamer - 3.7.12.12 - Release").as_deref(), Some("Everstone"));
        assert_eq!(character_of_title("Dofus"), None);
        assert_eq!(character_of_title("Dofus - 3.7.12.12"), None);
    }
}
