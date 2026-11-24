//! Screen time's tracker (the Screen time add-on), on the UI thread.
//!
//! The full-screen watcher's foreground hook tells the app when another
//! window comes to the front (`AppEvent::FrontWindow`, only while the add-on
//! is on): the stretch that ends goes to the app that was in front
//! (`poni_core::screentime::Usage`), and a new one starts. The PC locked,
//! its screen off, or nobody touching it for [`IDLE_S`] ends the stretch
//! too, with nothing started until someone is back. A check once a minute
//! (only while the add-on is on and someone is there) notices the idle,
//! cuts the stretch so the page and the saved file keep up, and moves to a
//! new day at midnight.
//!
//! What's in front: the window's process, its program's name (lowercase,
//! the key) and the name its file gives ("Google Chrome"), read once per
//! program. A Store app runs in ApplicationFrameHost's frame: the app's
//! own process is the frame's child.

use poni_core::screentime::Usage;
use std::collections::HashMap;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM};
use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::{EnumChildWindows, GetForegroundWindow, GetWindowThreadProcessId};

/// Nobody touched the PC for this long (seconds): the time since stops
/// counting, back to the last touch.
pub const IDLE_S: u64 = 5 * 60;
/// How often the tracker looks (ms): idle, midnight, and a cut so the page
/// and the file keep up. Only while the add-on is on and someone is there.
pub const CHECK_MS: u32 = 60_000;

/// An app in front.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Front {
    pub key: String,
    pub name: String,
    pub path: String,
}

struct Stretch {
    front: Front,
    since: i64,
    day: String,
}

#[derive(Default)]
pub struct Tracker {
    cur: Option<Stretch>,
    /// Program path -> the name its file gives (read once per program).
    names: HashMap<String, String>,
    /// Paused for idle (nobody touched the PC for [`IDLE_S`]).
    pub idle: bool,
}

impl Tracker {
    /// End the stretch under way at `now` (unix seconds), its time going to
    /// its app. True when anything was added.
    pub fn close(&mut self, usage: &mut Usage, now: i64) -> bool {
        let Some(s) = self.cur.take() else { return false };
        let secs = (now - s.since).max(0) as u64;
        usage.add(&s.day, &s.front.key, &s.front.name, &s.front.path, secs);
        secs > 0
    }

    /// Start a stretch of `front` (None: nothing to count, Hihu itself or the
    /// desktop), counted on `day`.
    pub fn open(&mut self, front: Option<Front>, now: i64, day: &str) {
        self.cur = front.map(|front| Stretch { front, since: now, day: day.to_string() });
    }

    /// The app in front now and since when (for the page's running total).
    pub fn current(&self) -> Option<(&Front, i64, &str)> {
        self.cur.as_ref().map(|s| (&s.front, s.since, s.day.as_str()))
    }

    /// End the stretch and start one for what's in front now (another window
    /// came to the front, or the minute's cut). True when time was added.
    pub fn switch(&mut self, usage: &mut Usage, hwnd: HWND, own_pid: u32, now: i64, day: &str) -> bool {
        let front = self.front_of(hwnd, own_pid);
        // Hihu itself in front (a click on the island): the app before keeps
        // counting, as the island sits on top of it.
        if front.is_none() && self.cur.is_some() && pid_of(hwnd) == Some(own_pid) {
            return false;
        }
        let added = self.close(usage, now);
        self.open(front, now, day);
        added
    }

    /// What's in front now, as the hook would say.
    pub fn foreground(&mut self, own_pid: u32) -> Option<Front> {
        let hwnd = unsafe { GetForegroundWindow() };
        self.front_of(hwnd, own_pid)
    }

    fn front_of(&mut self, hwnd: HWND, own_pid: u32) -> Option<Front> {
        if hwnd.is_invalid() {
            return None;
        }
        let mut pid = pid_of(hwnd)?;
        if pid == own_pid {
            return None;
        }
        let mut path = exe_path(pid)?;
        if file_name(&path).eq_ignore_ascii_case("ApplicationFrameHost.exe") {
            // A Store app: its own process owns a child of the frame.
            let child = store_app_pid(hwnd, pid)?;
            pid = child;
            path = exe_path(pid)?;
        }
        let key = file_name(&path).to_lowercase();
        let name = self.names.entry(path.clone()).or_insert_with(|| description(&path).unwrap_or_else(|| stem(&path))).clone();
        Some(Front { key, name, path })
    }
}

