//! Which app uses the camera, for the camera pop-up (Settings > Pop-ups >
//! Camera on and off): what Windows itself writes for its privacy
//! indicator, under `HKCU\...\CapabilityAccessManager\ConsentStore\webcam`
//! (an app's key, packaged ones there and the others under `NonPackaged`,
//! with `LastUsedTimeStart` and a `LastUsedTimeStop` of 0 while it's in use).
//!
//! A thread of its own, `poni-camera`, there only while the pop-up is on,
//! sleeps on `RegNotifyChangeKeyValue` for that key and wakes when Windows
//! writes there (an app took the camera or let it go); nothing polls.

use crate::events::{AppEvent, EventSink};
use std::collections::BTreeSet;
use windows::core::{w, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegGetValueW, RegNotifyChangeKeyValue, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_NOTIFY, KEY_READ, REG_NOTIFY_CHANGE_LAST_SET,
    RRF_RT_REG_QWORD,
};
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForMultipleObjects, INFINITE};

const KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\webcam";

/// The watcher; dropping it ends its thread.
pub struct Camera {
    stop: isize,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Camera {
    pub fn start(sink: EventSink) -> Option<Camera> {
        let stop = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.ok()?;
        let stop_raw = stop.0 as isize;
        let thread = std::thread::Builder::new().name("poni-camera".into()).spawn(move || watch(sink, HANDLE(stop_raw as *mut _))).ok()?;
        Some(Camera { stop: stop_raw, thread: Some(thread) })
    }
}

impl Drop for Camera {
    fn drop(&mut self) {
        let stop = HANDLE(self.stop as *mut _);
        unsafe {
            let _ = SetEvent(stop);
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        unsafe {
            let _ = CloseHandle(stop);
        }
    }
}

fn watch(sink: EventSink, stop: HANDLE) {
    let mut key = HKEY::default();
    if unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, &HSTRING::from(KEY), 0, KEY_READ | KEY_NOTIFY, &mut key) }.is_err() {
        crate::pwarn!("camera: Windows keeps no camera record here");
        return;
    }
    let Ok(changed) = (unsafe { CreateEventW(None, false, false, PCWSTR::null()) }) else {
        unsafe {
            let _ = RegCloseKey(key);
        }
        return;
    };
    // What uses the camera now is the start, not news.
    let mut using = in_use(key);
    loop {
        if unsafe { RegNotifyChangeKeyValue(key, true, REG_NOTIFY_CHANGE_LAST_SET, changed, true) }.is_err() {
            break;
        }
        let r = unsafe { WaitForMultipleObjects(&[stop, changed], false, INFINITE) };
        if r != windows::Win32::Foundation::WAIT_EVENT(WAIT_OBJECT_0.0 + 1) {
            break;
        }
        let now = in_use(key);
        if let Some(app) = now.difference(&using).next() {
            sink.send(AppEvent::Camera { on: true, app: display_name(app) });
        } else if now.is_empty() && !using.is_empty() {
            let app = using.iter().next().map(|a| display_name(a)).unwrap_or_default();
            sink.send(AppEvent::Camera { on: false, app });
        }
        using = now;
    }
    unsafe {
        let _ = CloseHandle(changed);
        let _ = RegCloseKey(key);
    }
}

/// The apps using the camera now (their keys' names, `NonPackaged\` ones with it).
fn in_use(key: HKEY) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for name in subkeys(key) {
        if name == "NonPackaged" {
            let mut np = HKEY::default();
            if unsafe { RegOpenKeyExW(key, w!("NonPackaged"), 0, KEY_READ, &mut np) }.is_ok() {
                for app in subkeys(np) {
                    if busy(np, &app) {
                        out.insert(format!("NonPackaged\\{app}"));
                    }
                }
                unsafe {
                    let _ = RegCloseKey(np);
                }
            }
        } else if busy(key, &name) {
            out.insert(name);
        }
    }
    out
}

fn subkeys(key: HKEY) -> Vec<String> {
    let mut out = Vec::new();
    for i in 0.. {
        let mut name = [0u16; 512];
        let mut len = name.len() as u32;
        if unsafe { RegEnumKeyExW(key, i, PWSTR(name.as_mut_ptr()), &mut len, None, PWSTR::null(), None, None) }.is_err() {
            break;
        }
        out.push(String::from_utf16_lossy(&name[..len as usize]));
    }
    out
}

fn qword(key: HKEY, sub: &str, value: PCWSTR) -> Option<u64> {
    let mut v = 0u64;
    let mut size = 8u32;
    let sub = HSTRING::from(sub);
    if unsafe { RegGetValueW(key, &sub, value, RRF_RT_REG_QWORD, None, Some(&mut v as *mut u64 as *mut _), Some(&mut size)) }.is_err() {
        return None;
    }
    Some(v)
}

/// An app's key says it has the camera now: it started, and hasn't stopped.
fn busy(key: HKEY, app: &str) -> bool {
    qword(key, app, w!("LastUsedTimeStart")).is_some_and(|s| s > 0) && qword(key, app, w!("LastUsedTimeStop")) == Some(0)
}

/// An app's key as people know it: a program's own name ("C:#…#Zoom.exe"
/// is Zoom), a Store app's name from its family ("Microsoft.WindowsCamera_…"
/// is WindowsCamera); one only a code names, none.
pub fn display_name(key: &str) -> String {
    if let Some(path) = key.strip_prefix("NonPackaged\\") {
        let path = path.replace('#', "\\");
        let stem = std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        return crate::screentime::program_name(&path).unwrap_or(stem);
    }
    let family = key.split('_').next().unwrap_or(key);
    let name = family.rsplit('.').next().unwrap_or(family);
    // "51895FA4EA97F": a code, not a name.
    if name.chars().all(|c| c.is_ascii_hexdigit()) {
        return String::new();
    }
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apps_are_named_as_people_know_them() {
        assert_eq!(display_name("Microsoft.WindowsCamera_8wekyb3d8bbwe"), "WindowsCamera");
        assert_eq!(display_name("5319275A.51895FA4EA97F_cv1g1gvanyjgm"), "");
    }
}
