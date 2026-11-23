//! Windows' own brightness (and volume) pop-up, kept out of sight while the
//! island shows its own (Settings > Pop-ups > Hide Windows' brightness
//! pop-up). A laptop's brightness keys always make Windows show it; there's
//! no setting for that.
//!
//! On Windows 11 the pop-up lives in a window of explorer's that stays
//! around: class `XamlExplorerHostIslandWindow`, no title, a small box at
//! the bottom middle of the screen (the same class with the whole screen is
//! Alt+Tab's, left alone). Minimized, it never shows: the way the
//! HideVolumeOSD tool and Windhawk's "Hide volume and brightness OSD" mod
//! do it. Those look every 250 ms; the island looks only when Windows says
//! the brightness changed, and a few times in the second after.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, GetSystemMetrics, GetWindowRect, GetWindowTextLengthW, IsIconic, ShowWindow, SM_CXSCREEN, SW_MINIMIZE, SW_RESTORE};

/// Windows' pop-up host, if there is one now.
fn host() -> Option<HWND> {
    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let mut after = HWND::default();
    let mut found = None;
    loop {
        let Ok(h) = (unsafe { FindWindowExW(HWND::default(), after, w!("XamlExplorerHostIslandWindow"), PCWSTR::null()) }) else { break };
        if h.is_invalid() {
            break;
        }
        after = h;
        let mut r = RECT::default();
        if unsafe { GetWindowRect(h, &mut r) }.is_err() {
            continue;
        }
        let (w, h_px) = (r.right - r.left, r.bottom - r.top);
        // A small box with no title: the pop-up's, not Alt+Tab's whole screen.
        let small = w > 0 && h_px > 0 && w < screen_w / 2 && h_px < 400;
        if small && unsafe { GetWindowTextLengthW(h) } == 0 {
            found = Some(h);
        }
    }
    found
}

/// Keep Windows' pop-up minimized (once; the app calls again as needed).
pub fn hide() {
    if let Some(h) = host() {
        unsafe {
            if !IsIconic(h).as_bool() {
                let _ = ShowWindow(h, SW_MINIMIZE);
            }
        }
    }
}

/// Give Windows its pop-up back (the option switched off, or Hihu quits).
pub fn restore() {
    if let Some(h) = host() {
        unsafe {
            if IsIconic(h).as_bool() {
                let _ = ShowWindow(h, SW_RESTORE);
            }
        }
    }
}
