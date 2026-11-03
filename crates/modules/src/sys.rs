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

/// A Dofus window: its handle (for [`focus_game`]) and its client area (the game's picture, without the frame) in screen
/// pixels.
pub struct GameWindow {
    pub handle: isize,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// The Dofus window in the foreground, else the first one. None without a Dofus window (or elsewhere than Windows).
pub fn game_window() -> Option<GameWindow> {
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
        (ok && r.right > 0 && r.bottom > 0).then_some(GameWindow { handle: w as isize, x: at.x, y: at.y, width: r.right, height: r.bottom })
    }
    #[cfg(not(windows))]
    None
}

/// Gives the keyboard focus back to a Dofus window: `handle` from [`game_window`] while it is still a Dofus window, else the
/// first one. Works while this app has the foreground (Windows refuses it to a background process). False when it failed.
pub fn focus_game(handle: Option<isize>) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
        let windows = dofus_windows();
        let w = windows.iter().find(|(w, _)| Some(*w as isize) == handle).or(windows.first());
        // SAFETY: a live top-level window handle just enumerated
        w.is_some_and(|(w, _)| unsafe { SetForegroundWindow(*w) } != 0)
    }
    #[cfg(not(windows))]
    {
        let _ = handle;
        false
    }
}

/// System-wide shortcuts (RegisterHotKey), owned by a thread of their own that waits for them: `on_key(n, true)` runs each
/// time shortcut `n` is pressed, `on_key(n, false)` when its key is let go (polled: Windows only tells the press). Keys are set
/// and changed with [`Hotkeys::set`].
pub struct Hotkeys {
    #[cfg(windows)]
    thread: u32,
}

#[cfg(windows)]
const SET_HOTKEY: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 1;

impl Hotkeys {
    /// Starts the thread (None elsewhere than Windows). `on_set(n, ok)` tells whether each `set` could register its key (false:
    /// another program holds it).
    pub fn start(on_key: impl Fn(i32, bool) + Send + Sync + 'static, on_set: impl Fn(i32, bool) + Send + 'static) -> Option<Self> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Threading::GetCurrentThreadId;
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_NOREPEAT};
            use windows_sys::Win32::UI::WindowsAndMessaging::{GetMessageW, PeekMessageW, MSG, PM_NOREMOVE, WM_HOTKEY};
            let (tx, rx) = std::sync::mpsc::channel();
            let on_key = std::sync::Arc::new(on_key);
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
                                WM_HOTKEY => {
                                    let (n, vk) = (msg.wParam as i32, (msg.lParam >> 16) as i32 & 0xFFFF);
                                    on_key(n, true);
                                    let on_key = on_key.clone();
                                    let _ = std::thread::Builder::new().name("hotkey-release".into()).spawn(move || {
                                        // (in the outer unsafe block: reads the key's state)
                                        while GetAsyncKeyState(vk) as u16 & 0x8000 != 0 {
                                            std::thread::sleep(Duration::from_millis(30));
                                        }
                                        on_key(n, false);
                                    });
                                }
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

/// Virtual key codes of Escape and the left / right mouse buttons (`watch_presses`).
pub const VK_ESCAPE: i32 = 0x1B;
pub const VK_LBUTTON: i32 = 0x01;
pub const VK_RBUTTON: i32 = 0x02;

/// Whether a key (virtual key code, mouse buttons too) is down.
#[cfg(windows)]
fn key_down(vk: i32) -> bool {
    // SAFETY: reads the key's state
    unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(vk) as u16 & 0x8000 != 0 }
}

/// Watches keys / mouse buttons (`vks`) while `on` stays true (polled, never taken from the game): `on_press(vk)` at each
/// press.
pub fn watch_presses(on: std::sync::Arc<std::sync::atomic::AtomicBool>, vks: Vec<i32>, on_press: impl Fn(i32) + Send + 'static) {
    #[cfg(windows)]
    {
        use std::sync::atomic::Ordering;
        let _ = std::thread::Builder::new().name("presses".into()).spawn(move || {
            let mut down: Vec<bool> = vks.iter().map(|_| true).collect(); // a press already held when the watch starts does not count
            while on.load(Ordering::Relaxed) {
                for (vk, was) in vks.iter().zip(down.iter_mut()) {
                    let now = key_down(*vk);
                    if now && !*was {
                        on_press(*vk);
                    }
                    *was = now;
                }
                std::thread::sleep(Duration::from_millis(30));
            }
        });
    }
    #[cfg(not(windows))]
    let _ = (on, vks, on_press);
}

/// Watches key combinations without taking them from the game (unlike `Hotkeys`): `binds` = (mods, vk) as for
/// `Hotkeys::set`, polled while the list is not empty (the thread then ends and clears `running`; start it again after
/// filling the list). `on_press(i)` when combination `i` is pressed while the game has the focus.
pub fn watch_bindings(
    binds: std::sync::Arc<parking_lot::Mutex<Vec<(u32, u32)>>>,
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    on_press: impl Fn(usize) + Send + 'static,
) {
    #[cfg(windows)]
    {
        use std::sync::atomic::Ordering;
        // Alt, Ctrl, Shift, Win (left or right)
        const MODS: [(u32, &[i32]); 4] = [(1, &[0x12]), (2, &[0x11]), (4, &[0x10]), (8, &[0x5B, 0x5C])];
        let _ = std::thread::Builder::new().name("passive-keys".into()).spawn(move || {
            let mut down: Vec<bool> = vec![];
            loop {
                let now = binds.lock().clone();
                if now.is_empty() {
                    running.store(false, Ordering::Relaxed);
                    return;
                }
                down.resize(now.len(), true);
                let mods = MODS.iter().filter(|(_, vks)| vks.iter().any(|vk| key_down(*vk))).fold(0, |m, (bit, _)| m | bit);
                for (i, (m, vk)) in now.iter().enumerate() {
                    let pressed = *vk != 0 && key_down(*vk as i32) && mods == *m;
                    if pressed && !down[i] && game_focused() {
                        on_press(i);
                    }
                    down[i] = pressed;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        });
    }
    #[cfg(not(windows))]
    {
        running.store(false, std::sync::atomic::Ordering::Relaxed);
        let _ = (binds, on_press);
    }
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
        let keys = super::Hotkeys::start(|_, _| {}, move |n, ok| tx.send((n, ok)).unwrap()).unwrap();
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
