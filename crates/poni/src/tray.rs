//! Notification-area icon and its menu (also shown on right-click of the island).

use crate::events::WM_APP_TRAY;
use poni_core::settings::Settings;
use windows::core::{Result, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW};
use windows::Win32::UI::WindowsAndMessaging::*;

pub struct Tray {
    hwnd: HWND,
}

impl Tray {
    pub fn add(hwnd: HWND) -> Result<Tray> {
        unsafe {
            let hinst = GetModuleHandleW(None)?;
            // Ask for the small-icon size so Windows picks hihu.ico's own 16-24 px
            // frames; LoadIconW gives the 32 px one and shrinks it (blurry).
            let (cx, cy) = (GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON));
            let icon = LoadImageW(hinst, PCWSTR(1 as _), IMAGE_ICON, cx, cy, LR_DEFAULTCOLOR)
                .map(|h| HICON(h.0))
                .or_else(|_| LoadIconW(hinst, PCWSTR(1 as _)))
                .or_else(|_| LoadIconW(None, IDI_APPLICATION))?;
            let mut nid = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
                uCallbackMessage: WM_APP_TRAY,
                hIcon: icon,
                ..Default::default()
            };
            let tip: Vec<u16> = format!("{} {}", crate::version::APP_NAME, crate::version::VERSION).encode_utf16().collect();
            nid.szTip[..tip.len()].copy_from_slice(&tip);
            if !Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
                return Err(windows::core::Error::from_win32());
            }
        }
        Ok(Tray { hwnd })
    }

    pub fn remove(self) {
        unsafe {
            let nid = NOTIFYICONDATAW { cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32, hWnd: self.hwnd, uID: 1, ..Default::default() };
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuCmd {
    TogglePinned,
    ToggleAutostart,
    /// "Settings…": the Settings window.
    ShowSettings,
    /// Apple Music lyrics' token (Settings > Music and lyrics).
    AppleMusicToken,
    AppleMusicHelp,
    AppleMusicForget,
    Quit,
}

pub enum MenuItem {
    Item { label: String, cmd: MenuCmd, checked: bool },
    Separator,
}

fn item(label: &str, cmd: MenuCmd, checked: bool) -> MenuItem {
    MenuItem::Item { label: label.to_string(), cmd, checked }
}

/// The tray's menu: short, as everything else is in Settings.
pub fn menu_items(_s: &Settings, pinned: bool) -> Vec<MenuItem> {
    vec![
        item("Settings\u{2026}", MenuCmd::ShowSettings, false),
        MenuItem::Separator,
        item("Keep island open", MenuCmd::TogglePinned, pinned),
        item("Start with Windows", MenuCmd::ToggleAutostart, crate::autostart::is_enabled()),
        MenuItem::Separator,
        item(&format!("Quit {}", crate::version::APP_NAME), MenuCmd::Quit, false),
    ]
}

fn build(menu: HMENU, items: &[MenuItem], cmds: &mut Vec<MenuCmd>) {
    for it in items {
        unsafe {
            match it {
                MenuItem::Item { label, cmd, checked } => {
                    cmds.push(*cmd);
                    let id = cmds.len();
                    let w: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
                    let flags = MF_STRING | if *checked { MF_CHECKED } else { MF_UNCHECKED };
                    let _ = AppendMenuW(menu, flags, id, PCWSTR(w.as_ptr()));
                }
                MenuItem::Separator => {
                    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                }
            }
        }
    }
}

/// Show the menu at `pt` (screen pixels); returns the chosen command.
pub fn show_menu(owner: HWND, pt: POINT, items: &[MenuItem]) -> Option<MenuCmd> {
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        let mut cmds = Vec::new();
        build(menu, items, &mut cmds);
        // Required so the menu closes when clicking elsewhere; afterwards the
        // previous app gets its focus back.
        let prev = GetForegroundWindow();
        let _ = SetForegroundWindow(owner);
        let chosen = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY, pt.x, pt.y, 0, owner, None);
        let _ = PostMessageW(owner, WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        let idx = chosen.0 as usize;
        let cmd = if idx == 0 { None } else { cmds.get(idx - 1).copied() };
        let launches = matches!(
            cmd,
            Some(MenuCmd::ShowSettings | MenuCmd::AppleMusicHelp)
        );
        if !launches && !prev.is_invalid() && prev != owner && IsWindow(prev).as_bool() && GetForegroundWindow() == owner {
            let _ = SetForegroundWindow(prev);
        }
        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_is_short_and_settings_has_the_rest() {
        let cmds: Vec<MenuCmd> = menu_items(&Settings::default(), false)
            .into_iter()
            .filter_map(|i| match i {
                MenuItem::Item { cmd, .. } => Some(cmd),
                MenuItem::Separator => None,
            })
            .collect();
        assert_eq!(cmds, [MenuCmd::ShowSettings, MenuCmd::TogglePinned, MenuCmd::ToggleAutostart, MenuCmd::Quit]);
    }
}
