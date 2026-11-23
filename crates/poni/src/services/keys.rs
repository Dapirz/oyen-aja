//! Keys the island's pop-ups answer (Settings > Pop-ups): the volume keys,
//! taken over so Windows' own volume flyout stays away and the island's
//! shows instead; Caps Lock, Num Lock and Scroll Lock; and the keys that
//! switch the keyboard's language (Windows + Space, Alt + Shift,
//! Ctrl + Shift).
//!
//! One low-level keyboard hook (`WH_KEYBOARD_LL`) on the UI thread, there
//! only while one of those is on. Windows calls it for each key press; it
//! looks at the key, posts `WM_APP_KEY` to the island for the few it cares
//! about, and lets every other key through at once. The volume keys alone
//! it keeps (and only while the island isn't hidden over a full-screen
//! app: then Windows shows its own flyout, as nothing of the island's would
//! show).

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, VIRTUAL_KEY, VK_CAPITAL, VK_CONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_NUMLOCK, VK_RSHIFT, VK_RWIN, VK_SCROLL, VK_SHIFT,
    VK_SPACE, VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, PostMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, WH_KEYBOARD_LL, WM_APP, WM_KEYDOWN, WM_SYSKEYDOWN,
};

/// To the island: `wParam` a volume key, a lock key (`lParam` 1 = now on),
/// or [`LANGUAGE`].
pub const WM_APP_KEY: u32 = WM_APP + 9;
/// `wParam` for a key that may have switched the keyboard's language.
pub const LANGUAGE: usize = 0xFFFF;

static HOOK: AtomicIsize = AtomicIsize::new(0);
static TO: AtomicIsize = AtomicIsize::new(0);
static VOLUME: AtomicBool = AtomicBool::new(false);
static LOCKS: AtomicBool = AtomicBool::new(false);
static LANG: AtomicBool = AtomicBool::new(false);
/// The island is hidden over a full-screen app: the volume keys go to Windows.
static PASS: AtomicBool = AtomicBool::new(false);
/// Lock keys held down (bit per key), so a held key's repeats say nothing.
static HELD: AtomicU32 = AtomicU32::new(0);

/// What the hook answers to. It's there while any is on, gone otherwise.
pub fn set(hwnd: HWND, volume: bool, locks: bool, language: bool) {
    TO.store(hwnd.0 as isize, Ordering::SeqCst);
    VOLUME.store(volume, Ordering::SeqCst);
    LOCKS.store(locks, Ordering::SeqCst);
    LANG.store(language, Ordering::SeqCst);
    let want = volume || locks || language;
    let have = HOOK.load(Ordering::SeqCst) != 0;
    if want && !have {
        match unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), None, 0) } {
            Ok(h) => {
                HOOK.store(h.0 as isize, Ordering::SeqCst);
                crate::pinfo!("keys: listening for the pop-ups' keys");
            }
            Err(e) => crate::pwarn!("keys: no keyboard hook ({e})"),
        }
    } else if !want && have {
        let h = HOOK.swap(0, Ordering::SeqCst);
        unsafe {
            let _ = UnhookWindowsHookEx(HHOOK(h as *mut _));
        }
        crate::pinfo!("keys: stopped listening");
    }
}

/// The island hid over a full-screen app (true), or came back.
pub fn set_passing(pass: bool) {
    PASS.store(pass, Ordering::SeqCst);
}

fn bit(vk: VIRTUAL_KEY) -> u32 {
    match vk {
        VK_CAPITAL => 1,
        VK_NUMLOCK => 2,
        _ => 4,
    }
}

fn held(vk: VIRTUAL_KEY) -> bool {
    unsafe { GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000 != 0 }
}

unsafe extern "system" fn hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && lp.0 != 0 {
        let k = &*(lp.0 as *const KBDLLHOOKSTRUCT);
        let vk = VIRTUAL_KEY(k.vkCode as u16);
        let down = wp.0 as u32 == WM_KEYDOWN || wp.0 as u32 == WM_SYSKEYDOWN;
        let to = HWND(TO.load(Ordering::Relaxed) as *mut _);
        match vk {
            VK_VOLUME_UP | VK_VOLUME_DOWN | VK_VOLUME_MUTE if VOLUME.load(Ordering::Relaxed) && !PASS.load(Ordering::Relaxed) => {
                if down {
                    let _ = PostMessageW(to, WM_APP_KEY, WPARAM(vk.0 as usize), LPARAM(0));
                }
                // Kept: Windows' flyout doesn't come.
                return LRESULT(1);
            }
            VK_CAPITAL | VK_NUMLOCK | VK_SCROLL if LOCKS.load(Ordering::Relaxed) => {
                let b = bit(vk);
                if down {
                    if HELD.fetch_or(b, Ordering::Relaxed) & b == 0 {
                        // The state before this press: it flips with it.
                        let on = GetKeyState(vk.0 as i32) & 1 == 0;
                        let _ = PostMessageW(to, WM_APP_KEY, WPARAM(vk.0 as usize), LPARAM(on as isize));
                    }
                } else {
                    HELD.fetch_and(!b, Ordering::Relaxed);
                }
            }
            // Let go of the key that ends a language switch.
            VK_SPACE if !down && LANG.load(Ordering::Relaxed) && (held(VK_LWIN) || held(VK_RWIN)) => {
                let _ = PostMessageW(to, WM_APP_KEY, WPARAM(LANGUAGE), LPARAM(0));
            }
            VK_SHIFT | VK_LSHIFT | VK_RSHIFT if !down && LANG.load(Ordering::Relaxed) && (held(VK_MENU) || held(VK_LMENU) || held(VK_CONTROL)) => {
                let _ = PostMessageW(to, WM_APP_KEY, WPARAM(LANGUAGE), LPARAM(0));
            }
            _ => {}
        }
    }
    CallNextHookEx(None, code, wp, lp)
}

/// The keyboard language of the window in front, as Windows names it
/// ("English (United States)"), and its layout handle to tell a change.
pub fn language() -> Option<(isize, String)> {
    use windows::Win32::Globalization::{GetLocaleInfoEx, LCIDToLocaleName, LOCALE_SLOCALIZEDDISPLAYNAME};
    use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayout;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    unsafe {
        let thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let hkl = GetKeyboardLayout(thread);
        let lang = (hkl.0 as usize & 0xFFFF) as u32;
        let mut name = [0u16; 86];
        if LCIDToLocaleName(lang, Some(&mut name), 0) == 0 {
            return None;
        }
        let mut out = [0u16; 128];
        let n = GetLocaleInfoEx(windows::core::PCWSTR(name.as_ptr()), LOCALE_SLOCALIZEDDISPLAYNAME, Some(&mut out));
        if n <= 1 {
            return None;
        }
        Some((hkl.0 as isize, String::from_utf16_lossy(&out[..n as usize - 1])))
    }
}
