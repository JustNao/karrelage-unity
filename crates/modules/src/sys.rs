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

/// The visible Dofus.exe windows with a title: (window handle, title).
#[cfg(windows)]
fn dofus_windows() -> Vec<(windows_sys::Win32::Foundation::HWND, String)> {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible};
    // (window, pid, title) of every visible top-level window with a title
    unsafe extern "system" fn each(w: HWND, out: LPARAM) -> i32 {
        // SAFETY: `out` is the Vec passed to EnumWindows below, alive for the whole enumeration
        unsafe {
            let mut pid = 0u32;
            if IsWindowVisible(w) != 0 {
                GetWindowThreadProcessId(w, &mut pid);
                let mut buf = [0u16; 256];
                let n = GetWindowTextW(w, buf.as_mut_ptr(), buf.len() as i32);
                if n > 0 {
                    (*(out as *mut Vec<(HWND, u32, String)>)).push((w, pid, String::from_utf16_lossy(&buf[..n as usize])));
                }
            }
            1
        }
    }
    let mut all: Vec<(HWND, u32, String)> = vec![];
    // SAFETY: the callback only pushes into `all`
    unsafe { EnumWindows(Some(each), &mut all as *mut _ as LPARAM) };
    let mut dofus = std::collections::BTreeMap::new();
    all.into_iter().filter(|(_, pid, _)| *dofus.entry(*pid).or_insert_with(|| is_dofus(*pid))).map(|(w, _, t)| (w, t)).collect()
}

/// The titles of the visible Dofus.exe windows (one per game client; empty elsewhere than Windows).
pub fn game_window_titles() -> Vec<String> {
    #[cfg(windows)]
    return dofus_windows().into_iter().map(|(_, t)| t).collect();
    #[cfg(not(windows))]
    vec![]
}

/// The client area (the game's picture, without the frame) of the Dofus window in the foreground, else of the first one, in
/// screen pixels: (x, y, width, height). None without a Dofus window (or elsewhere than Windows).
pub fn game_client_rect() -> Option<(i32, i32, i32, i32)> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{POINT, RECT};
        use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetClientRect, GetForegroundWindow};
        let windows = dofus_windows();
        // SAFETY: reads the foreground window handle
        let front = unsafe { GetForegroundWindow() };
        let w = windows.iter().find(|(w, _)| *w == front).or(windows.first())?.0;
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        let mut at = POINT { x: 0, y: 0 };
        // SAFETY: plain reads of a window's geometry into locals
        let ok = unsafe { GetClientRect(w, &mut r) != 0 && ClientToScreen(w, &mut at) != 0 };
        (ok && r.right > 0 && r.bottom > 0).then_some((at.x, at.y, r.right, r.bottom))
    }
    #[cfg(not(windows))]
    None
}

/// System-wide shortcuts (RegisterHotKey), owned by a thread of their own that waits for them: `on_key(n)` runs (on that
/// thread) each time shortcut `n` is pressed. Keys are set and changed with [`Hotkeys::set`].
pub struct Hotkeys {
    #[cfg(windows)]
    thread: u32,
}

#[cfg(windows)]
const SET_HOTKEY: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 1;

