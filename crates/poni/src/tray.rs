//! Notification-area icon and its menu (also shown on right-click of the island).

use crate::events::WM_APP_TRAY;
use poni_core::settings::{IdleStyle, Settings, Theme};
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
    ToggleIdleStyle(IdleStyle),
    SetTheme(Theme),
    ToggleLyrics,
    TogglePronunciation,
    /// true: hide in full-screen apps; false: stay on top of them.
    SetHideFullscreen(bool),
    ToggleSounds,
    TogglePinned,
    /// "Move island to the centre": back from where it was dragged.
    CentreIsland,
    /// "Lock island position": dragging the closed island doesn't move it.
    ToggleLockPosition,
    ToggleAutostart,
    /// "Advanced settings…": the Settings window.
    ShowSettings,
    OpenSettings,
    ReloadSettings,
    OpenDataFolder,
    /// "About Hihu Island 0.2.3 (…)": opens this version's release notes.
    About,
    /// "Lyrics by LRCLIB": opens lrclib.net.
    LyricsCredit,
    AppleMusicToken,
    AppleMusicHelp,
    AppleMusicForget,
    Quit,
}

pub enum MenuItem {
    Item { label: String, cmd: MenuCmd, checked: bool },
    Sub { label: String, items: Vec<MenuItem> },
    Separator,
}

fn item(label: &str, cmd: MenuCmd, checked: bool) -> MenuItem {
    MenuItem::Item { label: label.to_string(), cmd, checked }
}

/// An idle look as the menu names it.
fn idle_label(s: IdleStyle) -> &'static str {
    match s {
        IdleStyle::Clock => "Clock",
        IdleStyle::Eyes => "Eyes",
        IdleStyle::Minimal => "Minimal",
        IdleStyle::Hidden => "Hidden until hover",
        IdleStyle::Status => "Status bar",
    }
}

pub fn menu_items(s: &Settings, pinned: bool) -> Vec<MenuItem> {
    // The look the theme draws is the one checked: outside Pill, Status shows the clock.
    let shown = crate::pill::status::shown(s.theme, s.idle_style);
    let style = |st: IdleStyle| item(idle_label(st), MenuCmd::ToggleIdleStyle(st), shown == st);
    let theme = |label: &str, t: Theme| item(label, MenuCmd::SetTheme(t), s.theme == t);
    let fullscreen = |label: &str, hide: bool| item(label, MenuCmd::SetHideFullscreen(hide), s.hide_in_fullscreen == hide);
    let mut items = vec![
        item("Advanced settings…", MenuCmd::ShowSettings, false),
        MenuItem::Separator,
        item("Keep island open", MenuCmd::TogglePinned, pinned),
        item("Lock island position", MenuCmd::ToggleLockPosition, s.lock_position),
        MenuItem::Sub {
            label: "Theme".into(),
            items: vec![theme("Classic", Theme::Classic), theme("Glass", Theme::Glass), theme("Pill", Theme::Pill), theme("Diorama", Theme::Diorama)],
        },
        // The status bar is listed only in Pill, as on the Settings page: the
        // other themes would draw the clock for it (Diorama its picture).
        MenuItem::Sub { label: "Idle look".into(), items: crate::pill::status::looks(s.theme).iter().map(|l| style(*l)).collect() },
        MenuItem::Separator,
        item("Synced lyrics", MenuCmd::ToggleLyrics, s.lyrics),
        item("Lyrics pronunciation (romaji, pinyin, Korean)", MenuCmd::TogglePronunciation, s.lyrics_pronunciation),
        apple_music_menu(),
        item("Sounds", MenuCmd::ToggleSounds, s.sounds),
        MenuItem::Sub {
            label: "In full-screen apps".into(),
            items: vec![
                fullscreen("Hide", true),
                fullscreen("Stay on top", false),
            ],
        },
        item("Start with Windows", MenuCmd::ToggleAutostart, crate::autostart::is_enabled()),
        MenuItem::Separator,
        item("Open settings file", MenuCmd::OpenSettings, false),
        item("Reload settings", MenuCmd::ReloadSettings, false),
        item("Open data folder", MenuCmd::OpenDataFolder, false),
        item("Lyrics by LRCLIB", MenuCmd::LyricsCredit, false),
        // Which version and build this is, as the Mac menu says it.
        item(&crate::version::about_label(), MenuCmd::About, false),
        MenuItem::Separator,
        item(&format!("Quit {}", crate::version::APP_NAME), MenuCmd::Quit, false),
    ];
    // Only once the island was dragged off the middle: the way back, under Keep island open.
    if s.island_x.is_some() {
        let at = items.iter().position(|i| matches!(i, MenuItem::Item { cmd: MenuCmd::TogglePinned, .. })).map_or(items.len(), |i| i + 1);
        items.insert(at, item("Move island to the centre", MenuCmd::CentreIsland, false));
    }
    items
}