/// Seconds since anyone touched the keyboard, mouse or pen.
pub fn idle_secs() -> u64 {
    let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
        return 0;
    }
    let now = unsafe { GetTickCount() };
    u64::from(now.wrapping_sub(info.dwTime)) / 1000
}

fn pid_of(hwnd: HWND) -> Option<u32> {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    (pid != 0).then_some(pid)
}

fn exe_path(pid: u32) -> Option<String> {
    let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len) }.is_ok();
    unsafe {
        let _ = CloseHandle(h);
    }
    ok.then(|| String::from_utf16_lossy(&buf[..(len as usize).min(buf.len())]))
}

/// The Store app's own process behind ApplicationFrameHost's frame `hwnd`
/// (`frame_pid`): the first child window of another process.
fn store_app_pid(hwnd: HWND, frame_pid: u32) -> Option<u32> {
    struct Find {
        frame: u32,
        found: u32,
    }
    unsafe extern "system" fn each(child: HWND, lp: LPARAM) -> BOOL {
        let f = unsafe { &mut *(lp.0 as *mut Find) };
        match pid_of(child) {
            Some(p) if p != f.frame => {
                f.found = p;
                BOOL(0)
            }
            _ => BOOL(1),
        }
    }
    let mut find = Find { frame: frame_pid, found: 0 };
    unsafe {
        let _ = EnumChildWindows(hwnd, Some(each), LPARAM(&mut find as *mut Find as isize));
    }
    (find.found != 0).then_some(find.found)
}

fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

/// "chrome" for "C:\...\chrome.exe": the name when the file gives none.
fn stem(path: &str) -> String {
    let f = file_name(path);
    let s = f.rsplit_once('.').map_or(f, |(s, _)| s);
    let mut c = s.chars();
    c.next().map_or_else(String::new, |first| first.to_uppercase().chain(c).collect())
}

/// The name the program at `path` gives itself ("Google Chrome"), for the
/// camera pop-up too.
pub fn program_name(path: &str) -> Option<String> {
    description(path).filter(|s| !s.trim().is_empty())
}

/// The name the program's file gives itself (its version resource's
/// FileDescription: "Google Chrome").
fn description(path: &str) -> Option<String> {
    let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let size = GetFileVersionInfoSizeW(PCWSTR(wide.as_ptr()), None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(PCWSTR(wide.as_ptr()), 0, size, data.as_mut_ptr() as *mut _).ok()?;
        // The first language and code page it lists.
        let mut ptr = std::ptr::null_mut();
        let mut len = 0u32;
        let key: Vec<u16> = "\\VarFileInfo\\Translation".encode_utf16().chain(Some(0)).collect();
        if !VerQueryValueW(data.as_ptr() as *const _, PCWSTR(key.as_ptr()), &mut ptr, &mut len).as_bool() || len < 4 {
            return None;
        }
        let pair = std::slice::from_raw_parts(ptr as *const u16, 2);
        let key: Vec<u16> = format!("\\StringFileInfo\\{:04x}{:04x}\\FileDescription", pair[0], pair[1]).encode_utf16().chain(Some(0)).collect();
        if !VerQueryValueW(data.as_ptr() as *const _, PCWSTR(key.as_ptr()), &mut ptr, &mut len).as_bool() || len == 0 {
            return None;
        }
        let s = std::slice::from_raw_parts(ptr as *const u16, len as usize);
        let name = String::from_utf16_lossy(s).trim_end_matches('\0').trim().to_string();
        (!name.is_empty()).then_some(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stretch_goes_to_its_app_when_it_ends() {
        let mut t = Tracker::default();
        let mut u = Usage::default();
        let chrome = Front { key: "chrome.exe".into(), name: "Google Chrome".into(), path: "C:\\chrome.exe".into() };
        t.open(Some(chrome), 100, "2026-11-23");
        assert!(t.close(&mut u, 400));
        assert_eq!(u.total("2026-11-23"), 300);
        // Nothing open: nothing to add.
        assert!(!t.close(&mut u, 500));
    }

    #[test]
    fn names_fall_back_to_the_program() {
        assert_eq!(stem("C:\\Program Files\\Foo\\sonora.exe"), "Sonora");
        assert_eq!(file_name("C:\\a\\b.exe"), "b.exe");
    }
}
