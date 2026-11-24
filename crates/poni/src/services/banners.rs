//! Windows' own notification pop-ups, hidden for the apps the island shows
//! (Notifications' "Hide Windows' pop-ups"): each app's `ShowBanner` under
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Notifications\Settings\<AUMID>`,
//! the switch Windows Settings > Notifications > the app > "Show
//! notification banners" writes. Its notifications still go to Windows'
//! notification centre, where the island reads them.
//!
//! What each app had before is kept in `%APPDATA%\Hihu\banners.json` and
//! put back as soon as the island stops showing that app, the option or the
//! add-on goes off, or Hihu quits (so with Hihu closed, Windows shows its
//! pop-ups as before). A copy that closed without quitting puts them back,
//! or keeps them hidden, at its next start.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use windows::core::{w, HSTRING};
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteKeyValueW, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, RegSetKeyValueW, HKEY, HKEY_CURRENT_USER, KEY_READ, REG_DWORD, RRF_RT_REG_DWORD,
};

const BASE: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Notifications\\Settings\\";

/// The apps whose pop-ups the island hid, with each one's `ShowBanner`
/// before (None: it had none, Windows' default: shown).
pub struct Banners {
    was: BTreeMap<String, Option<u32>>,
    path: PathBuf,
    /// The file could be read (or wasn't there): it may be written. One
    /// that couldn't be read is left alone this run (audit A4), as it may
    /// hold the pop-ups an earlier run hid.
    writable: bool,
}

impl Banners {
    /// What an earlier run hid and didn't put back yet.
    pub fn load() -> Banners {
        let path = crate::paths::data_dir().join("banners.json");
        let (was, outcome) = poni_core::store::load_json::<BTreeMap<String, Option<u32>>>(&path);
        if let Some(m) = outcome.message() {
            crate::pwarn!("notifications: the pop-ups an earlier run hid couldn't be read ({m})");
        }
        Banners { was, path, writable: outcome.writable() }
    }

    /// Hide the pop-ups of exactly the apps in `want`: newly there ones are
    /// hidden (what they had noted first), the rest get theirs back.
    pub fn set(&mut self, want: &BTreeSet<String>) {
        let (hide, back) = poni_core::notifications::banner_plan(&self.was, want);
        if hide.is_empty() && back.is_empty() {
            return;
        }
        for id in back {
            if let Some(before) = self.was.remove(&id) {
                write(&id, before);
            }
        }
        for id in hide {
            let before = read(&id);
            // Already hidden by the person: noted as theirs, left as it is.
            self.was.insert(id.clone(), before);
            if before != Some(0) {
                write(&id, Some(0));
            }
        }
        crate::pinfo!("notifications: Windows' pop-ups off for {} app(s)", self.was.len());
        self.save();
    }

    fn save(&self) {
        if !self.writable {
            return;
        }
        if self.was.is_empty() {
            let _ = std::fs::remove_file(&self.path);
        } else if let Err(e) = poni_core::store::save_json_atomic(&self.path, &self.was) {
            crate::pwarn!("notifications: can't note the pop-ups hidden ({e})");
        }
    }
}

/// Pop-ups that need their own buttons (an alarm's Snooze and Dismiss, a
/// call's Answer), which the island's card hasn't: Windows keeps showing them.
pub fn keeps_its_popup(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    ["windowsalarms", "systemtoast.calling", "yourphone", "phoneexperiencehost"].iter().any(|k| id.contains(k))
}

/// Every app Windows has notification settings for (one key each under
/// `Notifications\Settings`): the apps that ever sent one, so Every app can
/// put their pop-ups away before their next notification, not after it.
/// Ids with a `\` in them (a program path) are nested keys there, left out.
pub fn known_apps() -> Vec<String> {
    let mut out = Vec::new();
    let base = HSTRING::from(BASE.trim_end_matches('\\'));
    let mut key = HKEY::default();
    unsafe {
        if RegOpenKeyExW(HKEY_CURRENT_USER, &base, 0, KEY_READ, &mut key).is_err() {
            return out;
        }
        for i in 0.. {
            let mut name = [0u16; 512];
            let mut len = name.len() as u32;
            if RegEnumKeyExW(key, i, windows::core::PWSTR(name.as_mut_ptr()), &mut len, None, windows::core::PWSTR::null(), None, None).is_err() {
                break;
            }
            let id = String::from_utf16_lossy(&name[..len as usize]);
            if !id.starts_with('{') && !id.is_empty() {
                out.push(id);
            }
        }
        let _ = RegCloseKey(key);
    }
    out
}

/// An app's `ShowBanner` now.
fn read(id: &str) -> Option<u32> {
    let key = HSTRING::from(format!("{BASE}{id}"));
    let mut v = 0u32;
    let mut size = 4u32;
    let r = unsafe { RegGetValueW(HKEY_CURRENT_USER, &key, w!("ShowBanner"), RRF_RT_REG_DWORD, None, Some(&mut v as *mut u32 as *mut _), Some(&mut size)) };
    r.is_ok().then_some(v)
}

/// Set an app's `ShowBanner`, or take it away (None: Windows' default).
fn write(id: &str, v: Option<u32>) {
    let key = HSTRING::from(format!("{BASE}{id}"));
    let r = unsafe {
        match v {
            Some(v) => RegSetKeyValueW(HKEY_CURRENT_USER, &key, w!("ShowBanner"), REG_DWORD.0, Some(&v as *const u32 as *const _), 4),
            None => RegDeleteKeyValueW(HKEY_CURRENT_USER, &key, w!("ShowBanner")),
        }
    };
    if r.is_err() && v.is_some() {
        crate::pwarn!("notifications: can't set an app's pop-ups (0x{:08X})", r.0 as u32);
    }
}