/// Opt-in lyrics from Apple Music with the user's own token.
fn apple_music_menu() -> MenuItem {
    let on = crate::paths::apple_music_token_path().exists();
    let mut items = vec![
        item("Use token from clipboard", MenuCmd::AppleMusicToken, false),
        item("How to get the token", MenuCmd::AppleMusicHelp, false),
    ];
    if on {
        items.push(MenuItem::Separator);
        items.push(item("Remove token", MenuCmd::AppleMusicForget, false));
    }
    MenuItem::Sub { label: if on { "Apple Music lyrics: on".into() } else { "Apple Music lyrics".into() }, items }
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
                MenuItem::Sub { label, items } => {
                    if let Ok(sub) = CreatePopupMenu() {
                        build(sub, items, cmds);
                        let w: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
                        let _ = AppendMenuW(menu, MF_POPUP, sub.0 as usize, PCWSTR(w.as_ptr()));
                    }
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
            Some(MenuCmd::ShowSettings | MenuCmd::OpenSettings | MenuCmd::OpenDataFolder | MenuCmd::About | MenuCmd::LyricsCredit | MenuCmd::AppleMusicHelp)
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

    fn fullscreen_choices(s: &Settings) -> Vec<(String, MenuCmd, bool)> {
        let items = menu_items(s, false);
        let Some(MenuItem::Sub { items, .. }) = items.into_iter().find(|i| matches!(i, MenuItem::Sub { label, .. } if label == "In full-screen apps")) else {
            panic!("no full-screen submenu");
        };
        items
            .into_iter()
            .filter_map(|i| match i {
                MenuItem::Item { label, cmd, checked } => Some((label, cmd, checked)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_menu_says_which_version_this_is() {
        let items = menu_items(&Settings::default(), false);
        let labels: Vec<(String, MenuCmd)> = items
            .into_iter()
            .filter_map(|i| match i {
                MenuItem::Item { label, cmd, .. } => Some((label, cmd)),
                _ => None,
            })
            .collect();
        let at = |cmd: MenuCmd| labels.iter().position(|l| l.1 == cmd);
        let about = at(MenuCmd::About).expect("no About line");
        assert!(labels[about].0.starts_with(&format!("About Hihu Island {}", env!("CARGO_PKG_VERSION"))), "{}", labels[about].0);
        // Right above Quit, with the lyrics credit kept above it.
        assert_eq!(at(MenuCmd::Quit), Some(about + 1));
        assert_eq!(at(MenuCmd::LyricsCredit), Some(about - 1));
    }

    #[test]
    fn the_way_back_to_the_centre_shows_once_the_island_moved() {
        let cmds = |s: &Settings| -> Vec<MenuCmd> {
            menu_items(s, false)
                .into_iter()
                .filter_map(|i| match i {
                    MenuItem::Item { cmd, .. } => Some(cmd),
                    _ => None,
                })
                .collect()
        };
        assert!(!cmds(&Settings::default()).contains(&MenuCmd::CentreIsland));
        let moved = cmds(&Settings { island_x: Some(0.8), ..Settings::default() });
        let pin = moved.iter().position(|c| *c == MenuCmd::TogglePinned).expect("no Keep island open");
        assert_eq!(moved.get(pin + 1), Some(&MenuCmd::CentreIsland));
    }

    fn idle_choices(s: &Settings) -> Vec<(String, bool)> {
        let items = menu_items(s, false);
        let Some(MenuItem::Sub { items, .. }) = items.into_iter().find(|i| matches!(i, MenuItem::Sub { label, .. } if label == "Idle look")) else {
            panic!("no idle look submenu");
        };
        items
            .into_iter()
            .filter_map(|i| match i {
                MenuItem::Item { label, checked, .. } => Some((label, checked)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn status_bar_is_listed_only_in_the_pill_theme() {
        let pill = Settings { theme: Theme::Pill, idle_style: IdleStyle::Status, ..Settings::default() };
        let labels: Vec<String> = idle_choices(&pill).into_iter().map(|c| c.0).collect();
        assert_eq!(labels, ["Clock", "Eyes", "Minimal", "Hidden until hover", "Status bar"]);
        assert_eq!(idle_choices(&pill).last(), Some(&("Status bar".to_string(), true)));
        // Another theme draws the clock for the status bar: Clock is checked,
        // and Status comes back with Pill.
        for theme in [Theme::Classic, Theme::Glass, Theme::Diorama] {
            let s = Settings { theme, ..pill.clone() };
            let choices = idle_choices(&s);
            assert_eq!(choices.len(), if theme == Theme::Diorama { 3 } else { 4 }, "{theme:?}");
            assert_eq!(choices[0], ("Clock".to_string(), true), "{theme:?}");
            assert!(choices.iter().skip(1).all(|c| !c.1), "{theme:?}");
            assert!(choices.iter().all(|c| c.0 != "Status bar"), "{theme:?}");
        }
        // Diorama's looks by name: its picture with the time, the picture
        // alone, and nothing until the pointer comes. It has no eyes: a
        // setting left on Eyes shows the picture alone, so Minimal is checked.
        let diorama = Settings { theme: Theme::Diorama, idle_style: IdleStyle::Minimal, ..Settings::default() };
        let looks = [("Clock".to_string(), false), ("Minimal".to_string(), true), ("Hidden until hover".to_string(), false)];
        assert_eq!(idle_choices(&diorama), looks);
        assert_eq!(idle_choices(&Settings { idle_style: IdleStyle::Eyes, ..diorama }), looks);
    }

    /// Where a theme comes in the menu. No `_` arm: a theme added to the
    /// enum stops this compiling until it has a place, and then the test
    /// below asks the menu for it.
    fn place(t: Theme) -> usize {
        match t {
            Theme::Classic => 0,
            Theme::Glass => 1,
            Theme::Pill => 2,
            Theme::Diorama => 3,
        }
    }

    fn theme_choices(s: &Settings) -> Vec<(String, MenuCmd, bool)> {
        let items = menu_items(s, false);
        let Some(MenuItem::Sub { items, .. }) = items.into_iter().find(|i| matches!(i, MenuItem::Sub { label, .. } if label == "Theme")) else {
            panic!("no theme submenu");
        };
        items
            .into_iter()
            .filter_map(|i| match i {
                MenuItem::Item { label, cmd, checked } => Some((label, cmd, checked)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_theme_submenu_has_every_theme_and_checks_the_one_in_use() {
        let names = ["Classic", "Glass", "Pill", "Diorama"];
        let choices = theme_choices(&Settings::default());
        assert_eq!(choices.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), names);
        for (i, (label, cmd, _)) in choices.iter().enumerate() {
            let MenuCmd::SetTheme(theme) = *cmd else { panic!("{label} doesn't set a theme") };
            assert_eq!(place(theme), i, "{label}");
            // Picked, it is the one checked.
            let checked: Vec<bool> = theme_choices(&Settings { theme, ..Settings::default() }).into_iter().map(|c| c.2).collect();
            assert_eq!(checked, (0..names.len()).map(|j| j == i).collect::<Vec<_>>(), "{label}");
        }
    }

    #[test]
    fn start_with_windows_follows_the_full_screen_submenu() {
        // Over maximized windows (Step aside) is gone: nothing sits between them.
        let items = menu_items(&Settings::default(), false);
        let fs = items.iter().position(|i| matches!(i, MenuItem::Sub { label, .. } if label == "In full-screen apps")).expect("full-screen submenu");
        assert!(matches!(items.get(fs + 1), Some(MenuItem::Item { cmd: MenuCmd::ToggleAutostart, .. })));
    }

    #[test]
    fn full_screen_submenu_checks_the_current_choice() {
        let mut s = Settings::default();
        for hide in [true, false] {
            s.hide_in_fullscreen = hide;
            let choices = fullscreen_choices(&s);
            assert_eq!(
                choices,
                vec![
                    ("Hide".to_string(), MenuCmd::SetHideFullscreen(true), hide),
                    ("Stay on top".to_string(), MenuCmd::SetHideFullscreen(false), !hide),
                ]
            );
        }
    }
}