impl Hotkeys {
    /// Starts the thread (None elsewhere than Windows). `on_set(n, ok)` tells whether each `set` could register its key (false:
    /// another program holds it).
    pub fn start(on_key: impl Fn(i32) + Send + 'static, on_set: impl Fn(i32, bool) + Send + 'static) -> Option<Self> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Threading::GetCurrentThreadId;
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_NOREPEAT};
            use windows_sys::Win32::UI::WindowsAndMessaging::{GetMessageW, PeekMessageW, MSG, PM_NOREMOVE, WM_HOTKEY};
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::Builder::new()
                .name("hotkeys".into())
                .spawn(move || {
                    // SAFETY: a message loop on this thread's own queue; the hotkeys belong to this thread (no window)
                    unsafe {
                        let mut msg: MSG = std::mem::zeroed();
                        PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE); // creates the queue before `set` posts to it
                        let _ = tx.send(GetCurrentThreadId());
                        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                            match msg.message {
                                WM_HOTKEY => on_key(msg.wParam as i32),
                                SET_HOTKEY => {
                                    let n = msg.wParam as i32;
                                    let (mods, vk) = ((msg.lParam >> 16) as HOT_KEY_MODIFIERS, (msg.lParam & 0xFFFF) as u32);
                                    UnregisterHotKey(std::ptr::null_mut(), n);
                                    on_set(n, vk == 0 || RegisterHotKey(std::ptr::null_mut(), n, mods | MOD_NOREPEAT, vk) != 0);
                                }
                                _ => {}
                            }
                        }
                    }
                })
                .ok()?;
            Some(Self { thread: rx.recv().ok()? })
        }
        #[cfg(not(windows))]
        {
            let _ = (on_key, on_set);
            None
        }
    }

    /// Binds shortcut `n` (0..0xBFFF): `mods` = MOD_ALT 1 | MOD_CONTROL 2 | MOD_SHIFT 4 | MOD_WIN 8, `vk` = virtual key code
    /// (0 = unbound).
    pub fn set(&self, n: i32, mods: u32, vk: u32) {
        #[cfg(windows)]
        // SAFETY: posts to the hotkeys thread's queue (created before `start` returned)
        unsafe {
            let packed = ((mods & 0xF) << 16 | (vk & 0xFFFF)) as isize;
            windows_sys::Win32::UI::WindowsAndMessaging::PostThreadMessageW(self.thread, SET_HOTKEY, n as usize, packed)
        };
        #[cfg(not(windows))]
        let _ = (n, mods, vk);
    }
}

/// Watches the Escape key while `on` stays true (polled, never taken from the game): `on_press` runs at each press.
pub fn watch_escape(on: std::sync::Arc<std::sync::atomic::AtomicBool>, on_press: impl Fn() + Send + 'static) {
    #[cfg(windows)]
    {
        use std::sync::atomic::Ordering;
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE};
        let _ = std::thread::Builder::new().name("escape".into()).spawn(move || {
            let mut down = true; // a press already held when the watch starts does not count
            while on.load(Ordering::Relaxed) {
                // SAFETY: reads the key's state
                let now = unsafe { GetAsyncKeyState(VK_ESCAPE as i32) } as u16 & 0x8000 != 0;
                if now && !down {
                    on_press();
                }
                down = now;
                std::thread::sleep(Duration::from_millis(40));
            }
        });
    }
    #[cfg(not(windows))]
    let _ = (on, on_press);
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

    /// The shortcuts thread registers a key with Windows and frees it (an unusual combination: Ctrl+Alt+Shift+F24).
    #[cfg(windows)]
    #[test]
    fn registers_a_global_shortcut() {
        let (tx, rx) = std::sync::mpsc::channel();
        let keys = super::Hotkeys::start(|_| {}, move |n, ok| tx.send((n, ok)).unwrap()).unwrap();
        keys.set(7, 1 | 2 | 4, 0x87);
        assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap(), (7, true));
        keys.set(7, 0, 0);
        assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap(), (7, true));
    }

    #[test]
    fn reads_the_character_of_a_window_title() {
        assert_eq!(character_of_title("Hinae - Osamodas - 3.7.12.12").as_deref(), Some("Hinae"));
        assert_eq!(character_of_title("Jean-Kévin - Sacrieur - 3.7.12.12").as_deref(), Some("Jean-Kévin"));
        assert_eq!(character_of_title("Everstone - Steamer - 3.7.12.12 - Release").as_deref(), Some("Everstone"));
        assert_eq!(character_of_title("Dofus"), None);
        assert_eq!(character_of_title("Dofus - 3.7.12.12"), None);
    }
}
