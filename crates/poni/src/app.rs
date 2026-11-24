//! The app: owns the island window, state, renderer and services; routes
//! window messages, service events and page commands.

use crate::activities::{self, Mode};
use crate::clock;
use crate::commands::{Command, MediaCmd, NotifyCmd, PairCmd, PhoneCmd};
use crate::events::{AppEvent, AppleMusicStatus, EventSink, SendHwnd, WM_APP_APPBAR, WM_APP_EVENT, WM_APP_FRAME, WM_APP_TRAY};
use crate::gfx::{px_rect, px_union, scale_about, union_rect, Canvas, Gfx, HitMask};
use crate::icons::Icon;
use crate::island::{Geom, Island, SHOULDER};
use crate::pages::{self, PageKind};
use crate::paths;
use crate::pill;
use crate::services::notifications::NotifyEvent;
use crate::services::{self, Services};
use crate::settings_host::{self, SettingsWindow};
use crate::settings_window::Section;
use crate::state::{
    AppState, HudKind, InlineAction, Locate, LyricsState, LyricsStatus, Notice, NoticeAction, NotifyAccess, PairedPhone, PhoneAsk,
    PhoneDrop, PhoneTransfer, PhoneView, PrayerState,
};
use crate::tray::{self, Tray};
use crate::ui::{wid, CursorKind, Hit, Input, KeyInput, MouseButton, Ui};
use crate::window::{self, FrameClock, FrameTick, MonitorInfo};
use crate::{dragdrop, selftest, shell};
use poni_core::spot;
use poni_core::addons::Status as AddonStatus;
use poni_core::frametime::{ambient_divisor, MotionStats};
use poni_core::geom::{palette, Color, Point, Rect};
use poni_core::pairdrop::{MeetEvent, PeerEvent};
use poni_core::phonedrop::{Event as PhoneEvent, QrMatrix};
use poni_core::prayer::{Due, PlaceList, Prayer, PrayerMoment};
use poni_core::settings::{IdleStyle, PrayerSound, Settings, Theme};
use poni_core::settings::MonitorChoice;
use poni_core::store::{load_json, save_json_atomic, LoadOutcome, Store};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use windows::core::{w, Result};
use windows::Foundation::Numerics::Matrix3x2;
use windows::Win32::Foundation::*;
use windows::Win32::System::Ole::{OleInitialize, OleUninitialize};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Not exported by windows-rs 0.58's WindowsAndMessaging module.
const WM_MOUSELEAVE: u32 = 0x02A3;

const TIMER_HOVER: usize = 1;
const TIMER_COLLAPSE: usize = 2;
const TIMER_WAKE: usize = 3;
const TIMER_SAVE: usize = 4;
const TIMER_MINUTE: usize = 6;
pub const TIMER_SELFTEST: usize = 7;
const TIMER_TRAY: usize = 8;
const TIMER_RAISE: usize = 9;
/// Make way: while the island is out of the pointer's way, how often to
/// look whether the pointer left (the window no longer takes the mouse
/// there, so no message says so). Runs only while it makes way.
const TIMER_MAKE_WAY: usize = 10;
const MAKE_WAY_POLL_MS: u32 = 100;
/// Screen time's look once a minute (`screentime::CHECK_MS`): idle, the day,
/// a cut so the page and the file keep up. Only while it's on and someone
/// is there (not locked, screen on).
const TIMER_SCREEN: usize = 11;
/// A moment after a key that switches the keyboard's language: Windows has
/// switched it by then, and the pop-up says which. One-shot.
const TIMER_LANGUAGE: usize = 12;
const LANGUAGE_WAIT_MS: u32 = 300;
/// Looks after the keys before giving up (Windows can take a moment).
const LANGUAGE_LOOKS: u8 = 3;
/// After the brightness changed, Windows' own pop-up is put away again this
/// often, this many times (it may come a moment after Windows' word).
const TIMER_OSD: usize = 13;
const OSD_EVERY_MS: u32 = 120;
const OSD_LOOKS: u8 = 8;
/// The next event reminder (Today's calendar): one-shot, set for when it's
/// due (at most 6 hours ahead, then set again), only while there's one.
const TIMER_AGENDA: usize = 14;
const AGENDA_MAX_WAIT_MS: i64 = 6 * 3600 * 1000;
/// Make way: the pointer this close to the top edge (DIPs, the strip the
/// island steps aside to) opens the island as usual instead.
const MAKE_WAY_EDGE: f32 = 5.0;
/// Make way: the island comes back once the pointer is this far (DIPs)
/// beyond where it rests.
const MAKE_WAY_MARGIN: f32 = 12.0;
/// Alt+Shift+S pauses and resumes the Script page's reading from any app.
const HOTKEY_SCRIPT: i32 = 2;
/// Raises that follow a new foreground window, and their spacing.
const RAISE_REPEATS: u8 = 4;
const RAISE_INTERVAL_MS: u32 = 500;

const WM_CAPTURECHANGED_: u32 = 0x0215;
const WM_WTSSESSION_CHANGE_: u32 = 0x02B1;
const WTS_SESSION_LOCK_: usize = 7;
const WTS_SESSION_UNLOCK_: usize = 8;

/// Steps for copying the Apple Music token (README).
const APPLE_MUSIC_HELP_URL: &str = "https://github.com/Dapirz/oyen-aja#apple-music-lyrics";
/// Phone drop closes itself after this long without a request from a phone.
const PHONE_IDLE_S: f64 = 600.0;

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    /// Posted messages that arrived while the app was borrowed (a nested
    /// message loop ran inside a handler). Replayed when the borrow ends.
    static MISSED: RefCell<Vec<(u32, WPARAM, LPARAM)>> = const { RefCell::new(Vec::new()) };
    /// A WM_APP_FRAME was swallowed: re-arm the frame clock.
    static FRAME_MISSED: Cell<bool> = const { Cell::new(false) };
}

/// Modal work that must run without the App borrowed (it pumps messages).
pub enum Deferred {
    DragOut(Vec<String>),
    Menu(POINT),
}

pub fn main_entry() -> i32 {
    let args: Vec<String> = std::env::args().collect();
    let selftest_dir = args.iter().position(|a| a == "--selftest").map(|i| args.get(i + 1).cloned().unwrap_or_else(|| "selftest-out".into()));
    if let Some(dir) = &selftest_dir {
        let home = std::path::Path::new(dir).join("home");
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("PONI_HOME", &home);
    }
    crate::plog::init(paths::log_path());
    pinfo!("Hihu {} starting (args: {:?})", env!("CARGO_PKG_VERSION"), &args[1..]);
    std::panic::set_hook(Box::new(|info| {
        perror!("panic: {info}");
    }));

    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    // Single instance (the self-test may run next to a normal instance).
    let mutex_name = if selftest_dir.is_some() { w!("Local\\Hihu.Island.SelfTest") } else { w!("Local\\Hihu.Island") };
    let _mutex = unsafe { CreateMutexW(None, true, mutex_name) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        pinfo!("another instance is running; exiting");
        return 0;
    }
    if let Err(e) = unsafe { OleInitialize(None) } {
        perror!("OleInitialize failed: {e}");
        return 2;
    }
    if crate::autostart::store_packaged() {
        pinfo!("running as the Microsoft Store package");
    } else if crate::autostart::packaged() {
        pinfo!("running with the identity package (notifications can be read)");
    }
    let code = match run(selftest_dir) {
        Ok(code) => code,
        Err(e) => {
            perror!("fatal: {e}");
            3
        }
    };
    unsafe { OleUninitialize() };
    pinfo!("Hihu exiting with code {code}");
    code
}

fn run(selftest_dir: Option<String>) -> Result<i32> {
    window::register_classes(island_wndproc, helper_wndproc)?;
    if let Err(e) = settings_host::register(settings_wndproc) {
        pwarn!("settings window: class not registered ({e})");
    }
    let hwnd = window::create_island_window()?;
    let helper = window::create_helper_window()?;
    pinfo!("windows created");

    let (raw_settings, s_out): (Settings, _) = load_json(&paths::settings_path());
    if let Some(m) = s_out.message() {
        pwarn!("settings: {m}");
    }
    let settings = raw_settings.clone().sanitized();
    // Write new files, and write migrated / clamped values back so a
    // migration (e.g. the v2 hover delay) happens once and sticks.
    if s_out == LoadOutcome::Missing || (settings != raw_settings && s_out.writable()) {
        if let Err(e) = save_json_atomic(&paths::settings_path(), &settings) {
            pwarn!("saving settings failed: {e}");
        } else if s_out != LoadOutcome::Missing {
            pinfo!("settings: wrote back migrated / out-of-range values (v{})", settings.settings_version);
        }
    }
    let (store, d_out): (Store, _) = load_json(&paths::store_path());
    if let Some(m) = d_out.message() {
        pwarn!("data: {m}");
    }

    let (tx, rx) = mpsc::channel();
    let sink = EventSink::new(tx, SendHwnd::new(hwnd));
    let app = App::new(hwnd, helper, sink, rx, settings, store, selftest_dir, s_out.writable(), d_out.writable())?;
    APP.with(|c| *c.borrow_mut() = Some(app));
    with_app(|app| app.start());
    pinfo!("started; entering message loop");

    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    let code = APP.with(|c| c.borrow_mut().take().map(|mut a| a.shutdown()).unwrap_or(0));
    Ok(code)
}

/// Run `f` with the app if it isn't already borrowed (re-entrancy safe).
pub fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    let (r, hwnd) = APP.with(|c| match c.try_borrow_mut() {
        Ok(mut g) => match g.as_mut() {
            Some(app) => {
                let r = f(app);
                if FRAME_MISSED.with(|m| m.replace(false)) {
                    app.clock.frame_handled();
                    app.request_frame();
                }
                (Some(r), Some(app.hwnd))
            }
            None => (None, None),
        },
        Err(_) => (None, None),
    });
    // Re-post messages a nested loop swallowed while we were borrowed.
    if let Some(hwnd) = hwnd {
        let missed = MISSED.with(|m| std::mem::take(&mut *m.borrow_mut()));
        for (msg, wp, lp) in missed {
            unsafe {
                let _ = PostMessageW(hwnd, msg, wp, lp);
            }
        }
    }
    r
}

/// Remember a message that couldn't be handled because the app was borrowed.
fn note_missed(msg: u32, wp: WPARAM, lp: LPARAM) -> bool {
    match msg {
        WM_APP_FRAME => {
            FRAME_MISSED.with(|f| f.set(true));
            true
        }
        WM_APP_EVENT | WM_MOUSELEAVE | WM_TIMER | WM_LBUTTONUP | WM_APP_TRAY | WM_APP_APPBAR => {
            MISSED.with(|m| {
                let mut v = m.borrow_mut();
                if v.len() < 64 && !(msg != WM_TIMER && v.iter().any(|x| x.0 == msg && x.1 == wp)) {
                    v.push((msg, wp, lp));
                }
            });
            true
        }
        _ => false,
    }
}

unsafe extern "system" fn island_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let mut deferred = Vec::new();
    let res = with_app(|app| app.handle_island(msg, wp, lp, &mut deferred));
    for d in deferred {
        run_deferred(d);
    }
    match res {
        Some(Some(r)) => r,
        Some(None) => DefWindowProcW(hwnd, msg, wp, lp),
        None if note_missed(msg, wp, lp) => LRESULT(0),
        None => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn helper_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let mut deferred = Vec::new();
    let res = with_app(|app| app.handle_helper(msg, wp, lp, &mut deferred));
    for d in deferred {
        run_deferred(d);
    }
    match res {
        Some(Some(r)) => r,
        Some(None) => DefWindowProcW(hwnd, msg, wp, lp),
        // Helper messages are re-posted to the island window, which forwards
        // tray/appbar ids back to the helper handler.
        None if matches!(msg, WM_APP_TRAY | WM_APP_APPBAR) => {
            let _ = note_missed(msg, wp, lp);
            LRESULT(0)
        }
        None => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// The Settings window's messages (`settings_window`).
unsafe extern "system" fn settings_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let mut deferred = Vec::new();
    let res = with_app(|app| app.handle_settings_window(hwnd, msg, wp, lp, &mut deferred));
    for d in deferred {
        run_deferred(d);
    }
    match res {
        Some(Some(r)) => r,
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn run_deferred(d: Deferred) {
    match d {
        Deferred::DragOut(paths) => {
            with_app(|app| app.release_mouse());
            if let Err(e) = dragdrop::drag_out(&paths) {
                pwarn!("drag out failed: {e}");
            }
            with_app(|app| app.request_frame());
        }
        Deferred::Menu(pt) => {
            let Some((helper, items)) = with_app(|app| {
                // Right-clicked on the open island (the tray's menu leaves it as it is).
                app.menu_open = app.expanded && app.hovering;
                (app.helper, tray::menu_items(&app.st.settings, app.st.pinned))
            }) else {
                return;
            };
            let cmd = tray::show_menu(helper, pt, &items);
            with_app(|app| {
                app.menu_closed();
                if let Some(cmd) = cmd {
                    app.on_menu(cmd);
                }
            });
        }
    }
}

pub struct App {
    hwnd: HWND,
    helper: HWND,
    gfx: Gfx,
    pub st: AppState,
    island: Island,
    mode: Mode,
    input: Input,
    hits: Vec<Hit>,
    hovering: bool,
    expanded: bool,
    drag_hover: bool,
    text_focus: bool,
    prev_foreground: HWND,
    monitor: MonitorInfo,
    /// The Settings window, while it is open.
    settings_win: Option<SettingsWindow>,
    clock: FrameClock,
    sink: EventSink,
    rx: Receiver<AppEvent>,
    services: Option<Services>,
    /// The Glass theme's screen copy, light/dark choice and glass.
    glass: crate::glass::Glass,
    /// Prayer times' adzan, while it plays.
    adzan: services::adzan::AdzanPlayer,
    /// The glass was still changing on the last frame (turning light or dark).
    glass_moving: bool,
    /// Animation time of the last frame (the vblank it was shown on).
    last_frame_t: f64,
    /// The window: a fixed-size surface the island is drawn in, centred on
    /// the monitor unless the island was dragged along the top edge.
    surface: Surface,
    /// The island's centre in the window, DIPs (where it hangs from): where
    /// it rests (`home_cx`), unless its outline needs more room there.
    cx: f32,
    /// Where the island's centre rests in the window, DIPs: the window's
    /// middle, or off it once dragged towards an edge (`place_surface`).
    home_cx: f32,
    /// Canvas pixels drawn last frame (the next frame clears them).
    prev_canvas_px: Option<RECT>,
    /// GPU renderer: the hit mask last pushed to the window, and when.
    mask: Option<(HitMask, f64)>,
    /// Smoothness of the morph in progress (logged when it settles).
    motion: Option<MotionStats>,
    motion_name: &'static str,
    /// Motions logged so far, and when a hitchy one was last logged.
    motions_logged: u32,
    hitch_logged_at: f64,
    /// Per-frame timing of the first few morphs (for the log): vblanks since
    /// the last frame, then ms from vblank to drawing, to draw, to present
    /// and to update the hit mask.
    trace: Vec<(u32, [f32; 4])>,
    /// DWM's counters when the morph began (refreshes, frames composed,
    /// frames missed), to tell our dropped frames from the compositor's.
    dwm_at_start: Option<(u64, u64, u64)>,
    /// Log lines written off the frame path (file writes can take tens of ms).
    log_queue: Vec<String>,
    /// Self-test: read the next frame back, and where it was.
    capture_next: bool,
    capture_rect: RECT,
    fullscreen_hidden: bool,
    /// The last full-screen state reported, hidden or not.
    fullscreen_on: bool,
    /// Step aside: the app windows at the top edge of the island's monitor
    /// (screen pixels, cut to the band along it), as last reported.
    top_windows: Vec<[i32; 4]>,
    /// Make way for the pointer: the island stepped aside because the
    /// pointer came onto it (`Settings::make_way`), until it leaves.
    making_way: bool,
    /// The Clipboard add-on's listener (its own thread; None while it's off).
    clip_listener: Option<services::clipboard::ClipboardHistory>,
    /// Text we put on the clipboard ourselves, not to record it again.
    own_clip: Option<String>,
    /// The history's change counter when it was last saved.
    clip_saved: u64,
    /// Screen time's tracker (None while it's off), and whether its days
    /// changed since they were saved.
    screen: Option<crate::screentime::Tracker>,
    /// The Notes page's meeting while it is recorded (only then).
    notes_rec: Option<services::notes::Recording>,
    /// The meeting's audio once stopped, kept until its notes are saved (or
    /// it is thrown away), for Try again.
    notes_take: Option<services::notes::Take>,
    /// When the meeting being taken down began (local time, for its notes).
    notes_started: poni_core::datetime::LocalDateTime,
    screen_dirty: bool,
    /// Show in screenshots off: what Windows was last asked (None: not yet).
    capture_hidden: Option<bool>,
    /// Follow-up raises still due (TIMER_RAISE).
    raises_left: u8,
    /// Window classes already logged for covering the island (once each).
    covered_by: HashSet<String>,
    shown: bool,
    /// Alt+Shift+S is registered (only while the Script page can read).
    script_hotkey: bool,
    /// The Script page has asked for the island to stay out of screen capture.
    script_excluded: bool,
    /// A reading held the island open at the last tick.
    script_held: bool,
    /// The Script page was on screen at the last tick.
    script_shown: bool,
    tray: Option<Tray>,
    taskbar_created_msg: u32,
    selftest: Option<selftest::SelfTest>,
    exit_code: i32,
    /// Settings as last written to disk (to diff page-made changes).
    saved_settings: Settings,
    /// False when the file existed but couldn't be read: never overwrite it.
    settings_writable: bool,
    store_writable: bool,
    /// The current press started as a double-click (its click is ignored).
    dbl_press: bool,
    /// Pending high surrogate from WM_CHAR.
    high_surrogate: Option<u16>,
    /// Unconsumed wheel notches accumulated for page switching.
    wheel_acc: f32,
    wheel_t: f64,
    page_switch_t: f64,
    /// Session locked / display off: pause ambient animation.
    locked: bool,
    display_off: bool,
    tray_retries: u32,
    /// Content of the previous mode, fading out while the outline morphs:
    /// (mode, start time, its geometry, the opacity it fades from).
    outgoing: Option<(Mode, f64, Geom, f32)>,
    /// A page switch in progress: (from page, start time, slide direction).
    page_anim: Option<(PageKind, f64, f32)>,
    /// Where the hover (dwell) timer was last armed; None when it isn't.
    hover_arm: Option<Point>,
    /// The pointer left the open island: close it on the next frame.
    collapse_due: bool,
    /// When the pointer last closed the island by leaving it.
    left_at: f64,
    /// The right-click menu is open over the open island. The pointer on
    /// the menu has left the island as far as Windows is concerned, so the
    /// menu holds the island open until it closes.
    menu_open: bool,
    /// Phone drop's web server while the Shelf's phone panel is open.
    phone: Option<services::phonedrop::PhoneDrop>,
    /// The Shelf paths last handed to it (to notice changes).
    phone_offer: Vec<String>,
    /// The Shelf paths last handed to the paired-phones service.
    pair_offer: Option<Vec<String>>,
    /// Whether the island floated when the window was last laid out (its
    /// room differs: see `sync_room`).
    laid_floating: bool,
    /// The spot along the top edge (`Settings::island_x`) the window was
    /// last laid out for (see `sync_spot`).
    laid_spot: Option<f32>,
    /// A press on the closed island, which a sideways move turns into a drag.
    grab: Option<Grab>,
    /// Notifications' listener while it is on; None while it's
    /// off, in the download (no package identity) and in the self-test.
    notify: Option<services::notifications::Listener>,
    /// Windows' own pop-ups hidden for the apps the island shows.
    banners: services::banners::Banners,
    /// The keyboard layout last shown (or found), and the screen's
    /// brightness Windows last reported: a pop-up only for a change.
    keyboard_layout: Option<isize>,
    brightness: Option<u8>,
    /// The microphone's mute as last reported (None: not yet).
    mic_muted: Option<bool>,
    /// Who uses the camera, watched while its pop-up is on.
    camera: Option<services::camera::Camera>,
    /// Looks left for a language switch after its keys.
    language_looks: u8,
    /// Looks left to put Windows' brightness pop-up away.
    osd_looks: u8,
    /// The minute (local, from 1970) up to which event reminders were shown.
    agenda_done: i64,
}

/// The island window: position and size in screen pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Surface {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

/// A press on the closed island: let go where it was, it opens the
/// island; moved sideways first, it drags the island along the top edge.
#[derive(Clone, Copy, Debug)]
struct Grab {
    /// Where the pointer was pressed, screen pixels across.
    from_px: i32,
    /// The island's centre then, DIPs from the monitor's left edge.
    centre: f32,
    /// The pointer went past `spot::DRAG_SLOP`: a drag, not a click.
    moved: bool,
}

/// The grow springs overshoot by about 3 %; the surface leaves 5 %.
const OVERSHOOT: f32 = 1.05;
/// Hit-mask updates while the island moves (the mask is invisible, so it
/// may trail the outline by a frame or two).
const MASK_INTERVAL: f64 = 1.0 / 30.0;

/// How long outgoing content takes to fade (seconds).
const OUTGOING_FADE: f64 = 0.14;
/// Page switch cross-fade duration (seconds).
const PAGE_SWITCH: f64 = 0.26;
/// Pointer travel (DIPs) that restarts the hover dwell: the island opens
/// only once the pointer rests on it, not when it brushes past.
const DWELL_SLOP: f32 = 4.0;
/// The pointer coming back onto the island this soon after leaving it
/// (while it is still shrinking) opens it again at once, from where the
/// outline is: overshooting an edge costs no close-and-reopen.
const REOPEN_S: f64 = 0.3;
/// A hold that kept the island open (typing, a file drag, Phone drop, the
/// pin) ended while the pointer is elsewhere: close after this grace, so
/// the change can be seen.
const HOLD_GRACE_MS: u32 = 380;
/// Collapsed pill growth (w, h, radius) while the pointer is on it.
const HOVER_LIFT: (f32, f32, f32) = (12.0, 4.0, 2.0);
/// Step aside: an app window is at the top when its top edge is within this
/// many DIPs of the edge the island hangs from (a maximized window overhangs
/// it, a snapped one starts on it). One placed lower keeps its title and
/// tabs below the island, which covers a sliver of its frame at most.
const TOP_BAND: f32 = 8.0;
/// How long the inline notice pill shows its check mark (seconds).
const INLINE_DONE_S: f64 = 0.6;
/// Incoming content starts at this scale and this many DIPs up, and
/// settles down from the bezel as it fades in.
const CONTENT_SCALE_FROM: f32 = 0.94;
const CONTENT_DROP: f32 = 6.0;
/// Smallest scale the outgoing content shrinks to as the outline narrows.
const GHOST_MIN_SCALE: f32 = 0.6;

/// How much of the outgoing content still shows `age` seconds into its fade
/// (a smoothstep: it holds, then leaves).
fn outgoing_left(age: f64) -> f32 {
    let t = ((age / OUTGOING_FADE) as f32).clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

impl App {
    #[allow(clippy::too_many_arguments)]
    fn new(
        hwnd: HWND,
        helper: HWND,
        sink: EventSink,
        rx: Receiver<AppEvent>,
        settings: Settings,
        store: Store,
        selftest_dir: Option<String>,
        settings_writable: bool,
        store_writable: bool,
    ) -> Result<Self> {
        let monitor = window::pick_monitor(settings.monitor);
        let gfx = Gfx::new(monitor.dpi as f32, hwnd)?;
        let mut st = AppState::new(settings, store);
        st.now = clock::mono();
        st.local = clock::local_now();
        st.unix = clock::unix_now();
        st.accent = theme_accent(st.settings.theme);
        // Notifications only where Windows lets this copy read them (the Store copy).
        st.notify_can = crate::autostart::packaged();
        let mode = Mode::Idle;
        let mut island = Island::new(activities::geom(&mode, &st));
        island.set_floating(pill::on(st.settings.theme).then_some(pill::TOP_GAP));
        let clock = FrameClock::start(SendHwnd::new(hwnd), monitor.handle);
        let taskbar_created_msg = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
        // The Downloader: not in the Microsoft Store build.
        let selftest = selftest_dir.map(selftest::SelfTest::new);
        let saved_settings = st.settings.clone();
        Ok(Self {
            hwnd,
            helper,
            gfx,
            st,
            island,
            mode,
            input: Input::default(),
            hits: Vec::new(),
            hovering: false,
            expanded: false,
            drag_hover: false,
            text_focus: false,
            prev_foreground: HWND::default(),
            monitor,
            settings_win: None,
            clock,
            sink,
            rx,
            services: None,
            glass: crate::glass::Glass::new(services::apps_use_light_theme()),
            adzan: Default::default(),
            glass_moving: false,
            last_frame_t: 0.0,
            surface: Surface::default(),
            cx: 0.0,
            home_cx: 0.0,
            prev_canvas_px: None,
            mask: None,
            motion: None,
            motion_name: "morph",
            motions_logged: 0,
            hitch_logged_at: f64::NEG_INFINITY,
            trace: Vec::new(),
            dwm_at_start: None,
            log_queue: Vec::new(),
            capture_next: false,
            capture_rect: RECT::default(),
            fullscreen_hidden: false,
            fullscreen_on: false,
            top_windows: Vec::new(),
            making_way: false,
            clip_listener: None,
            own_clip: None,
            clip_saved: 0,
            screen: None,
            notes_rec: None,
            notes_take: None,
            notes_started: Default::default(),
            screen_dirty: false,
            capture_hidden: None,
            raises_left: 0,
            covered_by: HashSet::new(),
            shown: false,
            script_hotkey: false,
            script_excluded: false,
            script_held: false,
            script_shown: false,
            tray: None,
            taskbar_created_msg,
            selftest,
            exit_code: 0,
            saved_settings,
            settings_writable,
            store_writable,
            dbl_press: false,
            high_surrogate: None,
            wheel_acc: 0.0,
            wheel_t: 0.0,
            page_switch_t: 0.0,
            locked: false,
            display_off: false,
            tray_retries: 0,
            outgoing: None,
            page_anim: None,
            hover_arm: None,
            collapse_due: false,
            left_at: f64::NEG_INFINITY,
            menu_open: false,
            phone: None,
            phone_offer: Vec::new(),
            pair_offer: None,
            laid_floating: false,
            laid_spot: None,
            grab: None,
            notify: None,
            banners: services::banners::Banners::load(),
            keyboard_layout: None,
            brightness: None,
            mic_muted: None,
            camera: None,
            language_looks: 0,
            osd_looks: 0,
            agenda_done: 0,
        })
    }

    fn start(&mut self) {
        self.layout_surface();
        let fake = self.selftest.is_some();
        self.st.power = services::power::query();
        services::power::register(self.helper);
        if !fake {
            services::power::register_brightness(self.helper);
            self.sync_keys();
            // Reminders from now on (one missed while Hihu was closed is past).
            self.agenda_done = self.local_minute();
            self.agenda_schedule();
            let s = &self.st.settings;
            if (s.hud_brightness || s.hud_volume) && s.hud_hide_windows_brightness {
                services::osd::hide();
            }
        }
        if !fake {
            self.services = Some(Services::start(&self.sink, self.hwnd, self.helper, &self.st.settings));
            self.sync_top_watch();
            self.st.paired = self.services.as_ref().map(|s| paired_phones(s.pairs())).unwrap_or_default();
            self.sync_pair_offer();
            // Notifications on: listen if Windows already allows it (its
            // question waits for the page's Allow).
            if self.st.notifications_on() {
                self.start_notify(false);
            }
            if self.st.clipboard_on() {
                self.clipboard_on();
            }
            if self.st.notes_on() {
                self.notes_restore();
            }
            if self.st.screentime_on() {
                self.screentime_on();
            }
            // Start with Windows keeps pointing at this exe.
            crate::autostart::refresh_path();
            // The Downloader left the app: the tools a copy from before kept
            // (yt-dlp, QuickJS, FFmpeg, about 28 MB) go, once.
            let tools = paths::addons_dir().join("downloader");
            if tools.exists() {
                std::thread::spawn(move || match std::fs::remove_dir_all(&tools) {
                    Ok(()) => crate::plog::write("INFO", "add-ons: the Downloader's old tools deleted"),
                    Err(e) => crate::plog::write("WARN", &format!("add-ons: the Downloader's old tools stay ({e})")),
                });
            }
            self.try_add_tray();
            services::power::register_session(self.helper);
        }
        if let Err(e) = dragdrop::register(self.hwnd, self.sink.clone()) {
            pwarn!("drop target registration failed: {e}");
        }
        // Refresh shelf thumbnails.
        for p in self.st.store.shelf.paths() {
            self.st.cmds.push(Command::LoadThumb(p));
        }
        self.schedule_minute_timer();
        if let Some(t) = self.selftest.as_mut() {
            t.begin(&mut self.st);
            unsafe { SetTimer(self.hwnd, TIMER_SELFTEST, 50, None) };
        }
        self.request_frame();
    }

    fn shutdown(&mut self) -> i32 {
        self.save_now();
        // With Hihu closed, Windows shows its own pop-ups again.
        self.banners.set(&Default::default());
        services::keys::set(self.helper, false, false, false);
        self.camera = None;
        if self.st.settings.hud_hide_windows_brightness && self.selftest.is_none() {
            services::osd::restore();
        }
        self.screen_close(self.st.unix);
        self.save_screentime(false);
        self.settings_win = None;
        self.set_script_hotkey(false);
        self.glass.stop(self.hwnd, &mut self.gfx);
        if let Some(p) = self.phone.take() {
            p.stop();
        }
        if let Some(t) = self.tray.take() {
            t.remove();
        }
        if let Some(s) = self.services.take() {
            s.stop(self.helper);
        }
        dragdrop::revoke(self.hwnd);
        unsafe {
            let _ = DestroyWindow(self.hwnd);
            let _ = DestroyWindow(self.helper);
        }
        self.exit_code
    }

    /// Leave the message loop. Never call DestroyWindow while the app is
    /// borrowed: WM_DESTROY would re-enter and be swallowed.
    fn quit(&mut self, code: i32) {
        self.exit_code = code;
        self.clock.set_active(false);
        unsafe { PostQuitMessage(code) };
    }

    pub fn request_frame(&mut self) {
        self.clock.set_active(true);
    }

    /// Switch the renderer to `dpi` and size the window for that scale.
    fn set_dpi(&mut self, dpi: u32) {
        self.gfx.set_dpi(dpi as f32);
        self.layout_surface();
    }

    /// Size and place the window once for the largest island (the widest,
    /// tallest expanded page plus spring overshoot). The island is drawn
    /// centred in it, so a morph never moves or resizes the window: the
    /// centre stays put and both edges glide at sub-pixel precision. Call at
    /// start and whenever the scale, the monitor or the expanded width changes.
    fn layout_surface(&mut self) {
        let tallest = PageKind::ALL.iter().map(|p| pages::content_height(*p, &self.st)).fold(pages::TALLEST, f32::max);
        // Wide enough for the closed island's longest lyric line too, at its size setting.
        let closed = crate::pill::media::MAX_W * self.st.settings.island_scale_pct.max(100) as f32 / 100.0;
        self.layout_surface_for(self.st.settings.expanded_width.max(closed), pages::HEADER_H + tallest + pages::PAD);
        // The band Step aside watches follows the monitor and its scale.
        self.sync_top_watch();
    }

    /// Step aside on: have the full-screen watcher report the app windows
    /// at the top edge of the island's monitor, where the island hangs from
    /// (below a taskbar at the top). Off (always, since 0.2.6; the self-test
    /// still draws it): it looks at nothing, and nothing is under the island.
    fn sync_top_watch(&mut self) {
        let m = &self.monitor;
        let px = (TOP_BAND * self.gfx.scale()).ceil() as i32;
        let band = self.st.settings.step_aside.then(|| RECT { left: m.rect.left, top: m.top(), right: m.rect.right, bottom: m.top() + px });
        if band.is_none() {
            self.top_windows.clear();
        }
        if let Some(s) = &self.services {
            s.watch_top(band);
        }
    }

    /// Lay the window out for an island body up to `body_w`×`body_h` DIPs.
    fn layout_surface_for(&mut self, body_w: f32, body_h: f32) {
        let s = self.gfx.scale();
        let mon_w = self.monitor.rect.right - self.monitor.rect.left;
        let (w, h) = surface_size(&self.island, body_w, body_h, s, mon_w);
        self.laid_floating = self.island.floating();
        self.place_surface(w, h);
    }

    /// Put the `w`×`h` px window at the top of the monitor: centred, or as
    /// near the spot the island was dragged to as the monitor allows. Past
    /// that the island goes the rest of the way inside the window
    /// (`place_island`).
    fn place_surface(&mut self, w: i32, h: i32) {
        let s = self.gfx.scale();
        let (left, mon_w) = (self.monitor.rect.left, self.monitor.rect.right - self.monitor.rect.left);
        self.laid_spot = self.st.settings.island_x;
        let (x, home) = match self.laid_spot {
            None => (left + (mon_w - w) / 2, w as f32 / 2.0 / s),
            Some(f) => {
                let at = left + (f * mon_w as f32).round() as i32;
                let x = spot::window_left(left, mon_w, w, at);
                (x, (at - x) as f32 / s)
            }
        };
        self.home_cx = home;
        let surface = Surface { x, y: self.monitor.top(), w, h };
        if surface != self.surface {
            self.surface = surface;
            self.prev_canvas_px = None;
            self.mask = None;
            if let Err(e) = self.gfx.set_surface(w, h) {
                pwarn!("window surface {w}x{h} failed: {e}");
            }
        }
        self.place_island();
    }

    /// The island's centre in the window this frame. In the middle, where
    /// it rests. Dragged towards an edge, moved in just enough to keep its
    /// outline (as wide as it is now) in the window: opening there, it
    /// grows away from the edge.
    fn place_island(&mut self) {
        self.cx = match self.laid_spot {
            None => self.home_cx,
            Some(_) => spot::fit_centre(self.home_cx, self.surface.w as f32 / self.gfx.scale(), self.island.current().w, self.island.side_room()),
        };
    }

    /// The spot along the top edge changed since the window was laid out
    /// (a drag, the menu's Move island to the centre, Reload settings): lay
    /// it out there. Called every frame; a layout only on that change.
    fn sync_spot(&mut self) {
        if self.st.settings.island_x != self.laid_spot {
            self.place_surface(self.surface.w, self.surface.h);
        }
    }

    /// Where a mouse message's point is on the screen, pixels across.
    fn screen_x(&self, lp: LPARAM) -> i32 {
        let mut pt = POINT { x: (lp.0 & 0xffff) as u16 as i16 as i32, y: ((lp.0 >> 16) & 0xffff) as u16 as i16 as i32 };
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::ClientToScreen(self.hwnd, &mut pt);
        }
        pt.x
    }

    /// The pointer moved while pressing the closed island: once it has
    /// gone `spot::DRAG_SLOP` sideways, the island follows it along the top
    /// edge, as far as it stays on the screen, and back to the middle when
    /// it comes within `spot::SNAP` of it.
    fn drag_island(&mut self, lp: LPARAM) {
        let Some(g) = self.grab else { return };
        // Locked in place (Settings, or the menu): the press stays a click.
        if self.st.settings.lock_position {
            return;
        }
        let s = self.gfx.scale();
        let dx = self.screen_x(lp) - g.from_px;
        if !g.moved {
            if (dx.abs() as f32) < spot::DRAG_SLOP * s {
                return;
            }
            pinfo!("island: dragged along the top edge");
            // No WM_SETCURSOR comes while the mouse is captured.
            unsafe {
                if let Ok(c) = LoadCursorW(None, IDC_SIZEWE) {
                    SetCursor(c);
                }
            }
        }
        self.grab = Some(Grab { moved: true, ..g });
        let mon_w = (self.monitor.rect.right - self.monitor.rect.left) as f32 / s;
        let centre = spot::fit_centre(g.centre + dx as f32 / s, mon_w, self.island.current().w, self.island.side_room());
        self.st.settings.island_x = spot::remember(centre, mon_w);
        self.sync_spot();
        self.request_frame();
    }

    /// A drag along the top edge ended: the island stays there (saved), and
    /// doesn't open under the pointer that just let go of it.
    fn dropped_island(&mut self) {
        self.cancel_hover();
        match self.st.settings.island_x {
            Some(x) => pinfo!("island: moved to {:.0} % across the screen", x * 100.0),
            None => pinfo!("island: back in the middle"),
        }
        self.save_settings();
    }

    /// The island started or stopped floating since the window was laid out
    /// (a theme switch into or out of Pill, the self-test picking its theme
    /// after start, or the outline docking at the edge after rising back to
    /// it): lay the window out again for the new room, grown for the gap and
    /// the shadow or given back, so Classic and Glass keep exactly the
    /// window they start with and Pill gets its own. Called every frame
    /// after the island steps; a layout only on that change.
    fn sync_room(&mut self) {
        let floats = self.island.floating();
        if floats != self.laid_floating {
            self.layout_surface();
            let (w, h) = (self.surface.w, self.surface.h);
            self.log_queue.push(format!("island {}; window {w}x{h}", if floats { "floats below the top edge" } else { "hangs from the top edge" }));
        }
    }

    /// The island no longer fits the window (a target beyond the laid-out
    /// size, with room for its overshoot, or an outline past it): grow it.
    fn ensure_room(&mut self) {
        let g = self.island.current();
        let t = self.island.target();
        let (bw, bh) = ((t.w * OVERSHOOT).max(g.w), (t.h * OVERSHOOT).max(g.h));
        let s = self.gfx.scale();
        let need_w = ((bw + 2.0 * self.island.side_room()) * s).ceil() as i32;
        let need_h = ((bh + self.island.bottom_room() + self.island.top()) * s).ceil() as i32;
        if need_w > self.surface.w || need_h > self.surface.h {
            let tallest = PageKind::ALL.iter().map(|p| pages::content_height(*p, &self.st)).fold(pages::TALLEST, f32::max);
            let body_h = (pages::HEADER_H + tallest + pages::PAD).max(bh / OVERSHOOT * 1.1);
            pinfo!("island outgrew the window; growing it to fit {bw:.0}x{bh:.0} DIP");
            self.layout_surface_for(self.st.settings.expanded_width.max(bw / OVERSHOOT * 1.1), body_h);
        }
    }

    /// Add the tray icon; Explorer can be busy at logon, so retry a few times.
    fn try_add_tray(&mut self) {
        if self.tray.is_some() {
            return;
        }
        match Tray::add(self.helper) {
            Ok(t) => {
                self.tray = Some(t);
                self.tray_retries = 0;
            }
            Err(e) => {
                self.tray_retries += 1;
                if self.tray_retries <= 15 {
                    pwarn!("tray icon failed ({e}); retry {} in 2 s", self.tray_retries);
                    unsafe {
                        SetTimer(self.hwnd, TIMER_TRAY, 2000, None);
                    }
                } else {
                    pwarn!("tray icon failed ({e}); giving up");
                }
            }
        }
    }

    /// Write settings only when they changed and the file is safe to overwrite.
    fn save_settings(&mut self) {
        if self.st.settings == self.saved_settings {
            return;
        }
        if !self.settings_writable {
            pwarn!("settings.json couldn't be read at start; not overwriting it");
            return;
        }
        match save_json_atomic(&paths::settings_path(), &self.st.settings) {
            Ok(()) => self.saved_settings = self.st.settings.clone(),
            Err(e) => pwarn!("saving settings failed: {e}"),
        }
    }

    /// A click on the closed island (pressed and let go without dragging
    /// it): a notice jumps to its page or app, the download pill to its
    /// page, and the island opens.
    fn open_from_press(&mut self) {
        // Clicking a notice jumps to its page (a new version's
        // may be waiting under a volume change).
        let shown = self.mode == Mode::Notice;
        // Another app's notification opens that app instead.
        let mut opened = false;
        if let Some(n) = self.st.notice.take_if(|_| shown) {
            match n.action {
                Some(NoticeAction::OpenPage(p)) => self.st.page = p,
                Some(NoticeAction::StartFocus) => {
                    self.st.page = PageKind::Focus;
                    if !self.st.pomodoro.is_active() {
                        self.st.pomodoro.start(self.st.now);
                    }
                }
                Some(NoticeAction::OpenApp(id)) => {
                    shell::open_app(&id);
                    opened = true;
                }
                None => {}
            }
        }
        self.st.hud = None;
        if !opened {
            self.expand();
        }
    }

    fn release_mouse(&mut self) {
        self.input.down = false;
        self.input.pressed_id = None;
        unsafe {
            let _ = ReleaseCapture();
        }
    }

    // ------------------------------------------------------------ messages

    fn handle_island(&mut self, msg: u32, wp: WPARAM, lp: LPARAM, deferred: &mut Vec<Deferred>) -> Option<LRESULT> {
        match msg {
            WM_APP_FRAME => {
                // Clear `pending` after the frame so a slow frame can't keep the
                // queue permanently non-empty (which starves WM_TIMER).
                let tick = self.clock.tick();
                self.frame(deferred, Some(tick));
                self.clock.frame_handled();
                Some(LRESULT(0))
            }
            WM_APP_EVENT => {
                while let Ok(ev) = self.rx.try_recv() {
                    self.on_event(ev);
                }
                self.request_frame();
                Some(LRESULT(0))
            }
            WM_TIMER => {
                self.on_timer(wp.0);
                Some(LRESULT(0))
            }
            WM_MOUSEMOVE => {
                let p = self.mouse_dip(lp);
                if self.grab.is_some() {
                    self.input.mouse = Some(p);
                    self.drag_island(lp);
                    return Some(LRESULT(0));
                }
                if pill::diorama::slide(p.x, clock::mono()) {
                    // The glass ball in hand follows the pointer (captured, so anywhere).
                    self.input.mouse = Some(p);
                    self.request_frame();
                    return Some(LRESULT(0));
                }
                if self.over_stale_mask(p) {
                    // The pointer is over a part of the hit mask the outline
                    // already left: it isn't on the island.
                    if self.hovering {
                        self.hovering = false;
                        self.input.mouse = None;
                        self.cancel_hover();
                        self.pointer_left();
                        self.request_frame();
                    }
                    return Some(LRESULT(0));
                }
                if !self.hovering && self.should_make_way(p) {
                    self.start_making_way();
                    return Some(LRESULT(0));
                }
                self.input.mouse = Some(p);
                if !self.hovering {
                    self.hovering = true;
                    self.collapse_due = false;
                    unsafe {
                        let mut tme = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: self.hwnd, dwHoverTime: 0 };
                        let _ = TrackMouseEvent(&mut tme);
                        let _ = KillTimer(self.hwnd, TIMER_COLLAPSE);
                    }
                    if !self.expanded && clock::mono() - self.left_at < REOPEN_S {
                        // Back onto the island while it is still closing.
                        self.expand();
                    } else if !self.expanded {
                        self.arm_hover(p);
                    }
                } else if let Some(p0) = self.hover_arm.filter(|_| !self.expanded) {
                    // Dwell: moving on restarts the wait, so brushing past on
                    // the way to a tab or title bar doesn't open the island.
                    if (p.x - p0.x).hypot(p.y - p0.y) > DWELL_SLOP {
                        self.arm_hover(p);
                    }
                }
                self.request_frame();
                Some(LRESULT(0))
            }
            WM_MOUSELEAVE => {
                self.hovering = false;
                self.input.mouse = None;
                self.cancel_hover();
                self.pointer_left();
                self.request_frame();
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
                let p = self.mouse_dip(lp);
                if self.over_shadow(p) {
                    // A press on the floating pill's shadow (software renderer): not the island.
                    return Some(LRESULT(0));
                }
                self.dbl_press = msg == WM_LBUTTONDBLCLK;
                self.input.mouse = Some(p);
                self.input.down = true;
                self.input.just_pressed = true;
                self.input.press_pos = Some(p);
                self.input.pressed_id = self.hit_at(p).map(|h| h.id);
                unsafe { SetCapture(self.hwnd) };
                if !self.expanded {
                    let inline = self.st.notice.as_ref().and_then(|n| n.inline).filter(|_| self.input.pressed_id == Some(wid("notice.act")));
                    if let Some(a) = inline {
                        // The notice's inline pill acts in place without
                        // expanding. The press is consumed here, so the
                        // release can't click the pill a second time.
                        self.input.pressed_id = None;
                        self.run_inline_action(a);
                    } else if self.input.pressed_id == Some(wid(activities::PRAYER_OK_ID)) {
                        // The prayer countdown's OK, the same way.
                        self.input.pressed_id = None;
                        self.prayer_countdown_ok();
                    } else if self.input.pressed_id == Some(wid(pill::diorama::BALL_ID)) {
                        // Diorama's glass ball: in hand until the button comes
                        // up, sliding with the pointer; the island neither
                        // opens nor moves.
                        self.input.pressed_id = None;
                        pill::diorama::grab(&self.st, p.x);
                        self.cancel_hover();
                    } else {
                        // Opens when the button comes up (`open_from_press`),
                        // unless the pointer moves sideways first: then it
                        // drags the island along the top edge (`drag_island`).
                        let centre = (self.surface.x - self.monitor.rect.left) as f32 / self.gfx.scale() + self.cx;
                        self.grab = Some(Grab { from_px: self.screen_x(lp), centre, moved: false });
                        // Held still on it, it doesn't open by itself either.
                        self.cancel_hover();
                    }
                } else if self.text_focus && self.input.pressed_id != self.input.focused {
                    self.blur_text();
                }
                self.request_frame();
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                if pill::diorama::let_go(&self.st, clock::mono()) {
                    // The ball rolls on to its end; the island stays closed under the pointer.
                    self.input.pressed_id = None;
                    self.cancel_hover();
                }
                if let Some(g) = self.grab.take() {
                    // What the closed island showed under the press doesn't click.
                    self.input.pressed_id = None;
                    if g.moved {
                        self.dropped_island();
                    } else {
                        self.open_from_press();
                    }
                }
                let p = self.mouse_dip(lp);
                self.input.mouse = Some(p);
                let hit = self.hit_at(p).map(|h| h.id);
                if let (Some(pid), Some(h)) = (self.input.pressed_id, hit) {
                    // The second press of a double-click doesn't click again
                    // (no double toggles / double opens).
                    if pid == h && !self.dbl_press {
                        self.input.clicked = Some((h, MouseButton::Left));
                    }
                }
                if self.input.down {
                    self.input.released = self.input.pressed_id.map(|id| (id, p));
                }
                self.dbl_press = false;
                self.release_mouse();
                if !self.hovering_inside(p) {
                    self.input.mouse = None;
                }
                if !self.hovering {
                    // Released outside after a press inside (a slider drag).
                    self.pointer_left();
                }
                self.request_frame();
                Some(LRESULT(0))
            }
            WM_CAPTURECHANGED_ => {
                // Capture taken by someone else mid-press: drop the stale press
                // (a drag leaves the island where it got to).
                if lp.0 != self.hwnd.0 as isize && self.grab.take().is_some_and(|g| g.moved) {
                    self.dropped_island();
                }
                if lp.0 != self.hwnd.0 as isize && pill::diorama::let_go(&self.st, clock::mono()) {
                    self.request_frame();
                }
                if lp.0 != self.hwnd.0 as isize && (self.input.down || self.input.pressed_id.is_some()) {
                    self.input.down = false;
                    self.input.pressed_id = None;
                    if !self.hovering {
                        self.pointer_left();
                    }
                    self.request_frame();
                }
                Some(LRESULT(0))
            }
            WM_SYSCOMMAND if (wp.0 & 0xFFF0) == SC_CLOSE as usize => {
                // Alt+F4 while typing in a field just closes the island, not the app.
                self.blur_text();
                self.collapse();
                Some(LRESULT(0))
            }
            WM_RBUTTONUP => {
                let p = self.mouse_dip(lp);
                if self.over_shadow(p) {
                    return Some(LRESULT(0));
                }
                match self.hit_at(p) {
                    Some(h) => self.input.clicked = Some((h.id, MouseButton::Right)),
                    None => {
                        let mut pt = POINT::default();
                        unsafe {
                            let _ = GetCursorPos(&mut pt);
                        }
                        deferred.push(Deferred::Menu(pt));
                    }
                }
                self.request_frame();
                Some(LRESULT(0))
            }
            WM_MOUSEWHEEL => {
                let delta = ((wp.0 >> 16) & 0xffff) as u16 as i16 as f32 / WHEEL_DELTA as f32;
                self.input.wheel += delta;
                self.request_frame();
                Some(LRESULT(0))
            }
            WM_SETCURSOR => {
                let kind = self.input.mouse.and_then(|p| self.hit_at(p)).map(|h| h.cursor).unwrap_or(CursorKind::Arrow);
                unsafe {
                    let id = match kind {
                        CursorKind::Arrow => IDC_ARROW,
                        CursorKind::Hand => IDC_HAND,
                        CursorKind::Text => IDC_IBEAM,
                    };
                    if let Ok(c) = LoadCursorW(None, id) {
                        SetCursor(c);
                    }
                }
                Some(LRESULT(1))
            }
            WM_MOUSEACTIVATE => Some(LRESULT(if self.text_focus { MA_ACTIVATE as isize } else { MA_NOACTIVATE as isize })),
            WM_CHAR => {
                if self.text_focus {
                    let u = wp.0 as u32;
                    let c = if (0xD800..0xDC00).contains(&u) {
                        self.high_surrogate = Some(u as u16);
                        None
                    } else if (0xDC00..0xE000).contains(&u) {
                        self.high_surrogate.take().and_then(|h| char::decode_utf16([h, u as u16]).next().and_then(|r| r.ok()))
                    } else {
                        self.high_surrogate = None;
                        char::from_u32(u)
                    };
                    if let Some(c) = c.filter(|c| !c.is_control()) {
                        self.input.keys.push(KeyInput::Char(c));
                        self.request_frame();
                    }
                }
                Some(LRESULT(0))
            }
            WM_KEYDOWN => {
                if self.text_focus {
                    let k = match VIRTUAL_KEY(wp.0 as u16) {
                        VK_BACK => Some(KeyInput::Backspace),
                        VK_DELETE => Some(KeyInput::Delete),
                        VK_RETURN => Some(KeyInput::Enter),
                        VK_ESCAPE => Some(KeyInput::Escape),
                        VK_LEFT => Some(KeyInput::Left),
                        VK_RIGHT => Some(KeyInput::Right),
                        VK_UP => Some(KeyInput::Up),
                        VK_DOWN => Some(KeyInput::Down),
                        VK_HOME => Some(KeyInput::Home),
                        VK_END => Some(KeyInput::End),
                        VK_V if unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0 => {
                            if let Some(text) = services::clipboard::read_text(self.hwnd) {
                                for c in text.chars().filter(|c| !c.is_control()) {
                                    self.input.keys.push(KeyInput::Char(c));
                                }
                            }
                            None
                        }
                        _ => None,
                    };
                    if let Some(k) = k {
                        self.input.keys.push(k);
                    }
                    self.request_frame();
                    return Some(LRESULT(0));
                }
                None
            }
            WM_ACTIVATE => {
                if (wp.0 & 0xffff) as u32 == WA_INACTIVE && self.text_focus {
                    self.text_focus = false;
                    self.input.focused = None;
                    self.set_noactivate(true);
                    self.maybe_schedule_collapse();
                    self.request_frame();
                }
                None
            }
            WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE => {
                self.refresh_monitor();
                if msg == WM_SETTINGCHANGE {
                    self.glass.app_light = services::apps_use_light_theme();
                }
                if msg == WM_SETTINGCHANGE && self.st.settings.theme == Theme::Glass {
                    self.st.accent = theme_accent(Theme::Glass);
                } else if msg == WM_SETTINGCHANGE && self.st.settings.theme != Theme::Diorama {
                    if let Some(a) = services::system_accent() {
                        self.st.accent = a;
                    }
                }
                Some(LRESULT(0))
            }
            WM_CLOSE => {
                self.quit(0);
                Some(LRESULT(0))
            }
            _ => None,
        }
    }

    fn handle_helper(&mut self, msg: u32, wp: WPARAM, lp: LPARAM, deferred: &mut Vec<Deferred>) -> Option<LRESULT> {
        if msg == self.taskbar_created_msg && msg != 0 {
            if let Some(t) = self.tray.take() {
                t.remove();
            }
            self.tray_retries = 0;
            self.try_add_tray();
            if let Some(s) = self.services.as_ref() {
                s.on_taskbar_created();
            }
            // A restarted Explorer puts its taskbar at the top of the topmost band.
            self.start_raises();
            return Some(LRESULT(0));
        }
        match msg {
            services::keys::WM_APP_KEY => {
                self.on_key(wp.0, lp.0 != 0);
                return Some(LRESULT(0));
            }
            WM_APP_TRAY => {
                let ev = (lp.0 & 0xffff) as u32;
                match ev {
                    WM_LBUTTONUP => {
                        if self.expanded {
                            self.collapse();
                        } else {
                            self.expand();
                        }
                        self.request_frame();
                    }
                    WM_RBUTTONUP | WM_CONTEXTMENU => {
                        let mut pt = POINT::default();
                        unsafe {
                            let _ = GetCursorPos(&mut pt);
                        }
                        deferred.push(Deferred::Menu(pt));
                    }
                    _ => {}
                }
                Some(LRESULT(0))
            }
            WM_APP_APPBAR => {
                if let Some(s) = self.services.as_ref() {
                    if let Some(fs) = s.on_appbar_message(wp, lp, self.monitor.handle) {
                        self.on_event(AppEvent::Fullscreen(fs));
                    }
                }
                Some(LRESULT(0))
            }
            WM_POWERBROADCAST => {
                if let Some(pct) = services::power::brightness_change(wp, lp) {
                    self.hide_windows_osd();
                    let was = self.brightness.replace(pct);
                    if was.is_some_and(|w| w != pct) && self.st.settings.hud_brightness {
                        self.st.show_hud(HudKind::Brightness { percent: pct }, 1.6);
                        self.sync_fullscreen_hud();
                        self.request_frame();
                    }
                    return Some(LRESULT(1));
                }
                if let Some(on) = services::power::display_state_change(wp, lp) {
                    self.display_off = !on;
                    self.screen_presence();
                }
                let p = services::power::query();
                self.on_event(AppEvent::Power(p));
                self.request_frame();
                Some(LRESULT(1))
            }
            WM_WTSSESSION_CHANGE_ => {
                match wp.0 {
                    WTS_SESSION_LOCK_ => self.locked = true,
                    WTS_SESSION_UNLOCK_ => self.locked = false,
                    _ => {}
                }
                self.screen_presence();
                self.request_frame();
                Some(LRESULT(0))
            }
            WM_HOTKEY if wp.0 == HOTKEY_SCRIPT as usize => {
                self.st.now = clock::mono();
                pages::script::toggle(&mut self.st);
                self.request_frame();
                Some(LRESULT(0))
            }
            WM_QUERYENDSESSION => Some(LRESULT(1)),
            WM_ENDSESSION => {
                self.save_now();
                // Windows shuts down or signs out: what Hihu took from it goes
                // back now (its banners, its volume and brightness bar), as Quit does.
                if wp.0 != 0 && self.selftest.is_none() {
                    self.banners.set(&Default::default());
                    if self.st.settings.hud_hide_windows_brightness {
                        services::osd::restore();
                    }
                }
                Some(LRESULT(0))
            }
            WM_DISPLAYCHANGE | WM_SETTINGCHANGE => {
                self.refresh_monitor();
                None
            }
            _ => None,
        }
    }

    // ------------------------------------------------------------ the Settings window

    /// Advanced settings (the Settings page's last row, the menu's
    /// Advanced settings…): the island closes and the window opens, so the
    /// open island never sits over it.
    fn show_settings(&mut self, deferred: &mut Vec<Deferred>) {
        self.collapse();
        // Settings > Notifications says what Windows allows now (it may have changed there).
        self.notify_cmd(NotifyCmd::Check);
        // Settings > Meeting notes lists the microphones there are now.
        if self.st.notes_on() {
            self.st.microphones = services::mic::microphones();
        }
        self.open_settings_window(None, deferred);
    }

    /// Open the Settings window on `section` (General when it opens), or
    /// bring it forward, on that section when one is given.
    fn open_settings_window(&mut self, section: Option<Section>, deferred: &mut Vec<Deferred>) {
        match self.settings_win.as_mut() {
            Some(w) => {
                if let Some(s) = section {
                    w.view.show_section(s);
                }
                w.raise();
            }
            None => match SettingsWindow::new(self.monitor.work, self.gfx.dpi.round() as u32, section.unwrap_or(Section::General)) {
                Ok(w) => {
                    pinfo!("settings window: open");
                    self.settings_win = Some(w);
                    // Start with Windows is read off the UI thread; its answer draws the switch.
                    crate::autostart::report(self.sink.clone(), None);
                }
                Err(e) => {
                    pwarn!("settings window: couldn't open it ({e})");
                    return;
                }
            },
        }
        self.paint_settings(deferred);
    }

    /// Close it: the window, its renderer and its pixels go.
    fn close_settings_window(&mut self) {
        if self.settings_win.take().is_some() {
            pinfo!("settings window: closed");
        }
    }

    /// Draw the Settings window, run what its clicks asked for and draw it
    /// again with the result (a few times at most).
    fn paint_settings(&mut self, deferred: &mut Vec<Deferred>) {
        for _ in 0..4 {
            self.st.now = self.st.now.max(clock::mono());
            let now = self.st.now;
            let Some(w) = self.settings_win.as_mut() else { return };
            let out = w.paint(&mut self.st, now, None, false);
            if out.close {
                self.close_settings_window();
                return;
            }
            let ran = !self.st.cmds.is_empty();
            if ran {
                self.run_commands(deferred);
                self.request_frame();
            }
            let dirty = self.settings_win.as_ref().is_some_and(|w| w.view.dirty);
            if !(out.again || ran || dirty) {
                break;
            }
        }
        // A switch sliding, a highlight gliding: the next frames come from the clock.
        if self.settings_moving() {
            self.request_frame();
        }
    }

    /// Draw the Settings window on this frame when something in it changed
    /// (all of it), or still glides (only what moves).
    fn paint_settings_if_dirty(&mut self, deferred: &mut Vec<Deferred>) {
        let Some(w) = self.settings_win.as_ref() else { return };
        if w.view.dirty {
            self.paint_settings(deferred);
        } else if w.view.moving {
            self.st.now = self.st.now.max(clock::mono());
            let now = self.st.now;
            if let Some(w) = self.settings_win.as_mut() {
                let only = w.view.next_clip();
                let _ = w.paint(&mut self.st, now, only, false);
            }
        }
    }

    /// Something in the Settings window glides: the frame clock keeps
    /// ticking at the display's rate until it has arrived.
    fn settings_moving(&self) -> bool {
        self.settings_win.as_ref().is_some_and(|w| w.view.moving)
    }

    fn handle_settings_window(&mut self, hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, deferred: &mut Vec<Deferred>) -> Option<LRESULT> {
        let w = self.settings_win.as_mut().filter(|w| w.hwnd == hwnd)?;
        let xy = |lp: LPARAM| ((lp.0 & 0xffff) as u16 as i16 as i32, ((lp.0 >> 16) & 0xffff) as u16 as i16 as i32);
        match msg {
            WM_NCHITTEST => {
                // Empty parts of the card move the window; the shadow takes nothing.
                let (x, y) = xy(lp);
                let p = w.mouse_dip(x - w.at.0, y - w.at.1);
                let hit = if !w.card().contains(p) {
                    HTNOWHERE
                } else if w.drags_at(p) {
                    HTCAPTION
                } else {
                    HTCLIENT
                };
                Some(LRESULT(hit as isize))
            }
            // A double-click on an empty part doesn't maximize it.
            WM_NCLBUTTONDBLCLK => Some(LRESULT(0)),
            WM_MOUSEMOVE => {
                let (x, y) = xy(lp);
                let p = w.mouse_dip(x, y);
                if !w.tracking {
                    w.tracking = true;
                    unsafe {
                        let mut tme = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                        let _ = TrackMouseEvent(&mut tme);
                    }
                }
                // Drawn again only when what's under the pointer changes.
                if w.view.hover(p) {
                    self.paint_settings(deferred);
                }
                Some(LRESULT(0))
            }
            WM_MOUSELEAVE => {
                w.tracking = false;
                w.view.leave();
                self.paint_settings(deferred);
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN => {
                let (x, y) = xy(lp);
                let p = w.mouse_dip(x, y);
                w.view.press(p);
                unsafe {
                    SetCapture(hwnd);
                }
                self.paint_settings(deferred);
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                let (x, y) = xy(lp);
                let p = w.mouse_dip(x, y);
                w.view.release(p);
                unsafe {
                    let _ = ReleaseCapture();
                }
                self.paint_settings(deferred);
                Some(LRESULT(0))
            }
            WM_MOUSEWHEEL => {
                let raw = ((wp.0 >> 16) & 0xffff) as u16 as i16 as i32;
                let delta = raw as f32 / WHEEL_DELTA as f32;
                let (x, y) = xy(lp);
                let p = w.mouse_dip(x - w.at.0, y - w.at.1);
                // Whole notches glide; a precision touchpad's small steps follow the fingers.
                if raw % WHEEL_DELTA as i32 == 0 {
                    w.view.wheel(p, delta);
                } else {
                    w.view.scroll_by(p, delta * crate::settings_window::WHEEL_STEP);
                }
                self.paint_settings(deferred);
                Some(LRESULT(0))
            }
            WM_SETCURSOR if (lp.0 & 0xffff) as u32 == HTCLIENT => {
                let kind = w.view.input.mouse.map_or(CursorKind::Arrow, |p| w.view.cursor_at(p));
                unsafe {
                    let id = match kind {
                        CursorKind::Arrow => IDC_ARROW,
                        CursorKind::Hand => IDC_HAND,
                        CursorKind::Text => IDC_IBEAM,
                    };
                    if let Ok(c) = LoadCursorW(None, id) {
                        SetCursor(c);
                    }
                }
                Some(LRESULT(1))
            }
            // Windows switched between light and dark: the window follows.
            WM_SETTINGCHANGE => {
                w.refresh_mode();
                self.paint_settings(deferred);
                None
            }
            WM_KEYDOWN => {
                let ctrl = unsafe { GetKeyState(VK_CONTROL.0 as i32) } < 0;
                let search = &mut self.st.ui.settings_search;
                if wp.0 == VK_ESCAPE.0 as usize && !search.is_empty() {
                    // Escape clears a search first, then closes.
                    search.clear();
                    w.view.dirty = true;
                    self.paint_settings(deferred);
                } else if wp.0 == VK_ESCAPE.0 as usize || (ctrl && wp.0 == b'W' as usize) {
                    self.close_settings_window();
                } else if wp.0 == VK_BACK.0 as usize && search.pop().is_some() {
                    w.view.dirty = true;
                    self.paint_settings(deferred);
                }
                Some(LRESULT(0))
            }
            WM_CHAR => {
                // Typing searches the settings.
                if let Some(c) = char::from_u32(wp.0 as u32).filter(|c| !c.is_control()) {
                    if self.st.ui.settings_search.chars().count() < 40 {
                        self.st.ui.settings_search.push(c);
                        w.view.dirty = true;
                        self.paint_settings(deferred);
                    }
                }
                Some(LRESULT(0))
            }
            WM_CLOSE => {
                self.close_settings_window();
                Some(LRESULT(0))
            }
            WM_MOVE => {
                w.at = xy(lp);
                Some(LRESULT(0))
            }
            WM_DPICHANGED => {
                // Dragged onto a display with another scale: Windows' place for it, drawn at that scale.
                if let Some(r) = unsafe { (lp.0 as *const RECT).as_ref() } {
                    w.at = (r.left, r.top);
                }
                w.set_dpi((wp.0 & 0xffff) as u32);
                self.paint_settings(deferred);
                Some(LRESULT(0))
            }
            _ => None,
        }
    }

    fn on_timer(&mut self, id: usize) {
        match id {
            TIMER_HOVER => {
                unsafe {
                    let _ = KillTimer(self.hwnd, TIMER_HOVER);
                }
                if !self.hovering || self.expanded || self.grab.is_some() {
                    self.hover_arm = None;
                } else if self.mode == Mode::Notice && self.st.notice.as_ref().is_some_and(|n| n.inline.is_some()) {
                    // A notice with an inline pill stays put under a resting
                    // pointer so the pill can be clicked (a click on the rest
                    // of the notice still opens its page). The dwell stays
                    // armed: the next movement restarts it.
                } else if self.input.mouse.and_then(|p| self.hit_at(p)).is_some_and(|h| h.id == wid(activities::PRAYER_OK_ID)) {
                    // So does the prayer countdown under a pointer resting on
                    // its OK (resting on the rest of it still opens).
                } else if self.input.mouse.and_then(|p| self.hit_at(p)).is_some_and(|h| h.id == wid(pill::diorama::BALL_ID)) {
                    // And Diorama's resting pill under a pointer resting on its
                    // glass ball, which is there to be slid.
                } else {
                    self.expand();
                }
            }
            TIMER_SCREEN => self.screen_check(),
            TIMER_AGENDA => {
                unsafe {
                    let _ = KillTimer(self.hwnd, TIMER_AGENDA);
                }
                self.agenda_remind();
            }
            TIMER_OSD => {
                self.osd_looks = self.osd_looks.saturating_sub(1);
                services::osd::hide();
                if self.osd_looks == 0 {
                    unsafe {
                        let _ = KillTimer(self.hwnd, TIMER_OSD);
                    }
                }
            }
            TIMER_LANGUAGE => {
                self.language_looks = self.language_looks.saturating_sub(1);
                let found = services::keys::language();
                let changed = match (&found, self.keyboard_layout) {
                    (Some((hkl, _)), Some(was)) => *hkl != was,
                    _ => false,
                };
                if let Some((hkl, name)) = found {
                    self.keyboard_layout = Some(hkl);
                    if changed && self.st.settings.hud_language {
                        self.st.show_hud(HudKind::Language { name }, 1.6);
                        self.sync_fullscreen_hud();
                        self.request_frame();
                    }
                }
                if changed || self.language_looks == 0 {
                    self.language_looks = 0;
                    unsafe {
                        let _ = KillTimer(self.hwnd, TIMER_LANGUAGE);
                    }
                }
            }
            TIMER_MAKE_WAY => {
                // Back once the pointer left, the island opened (pushed up to
                // the top edge), or the setting went off.
                if !self.making_way || self.expanded || !self.st.settings.make_way || !self.pointer_near_rest() {
                    self.stop_making_way();
                }
            }
            TIMER_RAISE => {
                self.raises_left = self.raises_left.saturating_sub(1);
                if self.shown && !self.fullscreen_hidden {
                    self.raise_if_covered();
                } else {
                    self.raises_left = 0;
                }
                if self.raises_left == 0 {
                    unsafe {
                        let _ = KillTimer(self.hwnd, TIMER_RAISE);
                    }
                }
            }
            TIMER_COLLAPSE => {
                unsafe {
                    let _ = KillTimer(self.hwnd, TIMER_COLLAPSE);
                }
                if self.free_to_collapse() {
                    self.collapse();
                }
            }
            TIMER_WAKE => unsafe {
                let _ = KillTimer(self.hwnd, TIMER_WAKE);
            },
            TIMER_SAVE => {
                unsafe {
                    let _ = KillTimer(self.hwnd, TIMER_SAVE);
                }
                self.save_now();
            }
            TIMER_MINUTE => {
                self.schedule_minute_timer();
                self.close_idle_phone();
            }
            TIMER_TRAY => {
                unsafe {
                    let _ = KillTimer(self.hwnd, TIMER_TRAY);
                }
                self.try_add_tray();
            }
            TIMER_SELFTEST => {
                if let Some(mut t) = self.selftest.take() {
                    let done = t.step(self);
                    if let Some(code) = done {
                        unsafe {
                            let _ = KillTimer(self.hwnd, TIMER_SELFTEST);
                        }
                        self.quit(code);
                    }
                    self.selftest = Some(t);
                }
            }
            _ => {}
        }
        self.request_frame();
    }

    /// Save the Apple Music token copied from the browser (sealed with DPAPI)
    /// and take it off the clipboard.
    fn use_apple_music_token(&mut self) {
        let text = services::clipboard::read_text(self.hwnd).unwrap_or_default();
        let Some(token) = poni_core::applemusic::clean_user_token(&text) else {
            self.st.notify(Icon::Lyrics, palette::ORANGE, "No Apple Music token copied", "Copy media-user-token first", 6.0, None);
            return;
        };
        if let Err(e) = services::secret::save(&paths::apple_music_token_path(), &token) {
            pwarn!("apple music: cannot save the token ({e})");
            self.st.notify(Icon::Lyrics, palette::ORANGE, "Couldn't save the token", "Details in hihu.log", 5.0, None);
            return;
        }
        pinfo!("apple music: token saved");
        self.st.clip.forget(&token);
        self.own_clip = Some(String::new());
        services::clipboard::set_text(self.helper, "");
        if let Some(s) = self.services.as_ref() {
            s.apple_music_token_changed();
        }
        self.refetch_lyrics();
        self.st.notify(Icon::Lyrics, palette::BLUE, "Checking the Apple Music token", "One moment", 4.0, None);
    }

    fn forget_apple_music_token(&mut self) {
        if let Err(e) = services::secret::delete(&paths::apple_music_token_path()) {
            pwarn!("apple music: cannot remove the token ({e})");
            self.st.notify(Icon::Lyrics, palette::ORANGE, "Couldn't remove the token", "Details in hihu.log", 5.0, None);
            return;
        }
        pinfo!("apple music: token removed");
        if let Some(s) = self.services.as_ref() {
            s.apple_music_token_changed();
        }
        self.refetch_lyrics();
        self.st.notify(Icon::Lyrics, palette::BLUE, "Apple Music lyrics off", "Lyrics from LRCLIB only", 4.0, None);
    }

    fn on_apple_music(&mut self, status: AppleMusicStatus) {
        let (color, title, subtitle) = match status {
            AppleMusicStatus::Accepted => (palette::GREEN, "Apple Music lyrics on", "LRCLIB fills in the rest"),
            AppleMusicStatus::Rejected => (palette::ORANGE, "Apple Music token not accepted", "Copy a fresh one from music.apple.com"),
            AppleMusicStatus::Unreachable => (palette::ORANGE, "Can't reach Apple Music", "Token saved; trying again later"),
            AppleMusicStatus::NoAccess => (palette::ORANGE, "No lyrics from Apple Music", "They need an Apple Music subscription"),
        };
        self.st.notify(Icon::Lyrics, color, title, subtitle, 6.0, None);
    }

    fn show_held_notice(&mut self, now: f64) {
        if let Some((mut n, at)) = self.st.held.take() {
            if now - at <= crate::state::HELD_MAX_S {
                n.until = now + crate::state::NOTICE_S;
                self.st.notice = Some(n);
            }
        }
    }

    /// Look up the current track's lyrics again (the sources changed).
    fn refetch_lyrics(&mut self) {
        if !self.st.settings.lyrics {
            return;
        }
        let Some(m) = self.st.media.clone().filter(|m| !m.title.is_empty()) else { return };
        if let Some(svc) = self.services.as_ref() {
            self.st.lyrics = LyricsState { track_key: m.track_key.clone(), status: LyricsStatus::Loading, ..Default::default() };
            svc.request_lyrics(&m);
        }
    }

    /// Apply side effects of a settings change (from the menu, a reload or a page).
    fn settings_changed(&mut self, before: &Settings) {
        let now = &self.st.settings;
        if before.theme != now.theme {
            self.st.accent = theme_accent(now.theme);
        }
        if before.hide_in_fullscreen != now.hide_in_fullscreen {
            // Apply at once: show the island over the full-screen app, or hide it.
            self.on_event(AppEvent::Fullscreen(self.fullscreen_on));
        }
        if before.step_aside != self.st.settings.step_aside && self.selftest.is_none() {
            // Step aside starts or stops watching the top of the screen.
            self.sync_top_watch();
        }
        let now = &self.st.settings;
        if !before.lyrics && now.lyrics {
            if let Some(m) = self.st.media.clone() {
                let stale = self.st.lyrics.track_key != m.track_key || self.st.lyrics.status == LyricsStatus::Idle;
                if stale && !m.title.is_empty() {
                    if let Some(svc) = self.services.as_ref() {
                        self.st.lyrics = LyricsState { track_key: m.track_key.clone(), status: LyricsStatus::Loading, ..Default::default() };
                        svc.request_lyrics(&m);
                    }
                }
            }
        }
        self.st.pomodoro.cfg = self.st.settings.pomodoro.clone();
        // Prayer times: off stops it at once; a new place, or following the location again.
        let prayer_id = poni_core::addons::PRAYER_TIMES.id;
        if before.addon_on(prayer_id) && !self.st.settings.addon_on(prayer_id) {
            self.prayer_off();
        }
        if before.prayer.place != self.st.settings.prayer.place {
            self.resolve_prayer_place();
        }
        if !before.prayer.follow_location && self.st.settings.prayer.follow_location && self.st.prayer_on() {
            self.locate();
        }
        // An add-on switched on downloads its files; off deletes them.
        for a in poni_core::addons::ALL {
            let on = self.st.settings.addon_on(a.id);
            if before.addon_on(a.id) != on {
                pinfo!("add-ons: {} switched {}", a.id, if on { "on" } else { "off" });
                self.st.addons.insert(a.id, if !on { AddonStatus::Off } else if a.downloads() { AddonStatus::Checking } else { AddonStatus::Ready });
                if let Some(s) = self.services.as_ref() {
                    s.set_addon(a.id, on);
                }
            }
        }
        // Meeting notes off: a recording stops and its audio goes. On: what
        // shows of its key.
        let notes_id = poni_core::addons::MEETING_NOTES.id;
        let notes_on = self.st.settings.addon_on(notes_id);
        if before.addon_on(notes_id) != notes_on && self.selftest.is_none() {
            if notes_on {
                self.notes_restore();
            } else {
                self.notes_off();
            }
        }
        // Screen time switched on: its days back and it counts; off: it
        // stops, and the days go.
        let screen_id = poni_core::addons::SCREEN_TIME.id;
        let screen_on = self.st.settings.addon_on(screen_id);
        if before.addon_on(screen_id) != screen_on && self.selftest.is_none() {
            if screen_on {
                self.screentime_on();
            } else {
                self.screentime_off();
            }
        }
        // Clipboard history switched on: its history back and its listener
        // on; off: nothing watches, and the history goes. Its size and its
        // time apply at once.
        let clip_id = poni_core::addons::CLIPBOARD.id;
        let clip_on = self.st.settings.addon_on(clip_id);
        if before.addon_on(clip_id) != clip_on && self.selftest.is_none() {
            if clip_on {
                self.clipboard_on();
            } else {
                self.clipboard_off();
            }
        }
        if before.clipboard_max != self.st.settings.clipboard_max {
            self.st.clip.set_cap(self.st.settings.clipboard_max as usize);
        }
        if before.clipboard_keep_s != self.st.settings.clipboard_keep_s {
            self.st.clip.prune(self.st.unix, self.st.settings.clipboard_keep_s);
        }
        // Focus switched off: no timer keeps running without its page.
        let focus_id = poni_core::addons::FOCUS.id;
        if before.addon_on(focus_id) && !self.st.settings.addon_on(focus_id) {
            self.st.pomodoro.reset_all();
        }
        // Notifications switched on on the Add-ons page: listen, Windows'
        // question first; off: stop.
        let notify_was = before.addon_on(poni_core::addons::NOTIFICATIONS.id);
        if self.st.notify_can && notify_was != self.st.notifications_on() && self.selftest.is_none() {
            if self.st.notifications_on() {
                self.start_notify(true);
            } else {
                self.stop_notify();
            }
        }
        // Windows' brightness pop-up: back when it's no longer kept away.
        let osd = |s: &Settings| (s.hud_brightness || s.hud_volume) && s.hud_hide_windows_brightness;
        if osd(&before) && !osd(&self.st.settings) && self.selftest.is_none() {
            services::osd::restore();
        }
        // The keys the pop-ups answer.
        let s = &self.st.settings;
        if (before.hud_volume_keys, before.hud_locks, before.hud_language, before.hud_camera) != (s.hud_volume_keys, s.hud_locks, s.hud_language, s.hud_camera) && self.selftest.is_none() {
            self.sync_keys();
        }
        // Which apps show (Every app, a switch), or Hide Windows' pop-ups.
        if before.notifications.every_app != self.st.settings.notifications.every_app
            || before.notifications.hide_banners != self.st.settings.notifications.hide_banners
            || before.notifications.apps != self.st.settings.notifications.apps
        {
            self.sync_banners();
        }
        // The display the island hangs on, or its width when open (the
        // Settings window has both).
        if before.monitor != self.st.settings.monitor {
            self.monitor = window::pick_monitor(self.st.settings.monitor);
            self.set_dpi(self.monitor.dpi);
            self.refresh_monitor();
        } else if before.expanded_width != self.st.settings.expanded_width || before.island_scale_pct != self.st.settings.island_scale_pct {
            self.layout_surface();
        }
        if let Some(w) = self.settings_win.as_mut() {
            w.view.dirty = true;
        }
    }

    fn on_menu(&mut self, cmd: tray::MenuCmd) {
        use tray::MenuCmd::*;
        let before = self.st.settings.clone();
        match cmd {
            TogglePinned => self.st.pinned = !self.st.pinned,
            ShowSettings => self.show_settings(&mut Vec::new()),
            ToggleAutostart => {
                let on = crate::autostart::is_enabled();
                if let Err(e) = crate::autostart::set_enabled(!on) {
                    pwarn!("autostart toggle failed: {e}");
                }
                // The Settings page's switch follows.
                crate::autostart::report(self.sink.clone(), None);
            }
            AppleMusicToken => self.use_apple_music_token(),
            AppleMusicHelp => shell::open(APPLE_MUSIC_HELP_URL),
            AppleMusicForget => self.forget_apple_music_token(),
            Quit => {
                self.quit(0);
                return;
            }
        }
        self.settings_changed(&before);
        self.save_settings();
        self.request_frame();
    }

    // ------------------------------------------------------------ events

    pub fn on_event(&mut self, ev: AppEvent) {
        let now = clock::mono();
        self.st.now = now;
        let media_before = matches!(ev, AppEvent::Media(_) | AppEvent::MediaTimeline { .. }).then(|| self.media_snapshot());
        // What the Settings window shows that a service reports.
        let shown = matches!(
            ev,
            AppEvent::Autostart(_) | AppEvent::Addon { .. } | AppEvent::Located(_) | AppEvent::Notify(_)
        );
        if let Some(w) = self.settings_win.as_mut().filter(|_| shown) {
            w.view.dirty = true;
        }
        match ev {
            AppEvent::FrontWindow(h) => self.screen_front(HWND(h as *mut _)),
            AppEvent::Camera { on, app } => {
                if self.st.settings.hud_camera {
                    self.st.show_hud(HudKind::Camera { on, app }, 2.0);
                    self.sync_fullscreen_hud();
                    self.request_frame();
                }
            }
            AppEvent::MicMute(muted) => {
                if self.mic_muted.replace(muted).is_some_and(|was| was != muted) && self.st.settings.hud_mic {
                    self.st.show_hud(HudKind::Mic { muted }, 1.6);
                    self.sync_fullscreen_hud();
                    self.request_frame();
                }
            }
            AppEvent::Notes(reply) => self.notes_reply(*reply),
            AppEvent::NotesStopped(issue) => {
                pwarn!("notes: the recording stopped by itself ({issue:?})");
                self.notes_stop();
            }
            AppEvent::ScreenIcon(key, img) => {
                self.st.screen_icons.insert(key, img);
                self.request_frame();
            }
            AppEvent::ClipboardText(t) => {
                if self.own_clip.as_deref() == Some(t.as_str()) {
                    self.own_clip = None;
                } else if self.st.clipboard_on() {
                    self.st.clip.prune(self.st.unix, self.st.settings.clipboard_keep_s);
                    self.st.clip.push(&t, self.st.unix);
                    self.st.ui.clip_flash = None;
                    self.request_frame();
                }
            }
            AppEvent::Backdrop => {
                if self.glass.on_backdrop(self.hwnd, &mut self.gfx) {
                    self.request_frame();
                }
            }
            AppEvent::Media(session) => {
                let prev_key = self.st.media.as_ref().map(|m| m.track_key.clone());
                match session {
                    Some(mut s) => {
                        // Keep already-decoded art if the track didn't change.
                        if s.art.is_none() {
                            if let Some(prev) = &self.st.media {
                                if prev.track_key == s.track_key {
                                    s.art = prev.art.clone();
                                    s.art_color = prev.art_color;
                                }
                            }
                        }
                        let changed = prev_key.as_deref() != Some(s.track_key.as_str());
                        if changed {
                            self.st.lyrics = LyricsState { track_key: s.track_key.clone(), status: LyricsStatus::Idle, ..Default::default() };
                            if self.st.settings.lyrics && !s.title.is_empty() {
                                if let Some(svc) = self.services.as_ref() {
                                    self.st.lyrics.status = LyricsStatus::Loading;
                                    svc.request_lyrics(&s);
                                }
                            }
                        }
                        self.st.media = Some(s);
                    }
                    None => {
                        self.st.media = None;
                        self.st.lyrics = LyricsState::default();
                    }
                }
            }
            AppEvent::MediaTimeline { track_key, timeline } => {
                if let Some(m) = self.st.media.as_mut() {
                    if m.track_key == track_key {
                        m.timeline = timeline;
                    }
                }
            }
            AppEvent::MediaArt { track_key, art } => {
                if let Some(m) = self.st.media.as_mut() {
                    if m.track_key == track_key {
                        m.art_color = art.as_ref().map(|a| a.accent_color());
                        m.art = art;
                    }
                }
            }
            AppEvent::Lyrics { track_key, result } => {
                if self.st.lyrics.track_key == track_key {
                    match result {
                        crate::events::LyricsResult::Found { lyrics, source, ruby, wants_ja_readings } => {
                            self.st.lyrics.status = LyricsStatus::Found;
                            self.st.lyrics.lyrics = Some(lyrics);
                            self.st.lyrics.source = source;
                            self.st.lyrics.ruby = ruby;
                            self.st.lyrics.wants_ja_readings = wants_ja_readings;
                            // The first Japanese song whose kanji went unread
                            // switches Japanese readings on, as its switch in
                            // Add-ons would: it downloads once (3.8 MB) and
                            // the song is read again with romaji. Only people
                            // who play Japanese songs get it; switched off by
                            // hand, it stays off (the Music page still offers it).
                            if poni_core::addons::ja_switches_on(&self.st.settings, wants_ja_readings) {
                                pinfo!("lyrics: a Japanese song's kanji went unread; switching Japanese readings on");
                                self.st.settings.set_addon(poni_core::addons::JAPANESE_READINGS.id, true);
                                self.st.cmd(Command::SaveSettings);
                            }
                        }
                        crate::events::LyricsResult::Instrumental => self.st.lyrics.status = LyricsStatus::Instrumental,
                        crate::events::LyricsResult::NotFound => self.st.lyrics.status = LyricsStatus::NotFound,
                        crate::events::LyricsResult::Error(e) => self.st.lyrics.status = LyricsStatus::Error(e),
                    }
                }
            }
            AppEvent::Volume { level, muted } => {
                let first = !self.st.volume.known;
                self.st.volume = crate::state::VolumeState { level, muted, known: true };
                // Headset buttons, media keys and other apps change it too:
                // Windows' own bar stays away for those as well.
                if !first {
                    self.hide_windows_osd();
                }
                if !first && self.st.settings.hud_volume {
                    self.st.show_hud(HudKind::Volume { level, muted }, 1.6);
                    self.sync_fullscreen_hud();
                }
            }
            AppEvent::Power(p) => {
                let old = self.st.power;
                self.st.power = p;
                if self.st.settings.hud_power && p.has_battery {
                    if p.on_ac != old.on_ac {
                        self.st.show_hud(HudKind::Charging { percent: p.percent, plugged: p.on_ac }, 2.2);
                    } else if !p.on_ac && ((old.percent > 20 && p.percent <= 20) || (old.percent > 10 && p.percent <= 10)) {
                        self.st.show_hud(HudKind::LowBattery { percent: p.percent }, 4.0);
                    }
                }
            }
            AppEvent::Bluetooth { name, connected, battery, audio } => {
                if self.st.settings.hud_bluetooth {
                    self.st.show_hud(HudKind::Bluetooth { name, connected, battery, audio }, 2.6);
                }
            }
            AppEvent::Fullscreen(on) => {
                self.fullscreen_on = on;
                self.apply_fullscreen(clock::mono());
            }
            AppEvent::Foreground => self.start_raises(),
            AppEvent::TopWindows(places) => {
                // The self-test puts its own window under the island.
                if self.selftest.is_none() {
                    self.top_windows = places;
                    self.request_frame();
                }
            }
            AppEvent::ShelfAdd(paths) if self.expanded && self.st.page == PageKind::Script && pages::script::take_drop(&mut self.st, &paths) => {
                // A text file dropped on the Script page becomes the script.
                self.drag_hover = false;
                self.st.drag_hover = false;
                self.request_frame();
            }
            AppEvent::ShelfAdd(paths) => {
                let n = self.st.store.shelf.add_paths(&paths, self.st.unix);
                for p in &paths {
                    self.st.cmds.push(Command::LoadThumb(p.clone()));
                }
                self.st.save();
                self.st.page = PageKind::Shelf;
                self.drag_hover = false;
                self.st.drag_hover = false;
                self.expand();
                pinfo!("shelf: {n} new item(s)");
            }
            AppEvent::DragHover(on) => {
                self.drag_hover = on;
                self.st.drag_hover = on;
                if on {
                    // The Script page takes a text file; everything else goes to the Shelf.
                    if !(self.expanded && self.st.page == PageKind::Script) {
                        self.st.page = PageKind::Shelf;
                    }
                    self.expand();
                } else {
                    self.maybe_schedule_collapse();
                }
            }
            AppEvent::ShelfThumb { path, image } => {
                self.st.shelf_thumbs.insert(path, image);
            }
            AppEvent::AppleMusic(status) => self.on_apple_music(status),
            AppEvent::Phone(e) => self.on_phone(e),
            AppEvent::Paired(e) => self.on_paired(e),
            AppEvent::Autostart(on) => self.st.ui.autostart = Some(on),
            AppEvent::Addon { id, status } => {
                // A late report from before a switch-off doesn't bring it back.
                if self.st.settings.addon_on(id) || status == AddonStatus::Off {
                    self.st.addons.insert(id, status);
                }
                if id == poni_core::addons::PRAYER_TIMES.id {
                    if status == AddonStatus::Ready && self.st.settings.addon_on(id) && self.st.prayer.places.is_none() {
                        self.prayer_ready();
                    } else if status == AddonStatus::Off {
                        self.prayer_off();
                    }
                }
                // Japanese readings arrived for the song that asked for them:
                // read it again (its lyrics come from the cache).
                if id == poni_core::addons::JAPANESE_READINGS.id && status == AddonStatus::Ready && self.st.lyrics.wants_ja_readings {
                    if let (Some(svc), Some(m)) = (self.services.as_ref(), self.st.media.as_ref()) {
                        if m.track_key == self.st.lyrics.track_key {
                            pinfo!("lyrics: Japanese readings are on this PC; reading the song again");
                            svc.request_lyrics(m);
                        }
                    }
                }
            }
            AppEvent::Located(found) => self.on_located(found),
            AppEvent::Cities { file, got } => pages::prayer::cities_came(&mut self.st, file, got),
            AppEvent::Notify(ev) => self.on_notify(ev),
            AppEvent::AdzanEnded => self.adzan_ended(),
            AppEvent::Log(s) => pinfo!("{s}"),
        }
        if let Some(before) = media_before {
            self.after_media_change(before, now);
        }
        self.request_frame();
    }

    /// (player, track, playing) of the current media session.
    fn media_snapshot(&self) -> Option<(String, String, bool)> {
        self.st.media.as_ref().map(|m| (m.app_id.clone(), m.track_key.clone(), m.is_playing()))
    }

    /// Track the pause grace period and the title peek after a media update.
    fn after_media_change(&mut self, before: Option<(String, String, bool)>, now: f64) {
        let Some(m) = self.st.media.as_ref() else {
            self.st.media_paused_at = None;
            return;
        };
        let playing = m.is_playing();
        let same_player = before.as_ref().is_some_and(|b| b.0 == m.app_id);
        let new_track = before.as_ref().is_none_or(|b| b.1 != m.track_key);
        if playing {
            self.st.media_paused_at = None;
        } else if !same_player {
            // A different (or first) session that isn't playing: no grace.
            self.st.media_paused_at = None;
        } else if before.as_ref().is_some_and(|b| b.2) {
            // Playing -> paused: the pill lingers for a while.
            self.st.media_paused_at = Some(now);
        }
        // A new track starting while collapsed: show its title for a moment.
        if playing && new_track && !self.expanded {
            self.st.ui.peek_until = now + 3.0;
        }
    }

    // ------------------------------------------------------------ island state

    fn expand(&mut self) {
        if !self.expanded {
            self.expanded = true;
            self.cancel_hover();
            if self.st.page == PageKind::Notifications {
                self.st.recent_unseen = 0;
            }
        }
        self.request_frame();
    }

    /// Make way: the pointer came onto the closed island at `p` (window
    /// DIPs), not pushed up to the top edge.
    fn should_make_way(&self, p: Point) -> bool {
        self.st.settings.make_way
            && !self.expanded
            && !self.st.pinned
            && !self.input.down
            && self.grab.is_none()
            && !self.drag_hover
            && !self.rests_hidden()
            && matches!(self.mode, Mode::Idle | Mode::Activity(_))
            // The prayer countdown has its OK to press: it stays.
            && self.mode != Mode::Activity(activities::Activity::Prayer)
            && p.y > MAKE_WAY_EDGE
    }

    /// Step aside out of the pointer's way, and watch for it leaving.
    fn start_making_way(&mut self) {
        self.making_way = true;
        self.cancel_hover();
        unsafe {
            SetTimer(self.hwnd, TIMER_MAKE_WAY, MAKE_WAY_POLL_MS, None);
        }
        self.request_frame();
    }

    fn stop_making_way(&mut self) {
        self.making_way = false;
        unsafe {
            let _ = KillTimer(self.hwnd, TIMER_MAKE_WAY);
        }
        self.request_frame();
    }

    /// Make way: the pointer is still over (or near) where the island rests.
    fn pointer_near_rest(&self) -> bool {
        let mut pt = POINT::default();
        if unsafe { GetCursorPos(&mut pt) }.is_err() {
            return false;
        }
        let r = self.rest_rect();
        let m = (MAKE_WAY_MARGIN * self.gfx.scale()).round() as i32;
        pt.x >= r.left - m && pt.x < r.right + m && pt.y >= r.top && pt.y < r.bottom + m
    }

    /// (Re)start the hover dwell from `p`.
    fn arm_hover(&mut self, p: Point) {
        // Set to open on a click: resting on it does nothing.
        if self.st.settings.open_on_click {
            return;
        }
        self.hover_arm = Some(p);
        unsafe {
            SetTimer(self.hwnd, TIMER_HOVER, self.st.settings.hover_delay_ms.max(1), None);
        }
    }

    fn cancel_hover(&mut self) {
        self.hover_arm = None;
        unsafe {
            let _ = KillTimer(self.hwnd, TIMER_HOVER);
        }
    }

    fn collapse(&mut self) {
        if self.expanded {
            self.expanded = false;
            // Today's calendar opens on this month and today again.
            self.st.ui.cal_month = None;
            self.st.ui.cal_day = None;
            if self.text_focus {
                self.blur_text();
            }
            // Settings closes with the island: it opens again on the page it was opened from.
            if self.st.page == PageKind::Settings {
                self.st.page = self.st.ui.before_settings.take().filter(|p| pages::tabs(&self.st).contains(p)).unwrap_or_else(|| pages::first_tab(&self.st));
            }
            // A notice that arrived while expanded gets its full time now.
            let min_until = self.st.now + 4.0;
            if let Some(n) = self.st.notice.as_mut() {
                n.until = n.until.max(min_until);
            }
        }
        self.request_frame();
    }

    /// Nothing holds the open island: no pointer on it, no pin, typing,
    /// file drag, press or Phone drop.
    fn free_to_collapse(&self) -> bool {
        !self.hovering && !self.st.pinned && !self.text_focus && !self.drag_hover && !self.menu_open && self.input.pressed_id.is_none() && !self.phone_holds()
    }

    /// The right-click menu closed: the island stays open with the pointer
    /// back on it, and closes after the hold's grace without.
    fn menu_closed(&mut self) {
        if !std::mem::take(&mut self.menu_open) || !self.expanded || self.hovering {
            return;
        }
        let mut pt = POINT::default();
        let on_island = unsafe { GetCursorPos(&mut pt).is_ok() && windows::Win32::Graphics::Gdi::ScreenToClient(self.hwnd, &mut pt).as_bool() } && {
            let s = self.gfx.scale();
            self.hovering_inside(Point::new(pt.x as f32 / s, pt.y as f32 / s))
        };
        if on_island {
            // As a move onto it would: watch for the pointer leaving again.
            self.hovering = true;
            unsafe {
                let mut tme = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: self.hwnd, dwHoverTime: 0 };
                let _ = TrackMouseEvent(&mut tme);
            }
        } else {
            self.maybe_schedule_collapse();
        }
    }

    /// The pointer left the open island: it starts closing on the next frame,
    /// or after `collapse_delay_ms` when the user set a grace.
    fn pointer_left(&mut self) {
        if !self.expanded || !self.free_to_collapse() {
            return;
        }
        match self.st.settings.collapse_delay_ms {
            0 => {
                self.collapse_due = true;
                self.request_frame();
            }
            ms => unsafe {
                SetTimer(self.hwnd, TIMER_COLLAPSE, ms, None);
            },
        }
    }

    /// A hold ended (see [`HOLD_GRACE_MS`]): close soon unless the pointer is on the island.
    fn maybe_schedule_collapse(&mut self) {
        if self.expanded && !self.hovering && !self.st.pinned && !self.text_focus && !self.drag_hover && !self.menu_open && !self.phone_holds() {
            unsafe {
                SetTimer(self.hwnd, TIMER_COLLAPSE, HOLD_GRACE_MS.max(self.st.settings.collapse_delay_ms), None);
            }
        }
    }

    /// Over a full-screen app the island hides when the setting says Hide.
    /// Then nothing covers the app, and Windows sends its frames straight to
    /// the screen.
    fn fullscreen_hides(&self) -> bool {
        let s = &self.st.settings;
        // A pop-up shows over it for its moment, when set to.
        let hud = s.hud_over_fullscreen && self.st.hud.is_some();
        self.fullscreen_on && s.hide_in_fullscreen && !hud
    }

    /// A pop-up came or went while a full-screen app is in front: the
    /// island shows for it, then hides again.
    fn sync_fullscreen_hud(&mut self) {
        if self.fullscreen_on && self.fullscreen_hides() != self.fullscreen_hidden {
            self.apply_fullscreen(self.st.now);
        }
    }

    /// Shows or hides the island for what is in front now.
    fn apply_fullscreen(&mut self, now: f64) {
        let hide = self.fullscreen_hides();
        if hide != self.fullscreen_hidden {
            self.fullscreen_hidden = hide;
            // Hidden with no pop-ups over it: the volume keys go to Windows, whose flyout shows.
            services::keys::set_passing(hide && !self.st.settings.hud_over_fullscreen);
            unsafe {
                let _ = ShowWindow(self.hwnd, if hide { SW_HIDE } else { SW_SHOWNOACTIVATE });
            }
            self.gfx.set_shown(!hide);
            if hide {
                self.collapse();
                self.cancel_hover();
                unsafe {
                    let _ = KillTimer(self.hwnd, TIMER_COLLAPSE);
                }
                self.hovering = false;
                self.input.mouse = None;
            }
            pinfo!("fullscreen app {}: island {}", if self.fullscreen_on { "in front" } else { "gone" }, if hide { "hidden" } else { "shown" });
            // Notices held back wait for the full-screen app to leave.
            if !hide && !self.fullscreen_on {
                self.show_held_notice(now);
            }
        }
        if !hide {
            // A borderless game or a browser in F11 may have taken the
            // top of the topmost band, and an island shown again after
            // a full-screen app is not raised by showing it.
            self.start_raises();
        }
    }

    /// Alt+Shift+S belongs to Poni only while the Script page can read.
    fn set_script_hotkey(&mut self, on: bool) {
        if on == self.script_hotkey {
            return;
        }
        unsafe {
            if on {
                match RegisterHotKey(self.helper, HOTKEY_SCRIPT, MOD_ALT | MOD_SHIFT | MOD_NOREPEAT, u32::from(b'S')) {
                    Ok(()) => self.script_hotkey = true,
                    Err(e) => pwarn!("script: Alt+Shift+S is taken ({e})"),
                }
            } else {
                let _ = UnregisterHotKey(self.helper, HOTKEY_SCRIPT);
                self.script_hotkey = false;
            }
        }
    }

    /// The Script page: a reading pauses while its page isn't on screen, the
    /// island may close once nothing holds it, and the shortcut is Poni's
    /// only while there's a script to read on the page.
    fn script_tick(&mut self) {
        let shown = self.expanded && self.st.page == PageKind::Script && !self.locked && !self.display_off && !self.fullscreen_hidden;
        self.script_shown = shown;
        if !shown && self.st.script.playing() {
            self.st.script.pause(self.st.now);
        }
        let held = self.st.script_holds_island();
        if self.script_held && !held {
            self.maybe_schedule_collapse();
        }
        self.script_held = held;
        let can_read = self.st.page == PageKind::Script && !self.st.store.script.is_empty() && (self.expanded || self.st.script.active);
        self.set_script_hotkey(can_read && self.selftest.is_none());
    }

    // ------------------------------------------------------------ Meeting notes

    /// Each frame while a meeting is recorded: how loud each source is, for
    /// the page's bars.
    fn notes_tick(&mut self) {
        if let Some(r) = &self.notes_rec {
            (self.st.notes.mic, self.st.notes.pc) = r.levels();
        }
    }

    /// A press on the Notes page (or Settings' key row).
    fn notes_cmd(&mut self, c: crate::commands::NotesCmd) {
        use crate::commands::NotesCmd;
        let now = self.st.now;
        match c {
            NotesCmd::Start => self.notes_start(),
            NotesCmd::Stop => self.notes_stop(),
            NotesCmd::Retry => self.notes_send(),
            NotesCmd::Discard => {
                self.notes_take = None;
                self.st.ui.notes_kept = false;
                services::notes::clear_audio(None);
                self.st.notes.reset();
                pinfo!("notes: the kept recording was thrown away");
            }
            NotesCmd::Done => self.st.notes.reset(),
            NotesCmd::Open => {
                if let Some(last) = &self.st.notes.last {
                    shell::open(&last.file);
                }
            }
            NotesCmd::Folder => {
                let dir = paths::notes_dir();
                let _ = std::fs::create_dir_all(&dir);
                shell::open(&dir.to_string_lossy());
            }
            NotesCmd::Copy => {
                if let Some(last) = &self.st.notes.last {
                    let md = poni_core::notes::markdown(&last.notes, &last.started, false, poni_core::notes::Lang::from_key(&self.st.settings.notes_lang));
                    self.own_clip = Some(md.clone());
                    services::clipboard::set_text(self.helper, &md);
                    self.st.ui.notes_note = Some(("Copied".into(), now + 2.0));
                }
            }
            NotesCmd::PasteKey => {
                // Only what looks like a key: anything else on the clipboard is left alone.
                // The clipboard may be busy a moment (the app that copied still holds it).
                let mut pasted = None;
                for _ in 0..5 {
                    pasted = services::clipboard::read_text(self.helper);
                    if pasted.is_some() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(30));
                }
                let pasted = pasted.map(|s| s.trim().trim_matches('"').to_string());
                match &pasted {
                    None => pwarn!("notes: Paste key: no text on the clipboard"),
                    Some(k) if !services::notes::looks_like_key(k) => {
                        pwarn!("notes: Paste key: the clipboard's text ({} chars, spaces {}) isn't a key", k.chars().count(), k.contains(char::is_whitespace))
                    }
                    _ => {}
                }
                let line = match pasted.filter(|k| services::notes::looks_like_key(k)) {
                    Some(k) => match services::secret::save(&paths::gemini_key_path(), &k) {
                        Ok(()) => {
                            pinfo!("notes: a Gemini key was saved");
                            // A problem with the key is over: back to Try again.
                            if matches!(self.st.notes.phase, poni_core::notes::Phase::Failed(p) if matches!(p, poni_core::notes::Problem::NoKey | poni_core::notes::Problem::KeyRefused)) && !self.st.ui.notes_kept {
                                self.st.notes.reset();
                            }
                            "Key saved"
                        }
                        Err(e) => {
                            pwarn!("notes: can't save the key ({e})");
                            "Couldn't save the key"
                        }
                    },
                    None => "Copy your key first",
                };
                self.st.ui.notes_note = Some((line.into(), now + 3.0));
                self.notes_key_shown();
            }
            NotesCmd::OpenSaved(i) => {
                if let Some((p, ..)) = self.st.notes_saved.get(i) {
                    shell::open(&p.to_string_lossy());
                }
            }
            NotesCmd::RecycleSaved(i) => {
                if let Some((p, ..)) = self.st.notes_saved.get(i).cloned() {
                    if services::notes::recycle(&p) {
                        pinfo!("notes: a saved note went to the Recycle Bin");
                    }
                    self.st.notes_saved = services::notes::saved(&paths::notes_dir(), 20);
                }
            }
            NotesCmd::RemoveKey => {
                let _ = services::secret::delete(&paths::gemini_key_path());
                pinfo!("notes: the Gemini key was removed");
                self.notes_key_shown();
            }
        }
        self.request_frame();
    }

    /// Settings > Meeting notes > Microphone: a menu of Windows' default and
    /// each microphone by its whole name, at the pointer; the one picked is saved.
    fn pick_mic(&mut self) {
        let Some(owner) = self.settings_win.as_ref().map(|w| w.hwnd) else { return };
        self.st.microphones = services::mic::microphones();
        let mut choices: Vec<(String, String)> = vec![(String::new(), "Windows' default".into())];
        choices.extend(self.st.microphones.iter().cloned());
        let labels: Vec<(String, bool)> = choices.iter().map(|(id, name)| (name.clone(), *id == self.st.settings.notes_mic)).collect();
        let mut pt = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut pt);
        }
        if let Some(i) = tray::pick(owner, pt, &labels) {
            let before = self.st.settings.clone();
            self.st.settings.notes_mic = choices[i].0.clone();
            self.settings_changed(&before);
            self.save_settings();
        }
        if let Some(w) = self.settings_win.as_mut() {
            w.view.dirty = true;
        }
    }

    /// Settings > Tabs > Opens on: the last open tab, or one of the tabs.
    fn pick_start_page(&mut self) {
        let Some(owner) = self.settings_win.as_ref().map(|w| w.hwnd) else { return };
        let pages = crate::settings_window::start_pages(&self.st);
        let s = &self.st.settings;
        let mut labels = vec![("Last open tab".to_string(), s.remember_page)];
        labels.extend(pages.iter().map(|p| (p.title().to_string(), !s.remember_page && p.key() == s.start_page)));
        let mut pt = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut pt);
        }
        if let Some(i) = tray::pick(owner, pt, &labels) {
            let before = self.st.settings.clone();
            match i.checked_sub(1).and_then(|i| pages.get(i)) {
                Some(p) => {
                    self.st.settings.remember_page = false;
                    self.st.settings.start_page = p.key().to_string();
                }
                None => self.st.settings.remember_page = true,
            }
            self.settings_changed(&before);
            self.save_settings();
        }
        if let Some(w) = self.settings_win.as_mut() {
            w.view.dirty = true;
        }
    }

    /// Settings > About > Export: everything (but this PC's keys) in one file.
    fn backup_export(&mut self) {
        let dir = paths::backups_dir();
        let t = self.st.local;
        let path = dir.join(poni_core::backup::file_name(t.year, t.month, t.day));
        let text = poni_core::backup::make(&self.st.settings, &self.st.store, self.st.unix);
        let note = match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, text)) {
            Ok(()) => {
                pinfo!("backup: written");
                format!("Saved as {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
            }
            Err(e) => {
                pwarn!("backup: can't write it ({e})");
                "Couldn't save the backup".to_string()
            }
        };
        self.st.ui.backup_note = Some(note);
        if let Some(w) = self.settings_win.as_mut() {
            w.view.dirty = true;
        }
    }

    /// Settings > About > Import (pressed twice): the newest backup in the
    /// folder takes the place of the settings and the data.
    fn backup_import(&mut self) {
        let newest = std::fs::read_dir(paths::backups_dir())
            .map(|r| r.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).max())
            .ok()
            .flatten();
        let note = match newest.as_ref().map(|p| (p, std::fs::read_to_string(p))) {
            None => "No backup in the folder yet".to_string(),
            Some((_, Err(_))) => "Couldn't read the backup".to_string(),
            Some((p, Ok(text))) => match poni_core::backup::read(&text) {
                Err(why) => why.to_string(),
                Ok(b) => {
                    let before = self.st.settings.clone();
                    self.st.settings = b.settings;
                    self.st.store = b.store;
                    self.settings_changed(&before);
                    self.save_settings();
                    self.save_now();
                    self.agenda_schedule();
                    pinfo!("backup: put back");
                    format!("Put back from {}", p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
                }
            },
        };
        self.st.ui.backup_note = Some(note);
        self.request_frame();
        if let Some(w) = self.settings_win.as_mut() {
            w.view.dirty = true;
        }
    }

    /// What Settings and the page show of the key: its last four characters.
    fn notes_key_shown(&mut self) {
        self.st.ui.notes_key = services::notes::key().map(|k| k.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect());
        if let Some(w) = self.settings_win.as_mut() {
            w.view.dirty = true;
        }
    }

    /// Take notes: the microphone and the PC's sound start recording.
    fn notes_start(&mut self) {
        if self.notes_rec.is_some() || self.st.notes.live() || self.notes_take.is_some() {
            return;
        }
        // Whatever an earlier meeting left (the app closed on it) goes first.
        services::notes::clear_audio(None);
        let now = self.st.now;
        let started = self.st.local;
        let mic = Some(self.st.settings.notes_mic.clone()).filter(|m| !m.is_empty());
        match services::notes::Recording::start(&paths::notes_audio_dir(), &started, &self.sink, mic) {
            Ok((rec, mic, pc)) => {
                pinfo!("notes: listening (microphone {}, the PC's sound {})", if mic { "on" } else { "off" }, if pc { "on" } else { "off" });
                self.notes_started = started;
                self.st.notes.started(now, mic, pc);
                self.notes_rec = Some(rec);
            }
            Err(issue) => {
                pwarn!("notes: no recording ({issue:?})");
                self.st.ui.notes_kept = false;
                self.st.notes.failed(poni_core::notes::Problem::Capture(issue));
            }
        }
    }

    /// Stop: the meeting is over, its notes get written.
    fn notes_stop(&mut self) {
        use poni_core::notes::{CaptureIssue, Problem};
        let Some(rec) = self.notes_rec.take() else { return };
        match rec.stop() {
            Ok(take) if take.secs >= 1.0 && !take.parts.is_empty() => {
                // Kept, and asked about first: Write notes sends it.
                self.st.notes.recorded(take.secs);
                self.notes_take = Some(take);
                self.st.ui.notes_kept = true;
            }
            Ok(take) => {
                let _ = std::fs::remove_dir_all(&take.dir);
                self.st.ui.notes_kept = false;
                self.st.notes.failed(Problem::NothingHeard);
            }
            Err(issue) => {
                self.st.ui.notes_kept = false;
                self.st.notes.failed(Problem::Capture(if issue == CaptureIssue::MicDenied { issue } else { CaptureIssue::Failed }));
            }
        }
    }

    /// Hand the kept meeting over (again, for Try again).
    fn notes_send(&mut self) {
        let Some(take) = self.notes_take.clone() else { return };
        self.st.notes.stopped(take.parts.len() as u32, take.secs);
        let job = services::notes::Job {
            take,
            started: self.notes_started,
            lang: poni_core::notes::Lang::from_key(&self.st.settings.notes_lang),
            model: self.st.settings.notes_model.clone(),
            out: paths::notes_dir(),
        };
        services::notes::send(job, &self.sink);
    }

    /// The meeting on its way: how far, the notes saved, or what went wrong.
    /// With the island closed, the end of it shows as a notice.
    fn notes_reply(&mut self, r: poni_core::notes::Reply) {
        use poni_core::notes::Reply;
        if !self.st.notes_on() {
            return;
        }
        let now = self.st.now;
        let end = match &r {
            Reply::Saved(s) => Some(("Your notes are ready".to_string(), s.notes.title.clone(), true)),
            Reply::Failed(p) => Some((p.title().to_string(), "Open Notes to try again".to_string(), false)),
            _ => None,
        };
        let saved = matches!(r, Reply::Saved(_));
        self.st.notes.reply(r, now);
        if saved {
            self.notes_take = None;
            self.st.ui.notes_kept = false;
            self.st.notes_saved = services::notes::saved(&paths::notes_dir(), 20);
        }
        if let Some((title, subtitle, good)) = end {
            if !(self.expanded && self.st.page == PageKind::Notes) {
                let color = if good { self.st.accent } else { palette::TEXT_DIM };
                self.st.notify(Icon::Notes, color, title, subtitle, 8.0, Some(crate::state::NoticeAction::OpenPage(PageKind::Notes)));
            }
        }
        self.request_frame();
    }

    /// The add-on came on (or was on at start): what shows of the key, and
    /// a meeting the app closed on before its notes were saved, which waits
    /// on the page for Write it up or Throw away.
    fn notes_restore(&mut self) {
        self.notes_key_shown();
        if self.notes_take.is_some() || self.notes_rec.is_some() {
            return;
        }
        if let Some((take, started)) = services::notes::kept() {
            pinfo!("notes: a meeting of {:.0} s from before wasn't written up; it's kept", take.secs);
            self.notes_started = started;
            self.st.notes.secs = take.secs;
            self.notes_take = Some(take);
            self.st.ui.notes_kept = true;
            self.st.notes.failed(poni_core::notes::Problem::Unfinished);
        }
    }

    /// The add-on switched off: a recording stops, its audio goes, and the
    /// page forgets the meeting. The key stays (Settings removes it).
    fn notes_off(&mut self) {
        drop(self.notes_rec.take());
        self.notes_take = None;
        services::notes::clear_audio(None);
        self.st.notes = Default::default();
        self.st.ui.notes_kept = false;
    }

    // ------------------------------------------------------------ screen time

    /// Screen time came on (or was on at start): its saved days back, the
    /// foreground hook reporting, and a stretch for what's in front.
    fn screentime_on(&mut self) {
        let mut usage: poni_core::screentime::Usage = poni_core::store::load_json(&paths::screentime_path()).0;
        usage.prune(&self.st.local.day_key());
        self.st.screen = usage;
        self.screen.get_or_insert_with(Default::default);
        services::fullscreen::report_foreground(true);
        pinfo!("screen time: counting");
        self.screen_presence();
    }

    /// Screen time went off: nothing counts, and the counted days go.
    fn screentime_off(&mut self) {
        services::fullscreen::report_foreground(false);
        unsafe {
            let _ = KillTimer(self.hwnd, TIMER_SCREEN);
        }
        self.screen = None;
        self.st.screen = poni_core::screentime::Usage::default();
        self.st.screen_live = None;
        self.st.screen_icons.clear();
        self.screen_dirty = false;
        if let Err(e) = std::fs::remove_file(paths::screentime_path()) {
            if e.kind() != std::io::ErrorKind::NotFound {
                pwarn!("screen time: couldn't delete the counted days ({e})");
            }
        }
        pinfo!("screen time: stopped");
    }

    /// Someone can be at the PC: not locked, the screen on.
    fn screen_present(&self) -> bool {
        !self.locked && !self.display_off
    }

    /// The PC was locked or unlocked, or its screen went off or on: the
    /// stretch ends while nobody's there, and the minute's look rests; back,
    /// a stretch starts for what's in front.
    fn screen_presence(&mut self) {
        if self.screen.is_none() {
            return;
        }
        if self.screen_present() {
            if let Some(t) = self.screen.as_mut() {
                t.idle = false;
            }
            self.screen_open_front();
            unsafe {
                SetTimer(self.hwnd, TIMER_SCREEN, crate::screentime::CHECK_MS, None);
            }
        } else {
            self.screen_close(self.st.unix);
            unsafe {
                let _ = KillTimer(self.hwnd, TIMER_SCREEN);
            }
            self.save_screentime(true);
        }
    }

    /// End the stretch under way at `at` (unix seconds).
    fn screen_close(&mut self, at: i64) {
        let Some(t) = self.screen.as_mut() else { return };
        if t.close(&mut self.st.screen, at) {
            self.screen_dirty = true;
        }
        self.st.screen_live = None;
    }

    /// Start a stretch for what's in front now.
    fn screen_open_front(&mut self) {
        let pid = unsafe { windows::Win32::System::Threading::GetCurrentProcessId() };
        let (now, day) = (self.st.unix, self.st.local.day_key());
        let Some(t) = self.screen.as_mut() else { return };
        let front = t.foreground(pid);
        t.close(&mut self.st.screen, now).then(|| self.screen_dirty = true);
        t.open(front, now, &day);
        self.st.screen_live = Self::live_of(t);
    }

    fn live_of(t: &crate::screentime::Tracker) -> Option<crate::state::ScreenLive> {
        t.current().map(|(f, since, day)| crate::state::ScreenLive { key: f.key.clone(), name: f.name.clone(), path: f.path.clone(), since, day: day.to_string() })
    }

    /// Another window came to the front.
    fn screen_front(&mut self, hwnd: HWND) {
        if !self.screen_present() {
            return;
        }
        let pid = unsafe { windows::Win32::System::Threading::GetCurrentProcessId() };
        let (now, day) = (self.st.unix, self.st.local.day_key());
        let Some(t) = self.screen.as_mut() else { return };
        t.idle = false;
        if t.switch(&mut self.st.screen, hwnd, pid, now, &day) {
            self.screen_dirty = true;
        }
        self.st.screen_live = Self::live_of(t);
        if self.expanded && self.st.page == PageKind::Screentime {
            self.request_frame();
        }
    }

    /// The minute's look: nobody there for a while ends the stretch back at
    /// the last touch; back, one starts; otherwise a cut (the page and the
    /// file keep up, and a new day starts at midnight). Then a save if
    /// anything changed.
    fn screen_check(&mut self) {
        if self.screen.is_none() || !self.screen_present() {
            return;
        }
        self.refresh_time();
        let idle = crate::screentime::idle_secs();
        let was_idle = self.screen.as_ref().is_some_and(|t| t.idle);
        if idle >= crate::screentime::IDLE_S {
            if !was_idle {
                self.screen_close(self.st.unix - idle as i64);
                if let Some(t) = self.screen.as_mut() {
                    t.idle = true;
                }
            }
        } else {
            if let Some(t) = self.screen.as_mut() {
                t.idle = false;
            }
            self.screen_open_front();
        }
        if self.st.screen.prune(&self.st.local.day_key()) {
            self.screen_dirty = true;
        }
        self.save_screentime(true);
        if self.expanded && self.st.page == PageKind::Screentime {
            self.request_frame();
        }
    }

    /// Write the counted days when they changed (`thread`: off the UI
    /// thread; at the end, right here).
    fn save_screentime(&mut self, thread: bool) {
        if !std::mem::take(&mut self.screen_dirty) || self.screen.is_none() || self.selftest.is_some() {
            return;
        }
        let usage = self.st.screen.clone();
        let save = move || {
            if let Err(e) = poni_core::store::save_json_atomic(&paths::screentime_path(), &usage) {
                crate::plog::write("WARN", &format!("screen time: couldn't save ({e})"));
            }
        };
        if thread {
            let _ = std::thread::Builder::new().name("poni-screen-save".into()).spawn(save);
        } else {
            save();
        }
    }

    /// The Screen time page's apps without an icon yet: each program's,
    /// once, on a thread of its own.
    fn screen_icons(&mut self, list: Vec<(String, String)>) {
        thread_local! {
            static ASKED: std::cell::RefCell<std::collections::HashSet<String>> = std::cell::RefCell::new(std::collections::HashSet::new());
        }
        let list: Vec<(String, String)> = list.into_iter().filter(|(k, _)| ASKED.with(|a| a.borrow_mut().insert(k.clone()))).collect();
        if list.is_empty() {
            return;
        }
        let sink = self.sink.clone();
        let _ = std::thread::Builder::new().name("poni-screen-icons".into()).spawn(move || {
            for (key, path) in list {
                if let Ok(img) = services::imaging::shell_icon(&path, 48) {
                    sink.send(AppEvent::ScreenIcon(key, img));
                }
            }
        });
    }

    /// The Clipboard add-on came on (or was on at start): its saved history
    /// back, and its listener on a thread of its own.
    fn clipboard_on(&mut self) {
        let s = &self.st.settings;
        let items: Vec<poni_core::cliphist::ClipItem> = services::secret::load(&paths::clipboard_path()).and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
        self.st.clip = poni_core::cliphist::ClipHistory::from_items(items, s.clipboard_max as usize);
        self.st.clip.prune(self.st.unix, s.clipboard_keep_s);
        self.clip_saved = self.st.clip.rev();
        if self.clip_listener.is_none() {
            self.clip_listener = match services::clipboard::ClipboardHistory::start(self.sink.clone(), true) {
                Ok(l) => {
                    pinfo!("clipboard: watching for copies ({} kept)", self.st.clip.len());
                    Some(l)
                }
                Err(e) => {
                    pwarn!("clipboard: can't watch for copies: {e}");
                    None
                }
            };
        }
    }

    /// The Clipboard add-on went off: nothing watches the clipboard, and
    /// the history goes, in memory and on disk.
    fn clipboard_off(&mut self) {
        if let Some(l) = self.clip_listener.take() {
            l.stop();
            pinfo!("clipboard: stopped watching");
        }
        self.st.clip = poni_core::cliphist::ClipHistory::new(self.st.settings.clipboard_max as usize);
        self.clip_saved = self.st.clip.rev();
        self.st.ui.clip_flash = None;
        self.st.ui.clip_scroll = 0.0;
        if let Err(e) = services::secret::delete(&paths::clipboard_path()) {
            if e.kind() != std::io::ErrorKind::NotFound {
                pwarn!("clipboard: couldn't delete the saved history ({e})");
            }
        }
    }

    /// Save the history (sealed) when it changed since the last save, off
    /// the UI thread. Each frame; a write only after a copy, a pin, a removal.
    fn save_clipboard_if_changed(&mut self) {
        if !self.st.clipboard_on() || self.st.clip.rev() == self.clip_saved || self.selftest.is_some() {
            return;
        }
        self.clip_saved = self.st.clip.rev();
        let Ok(json) = serde_json::to_string(&self.st.clip.items()) else { return };
        let path = paths::clipboard_path();
        let spawned = std::thread::Builder::new().name("poni-clip-save".into()).spawn(move || {
            if let Err(e) = services::secret::save(&path, &json) {
                pwarn!("clipboard: couldn't save the history ({e})");
            }
        });
        if let Err(e) = spawned {
            pwarn!("clipboard: no thread to save on: {e}");
        }
    }

    /// Out of screenshots, recordings and screen shares while Show in
    /// screenshots is off (each frame; Windows is told only on a change).
    fn sync_capture_setting(&mut self) {
        let hide = !self.st.settings.show_in_capture && self.selftest.is_none();
        if self.capture_hidden == Some(hide) {
            return;
        }
        let ok = !hide || window::windows_build().is_some_and(|b| b >= crate::glass::EXCLUDE_FROM_CAPTURE_BUILD);
        let ok = ok && window::exclude_from_capture(self.hwnd, window::CaptureOwner::Setting, hide);
        pinfo!("capture: {}", match (hide, ok) {
            (false, _) => "the island shows in screenshots",
            (true, true) => "the island is left out of screenshots",
            (true, false) => "Windows can't leave the island out of screenshots",
        });
        self.capture_hidden = Some(hide);
    }

    /// Out of screen shares and recordings while the Script page shows, and
    /// until the island has closed over it.
    fn sync_script_capture(&mut self, on: bool) {
        if on == self.script_excluded {
            return;
        }
        self.script_excluded = on;
        if !on {
            window::exclude_from_capture(self.hwnd, window::CaptureOwner::Script, false);
            return;
        }
        let ok = window::windows_build().is_some_and(|b| b >= crate::glass::EXCLUDE_FROM_CAPTURE_BUILD)
            && window::exclude_from_capture(self.hwnd, window::CaptureOwner::Script, true);
        if self.st.script_hidden != Some(ok) {
            pinfo!("script: {}", if ok { "left out of screen capture" } else { "Windows can't leave the island out of screen capture" });
        }
        self.st.script_hidden = Some(ok);
    }

    /// Puts the island back on top after another window came forward. Apps
    /// often make themselves topmost just after they take the focus, with no
    /// event of their own, so a few more checks follow over the next two
    /// seconds. (Exclusive full-screen still covers every window.)
    fn start_raises(&mut self) {
        if !self.shown || self.fullscreen_hidden {
            return;
        }
        self.raise_if_covered();
        self.raises_left = RAISE_REPEATS;
        unsafe {
            SetTimer(self.hwnd, TIMER_RAISE, RAISE_INTERVAL_MS, None);
        }
    }

    /// Raises the island when a window sits above it or it lost its topmost
    /// style, and logs each culprit once.
    fn raise_if_covered(&mut self) {
        let topmost = unsafe { GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE) } & WS_EX_TOPMOST.0 as isize != 0;
        let above = services::fullscreen::window_above(self.hwnd);
        if topmost && above.is_none() {
            return;
        }
        let key = if topmost { above.clone().unwrap_or_default() } else { String::new() };
        if self.covered_by.insert(key) {
            match (&above, topmost) {
                (_, false) => pinfo!("island lost its topmost style; raised again"),
                (Some(class), true) => pinfo!("island covered by a {class} window; raised again"),
                (None, true) => {}
            }
        }
        self.raise_topmost();
    }

    /// Top of the topmost band, without taking the focus.
    fn raise_topmost(&self) {
        unsafe {
            let _ = SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }

    fn set_noactivate(&self, on: bool) {
        unsafe {
            let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            let flag = WS_EX_NOACTIVATE.0 as isize;
            let new = if on { ex | flag } else { ex & !flag };
            if new != ex {
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, new);
            }
        }
    }

    fn focus_text(&mut self) {
        if self.text_focus {
            return;
        }
        self.text_focus = true;
        unsafe {
            self.prev_foreground = GetForegroundWindow();
            self.set_noactivate(false);
            let _ = SetForegroundWindow(self.hwnd);
            let _ = SetFocus(self.hwnd);
        }
    }

    fn blur_text(&mut self) {
        self.input.focused = None;
        if !self.text_focus {
            return;
        }
        self.text_focus = false;
        self.set_noactivate(true);
        unsafe {
            if !self.prev_foreground.is_invalid() && IsWindow(self.prev_foreground).as_bool() && GetForegroundWindow() == self.hwnd {
                let _ = SetForegroundWindow(self.prev_foreground);
            }
        }
        self.maybe_schedule_collapse();
    }

    fn refresh_monitor(&mut self) {
        let m = match self.st.settings.monitor {
            MonitorChoice::Cursor => {
                window::monitor_info(windows::Win32::Graphics::Gdi::HMONITOR(self.monitor.handle as *mut core::ffi::c_void))
                    .unwrap_or_else(|| window::pick_monitor(MonitorChoice::Primary))
            }
            c => window::pick_monitor(c),
        };
        if m != self.monitor {
            pinfo!("monitor: {:?} dpi {}", m.rect, m.dpi);
            self.monitor = m;
            self.set_dpi(m.dpi);
            self.layout_surface();
        }
        // The refresh rate may have changed too (60 <-> 144 Hz).
        self.clock.set_monitor(m.handle);
        self.request_frame();
    }

    fn mouse_dip(&self, lp: LPARAM) -> Point {
        let x = (lp.0 & 0xffff) as u16 as i16 as f32;
        let y = ((lp.0 >> 16) & 0xffff) as u16 as i16 as f32;
        let s = self.gfx.scale();
        Point::new(x / s, y / s)
    }

    fn hit_at(&self, p: Point) -> Option<Hit> {
        self.hits.iter().rev().find(|h| h.rect.contains(p)).copied()
    }

    /// Whether `p` is over a part of the window that takes the mouse: the
    /// whole canvas while expanded (hit margin), only the island collapsed
    /// (floating: its outline and the gap above it, see [`hit_mask_for`]).
    fn hovering_inside(&self, p: Point) -> bool {
        if self.expanded {
            return self.island.canvas_rect_at(self.cx).contains(p);
        }
        if self.island.floating() {
            return self.hit_mask().contains(p);
        }
        self.island.body_rect_at(self.cx).contains(p)
    }

    /// Where the window takes the mouse now (see [`hit_mask_for`]).
    fn hit_mask(&self) -> HitMask {
        hit_mask_for(&self.island, self.cx, self.expanded, self.rests_hidden())
    }

    /// The closed island rests invisible, a strip at the top edge that only
    /// catches the pointer: the Hidden idle look, or stepped aside.
    fn rests_hidden(&self) -> bool {
        self.mode == Mode::Aside || (self.mode == Mode::Idle && self.st.settings.idle_style == IdleStyle::Hidden)
    }

    /// Step aside: where the island rests (what it shows at rest, the clock
    /// or the song, never the strip it steps aside to), from the edge it
    /// hangs from to its bottom, in screen pixels. Classic's shoulders and
    /// the floating Pill's gap count: they cover what is under them too.
    fn rest_rect(&self) -> RECT {
        let g = activities::geom(&activities::resting_mode(&self.st), &self.st);
        let floating = self.island.floating();
        let w = g.w + if floating { 0.0 } else { 2.0 * SHOULDER };
        let h = g.h + if floating { pill::TOP_GAP } else { 0.0 };
        let s = self.gfx.scale();
        let cx = self.surface.x as f32 + self.cx * s;
        let left = (cx - w * s / 2.0).floor() as i32;
        let right = (cx + w * s / 2.0).ceil() as i32;
        RECT { left, top: self.surface.y, right, bottom: self.surface.y + (h * s).ceil() as i32 }
    }

    /// Step aside (off since 0.2.6, see `Settings::step_aside`) while an app window
    /// reaches under where the island rests. Over a full-screen app the
    /// full-screen choice decides instead (Hide, or Stay on top). Each frame, before the mode is picked: it only
    /// compares a few rects.
    fn sync_aside(&mut self) {
        let r = self.rest_rect();
        let under = self.top_windows.iter().any(|t| t[0] < r.right && t[2] > r.left && t[1] < r.bottom && t[3] > r.top);
        let step = self.st.settings.step_aside && !self.fullscreen_on && under;
        let aside = step || self.making_way;
        if aside != self.st.aside {
            self.st.aside = aside;
            // (Making way comes and goes with the pointer: not worth a log line each time.)
            if step || !self.making_way {
                let why = if aside { "a window is under the island; it rests out of the way" } else { "nothing under the island; it rests as before" };
                self.log_queue.push(format!("step aside: {why}"));
            }
        }
    }

    /// Whether `p`, where the window took the mouse, is off the island.
    /// GPU renderer: the invisible hit mask trails a moving outline by up to
    /// [`MASK_INTERVAL`], and a pointer over the part the island already left
    /// (well outside its shape and shoulders) gets no hover. Software
    /// renderer: see [`App::over_shadow`].
    fn over_stale_mask(&self, p: Point) -> bool {
        if self.input.down {
            return false;
        }
        if !self.gfx.needs_hit_mask() {
            return self.over_shadow(p);
        }
        if self.island.settled() {
            return false;
        }
        let r = if self.expanded { self.island.canvas_rect_at(self.cx) } else { self.island.hover_rect_at(self.cx).inset(-SHOULDER - 2.0, 0.0) };
        !Rect::new(r.x, r.y, r.w, r.h + 2.0).contains(p)
    }

    /// Software renderer, floating island (the Pill theme) collapsed: a
    /// layered window takes the mouse wherever its pixels aren't fully
    /// transparent. That renderer draws the pill without its shadow
    /// (`pill::shape::draw_body`), so what's left beside the outline is its
    /// anti-aliased edge: a pointer there is off the island (no hover, no
    /// click, no menu). (The GPU renderer's hit mask decides on its own.)
    /// `p` is a pixel's corner: its centre is tested, so an edge pixel the
    /// outline mostly covers still counts.
    fn over_shadow(&self, p: Point) -> bool {
        if self.expanded || !self.island.floating() || self.gfx.needs_hit_mask() {
            return false;
        }
        let half = 0.5 / self.gfx.scale();
        !self.hit_mask().contains(Point::new(p.x + half, p.y + half))
    }

    fn target_mode(&self) -> Mode {
        if self.expanded {
            return Mode::Expanded(self.st.page);
        }
        activities::collapsed_mode(&self.st)
    }

    /// Island geometry for `m`. The resting pill lifts a little under the
    /// pointer: instant feedback while the hover dwell runs.
    fn geom_for(&self, m: &Mode) -> Geom {
        let g = activities::geom(m, &self.st);
        let liftable = match m {
            Mode::Idle => self.st.settings.idle_style != IdleStyle::Hidden,
            Mode::Activity(_) => true,
            _ => false,
        };
        if liftable && self.hovering && !self.expanded {
            if pill::on(self.st.settings.theme) {
                // Floating, it swells evenly about its middle and stays a capsule.
                pill::motion::hover_lift(g)
            } else {
                Geom::new(g.w + HOVER_LIFT.0, g.h + HOVER_LIFT.1, g.r + HOVER_LIFT.2)
            }
        } else {
            g
        }
    }

    // ------------------------------------------------------------ frame

    fn refresh_time(&mut self) {
        self.st.now = clock::mono();
        self.st.local = clock::local_shown();
        self.st.unix = clock::unix_now();
    }

    fn tick(&mut self) {
        let now = self.st.now;
        if let Some(h) = &self.st.hud {
            if now >= h.until {
                self.st.hud = None;
            }
        }
        if let Some(n) = &self.st.notice {
            if !self.expanded && now >= n.until {
                self.st.notice = None;
            }
        }
        if let Some(ended) = self.st.pomodoro.tick(now) {
            pages::focus::on_phase_ended(&mut self.st, ended);
        }
        self.prayer_tick();
        self.script_tick();
        self.notes_tick();
        self.sync_fullscreen_hud();
        // Prayer times or Notifications switched off: its tab is gone.
        let gone = match self.st.page {
            PageKind::Prayer => !self.st.prayer_on(),
            PageKind::Notifications => !self.st.notifications_on(),
            _ => false,
        };
        if gone {
            self.st.page = PageKind::Addons;
        }
    }

    /// Draw and show one frame. `tick` is the vblank that triggered it (None
    /// for self-test captures, which run off the clock).
    fn frame(&mut self, deferred: &mut Vec<Deferred>, tick: Option<FrameTick>) {
        if self.fullscreen_hidden {
            // Nothing is drawn, but timers, chimes and commands keep working.
            // Glass stops copying the screen: nobody sees the island.
            let off = RECT::default();
            self.glass.sync(false, self.hwnd, &self.sink, self.monitor.handle, off, &mut self.gfx);
            self.refresh_time();
            self.tick();
            self.run_commands(deferred);
            self.paint_settings_if_dirty(deferred);
            self.clock.set_active(self.settings_moving());
            self.motion = None;
            self.schedule_wake();
            return;
        }
        let rendered_at = clock::mono();
        self.refresh_time();
        // Stamp the frame with the vblank it will be shown after, so every
        // animation step is a whole number of refresh periods: even motion at
        // 60, 144 or 240 Hz, whatever the thread's wake-up jitter.
        let period = tick.map_or_else(|| self.clock.period(), |t| t.period);
        let anim_t = tick.map_or(rendered_at, |t| t.vblank + t.period).max(self.last_frame_t);
        let resumed = tick.is_some_and(|t| t.resumed) || self.last_frame_t == 0.0;
        // Coming out of rest, a motion advances one refresh, not the whole
        // time the clock was parked.
        let mut dt = if resumed { period } else { anim_t - self.last_frame_t };
        self.last_frame_t = anim_t;
        self.st.now = self.st.now.max(anim_t);
        self.tick();
        let now = self.st.now;

        // The pointer left the open island since the last frame: the close
        // starts on this frame, no timer in between.
        if std::mem::take(&mut self.collapse_due) && self.expanded && self.free_to_collapse() {
            self.collapse();
            self.left_at = clock::mono();
        }

        // A window under where the island rests: it rests out of the way.
        self.sync_aside();
        let target = self.target_mode();
        let mut retarget = false;
        if target != self.mode {
            let new_content = activities::content_key(&target) != activities::content_key(&self.mode);
            // A morph turned around mid-fade (the pointer came back while the
            // island closed): the content coming back is the one still fading
            // out, and it fades back in from where it is, not from nothing.
            let back = self
                .outgoing
                .filter(|o| new_content && activities::content_key(&o.0) == activities::content_key(&target))
                .map(|(_, t0, _, from)| from * outgoing_left(now - t0));
            if new_content {
                // The leaving content fades from how visible it is now.
                self.outgoing = Some((self.mode, now, self.island.target(), self.island.content_alpha()));
            }
            if let (Mode::Expanded(a), Mode::Expanded(b)) = (self.mode, target) {
                if a != b {
                    let dir = if pages::tab_pos(&self.st, b) > pages::tab_pos(&self.st, a) { 1.0 } else { -1.0 };
                    self.page_anim = Some((a, now, dir));
                }
            }
            self.motion_name = motion_name(&self.mode, &target);
            self.island.set_target(self.geom_for(&target), now, new_content);
            if let Some(a) = back {
                self.island.resume_content(a);
            }
            self.mode = target;
            retarget = true;
        } else {
            // Size can change within a mode (a page's height, the hover lift); follow it.
            let g = self.geom_for(&self.mode);
            if g != self.island.target() {
                if self.island.settled() {
                    self.motion_name = "resize";
                }
                self.island.set_target(g, now, false);
                retarget = true;
            }
        }
        // A motion that starts on this frame moves one refresh's worth, even
        // when the previous frame was a slower ambient one.
        if retarget {
            dt = dt.min(period);
        }
        let dt = dt.clamp(0.0, 0.05) as f32;
        // The Pill theme floats the island below the top edge.
        self.island.set_floating(pill::on(self.st.settings.theme).then_some(pill::TOP_GAP));
        self.island.step(dt, now);
        self.sync_room();
        self.ensure_room();
        self.sync_spot();
        self.place_island();
        self.note_motion(retarget, rendered_at, period, dt as f64);

        let scale = self.gfx.scale();
        let (sw, sh) = (self.surface.w, self.surface.h);
        let body = self.island.body_rect_at(self.cx);
        let canvas_r = self.island.canvas_rect_at(self.cx);
        let geom = self.island.current();
        let content_alpha = self.island.content_alpha();
        // Clear what the island covered last frame as well as this one.
        let canvas_px = px_rect(canvas_r, scale, 1, sw, sh);
        let full = RECT { left: 0, top: 0, right: sw, bottom: sh };
        let dirty = px_union(self.prev_canvas_px.unwrap_or(full), canvas_px);
        self.prev_canvas_px = Some(canvas_px);
        if std::mem::take(&mut self.capture_next) {
            self.capture_rect = px_rect(canvas_r, scale, 0, sw, sh);
            self.gfx.request_capture(self.capture_rect);
        }
        let mut hits = std::mem::take(&mut self.hits);
        hits.clear();

        // Stepping aside, the island flattens into the top edge and goes
        // invisible once there; coming back, it drops from the edge.
        // Making way for the pointer, the strip stays in sight: a thin bar
        // along the top edge, where the island went (it takes the mouse only
        // there either way).
        let making_way_strip = self.mode == Mode::Aside && self.making_way;
        let hidden_idle = self.rests_hidden() && !making_way_strip && (self.mode != Mode::Aside || self.island.settled());
        let floating = self.island.floating();
        // Software renderer: what of the floating island's hit area isn't
        // drawn (the gap above it) is painted at 1/255 (see hit_mask_for).
        let float_hit = if floating && !self.expanded { self.hit_mask().rect } else { None };
        // Content is masked to the island silhouette only while the outline
        // or the content moves. At rest every layout fits inside the body,
        // and skipping the mask layer keeps ambient frames cheap.
        let morphing = !self.island.settled() || self.outgoing.is_some();
        let mode = self.mode;
        let transition = self.page_anim.and_then(|(from, t0, dir)| {
            let t = ((now - t0) / PAGE_SWITCH) as f32;
            (t < 1.0).then_some(pages::PageTransition { from, t, dir })
        });
        if transition.is_none() {
            self.page_anim = None;
        }
        // Pages lay out at their final size, centred on the morphing outline,
        // so the header and the page body don't slide with the edges; the
        // outline reveals them.
        let lay = match mode {
            Mode::Expanded(_) => {
                let tgt = self.island.target();
                Rect::new(body.center().x - tgt.w / 2.0, body.y, tgt.w, tgt.h)
            }
            _ => body,
        };
        let mut wheel_left = 0.0;
        let draw_from = clock::mono();
        let mut drawn_at = draw_from;
        // Glass: copy the screen under the window while the GPU draws (the
        // self-test brings its own backdrop), then draw this frame's glass
        // before the frame starts. The look follows the glass, light or dark.
        let glass_theme = self.st.settings.theme == Theme::Glass;
        let window_px = RECT { left: self.surface.x, top: self.surface.y, right: self.surface.x + sw, bottom: self.surface.y + sh };
        // Only while someone can see it: not while hidden, locked or the display
        // is off. Not over a full-screen app either: copying a game's screen
        // every frame costs the game (Glass draws smoked glass there).
        let copy_on = glass_theme
            && self.selftest.is_none()
            && !hidden_idle
            && !self.locked
            && !self.display_off
            && !self.fullscreen_on
            && matches!(self.gfx.renderer(), crate::gfx::Renderer::Gpu { .. });
        self.glass.sync(copy_on, self.hwnd, &self.sink, self.monitor.handle, window_px, &mut self.gfx);
        self.sync_script_capture(self.st.page == PageKind::Script && (self.expanded || !self.island.settled()));
        self.sync_capture_setting();
        self.save_clipboard_if_changed();
        let mut glass_moving = false;
        let glass = if glass_theme && !hidden_idle {
            let small = !matches!(mode, Mode::Expanded(_));
            let (light, p, moving) = self.glass.frame(body, geom.r, SHOULDER, small, now, dt, scale, &mut self.gfx);
            glass_moving = moving;
            self.gfx.set_look(poni_core::look::glass(light));
            self.gfx.prepare_glass(&p, canvas_px);
            Some(p)
        } else {
            self.gfx.set_look(pill::look(&self.st, false));
            None
        };
        let (repaint, anim, focus, ui_live) = match self.gfx.begin(dirty) {
            Ok(mut canvas) => {
                if self.expanded && !canvas.on_gpu() {
                    // Near-invisible hit margin: a forgiving edge for the open
                    // panel. Collapsed, only the visible shape takes the mouse.
                    // (The GPU renderer keeps this in its hit mask instead.)
                    canvas.fill_rect(canvas_r, Color::black(1.0 / 255.0));
                } else if let Some(r) = float_hit.filter(|_| !canvas.on_gpu()) {
                    // The gap above the floating pill (or the Hidden style's
                    // band at the top edge) takes the mouse too.
                    canvas.fill_rect(r, Color::black(1.0 / 255.0));
                }
                if let Some(p) = glass.as_ref() {
                    canvas.glass(p, canvas_r);
                } else {
                    let island_color = if hidden_idle {
                        Color::black(1.0 / 255.0)
                    } else if crate::smoke::on(self.st.settings.theme) {
                        crate::smoke::ground(&self.st.settings, self.expanded)
                    } else {
                        palette::ISLAND
                    };
                    let border = if hidden_idle { None } else { Some(canvas.look().border) };
                    if floating {
                        // Hidden floats nothing: its band takes the mouse.
                        if !hidden_idle {
                            pill::shape::draw_body(&mut canvas, body, geom.r, island_color, border);
                        }
                    } else {
                        canvas.notch(body, geom.r, SHOULDER, island_color, border);
                    }
                }

                // Content scales about the top centre, where the island hangs
                // (floating, about the middle of the pill end of the morph).
                let pivot = if floating { pill::motion::pivot(body, self.island.target()) } else { Point::new(body.center().x, body.y) };
                if morphing {
                    canvas.clip(body);
                    if floating {
                        canvas.push_outline_mask(&pill::shape::segments(body, geom.r));
                    } else {
                        canvas.push_shape_mask(body, geom.r, SHOULDER);
                    }
                }

                // Outgoing content: laid out at its own (old) size, centred,
                // shrinking with the outline instead of being sliced, and
                // fading out on a smoothstep (it holds, then leaves). Drawn as
                // one layer, so it shrinks as a picture. Ghost input: no clicks.
                if let Some((m, t0, g, from)) = self.outgoing {
                    if now - t0 >= OUTGOING_FADE {
                        self.outgoing = None;
                    } else {
                        let old_body = Rect::new(body.center().x - g.w / 2.0, body.y, g.w, g.h);
                        let alpha = from * outgoing_left(now - t0);
                        let shrink = if floating { pill::motion::ghost_scale(geom, g, GHOST_MIN_SCALE) } else { (geom.w / g.w.max(1.0)).clamp(GHOST_MIN_SCALE, 1.0) };
                        let xf = scale_about(pivot, shrink);
                        let st = &mut self.st;
                        canvas.layered(old_body, alpha, xf, |c| {
                            let mut ghost_input = Input::default();
                            let mut ghost_hits = Vec::new();
                            draw_content(c, &mut ghost_input, &mut ghost_hits, st, m, old_body, old_body, None, now);
                        });
                    }
                }

                // Incoming content settles down from the bezel as it fades in,
                // as one layer, so its text doesn't re-hint every frame.
                let out = if content_alpha < 1.0 {
                    let grow = scale_about(pivot, CONTENT_SCALE_FROM + (1.0 - CONTENT_SCALE_FROM) * content_alpha);
                    let drop_by = if floating { pill::motion::CONTENT_DROP } else { CONTENT_DROP };
                    let xf = grow * Matrix3x2::translation(0.0, -drop_by * (1.0 - content_alpha));
                    let (input, st) = (&mut self.input, &mut self.st);
                    canvas.layered(union_rect(lay, body), content_alpha, xf, |c| draw_content(c, input, &mut hits, st, mode, lay, body, transition, now))
                } else {
                    draw_content(&mut canvas, &mut self.input, &mut hits, &mut self.st, mode, lay, body, transition, now)
                };
                wheel_left = self.input.wheel;
                if morphing {
                    canvas.pop_shape_mask();
                    canvas.unclip();
                }
                drop(canvas);
                if let Err(e) = self.gfx.end() {
                    pwarn!("EndDraw failed: {e}");
                }
                drawn_at = clock::mono();
                out
            }
            Err(e) => {
                pwarn!("begin frame failed: {e}");
                (false, false, None, false)
            }
        };
        self.hits = hits;

        // Unconsumed wheel over the header switches pages: one page per whole
        // notch, with a short cooldown so touchpads don't race through. Over
        // page content a wheel the page didn't use does nothing (a short list
        // must not jump pages where a long one would scroll).
        if let Mode::Expanded(p) = self.mode {
            let over_header = self.input.mouse.is_some_and(|m| m.y < body.y + pages::HEADER_H);
            if wheel_left != 0.0 && over_header {
                if now - self.wheel_t > 0.25 || wheel_left.signum() != self.wheel_acc.signum() {
                    self.wheel_acc = 0.0;
                }
                self.wheel_t = now;
                self.wheel_acc += wheel_left;
                if self.wheel_acc.abs() >= 1.0 && now - self.page_switch_t > 0.3 {
                    let next = pages::step_tab(p, &self.st, self.wheel_acc < 0.0);
                    self.st.cmds.push(Command::SetPage(next));
                    self.wheel_acc = 0.0;
                    self.page_switch_t = now;
                }
            }
        }
        self.input.end_frame();

        // Present. A renderer switch (GPU lost, or back) leaves the other
        // renderer's content in the window's bitmap: push the hit mask in
        // full, and redraw everything on the next frame too.
        let mut renderer_changed = self.gfx.take_renderer_changed();
        let (x, y) = (self.surface.x, self.surface.y);
        if let Err(e) = self.gfx.present(x, y) {
            pwarn!("present failed: {e}");
        }
        renderer_changed |= self.gfx.take_renderer_changed();
        if renderer_changed {
            self.mask = None;
            self.prev_canvas_px = None;
        }
        let presented_at = clock::mono();
        if self.gfx.needs_hit_mask() {
            self.update_hit_mask(now);
        }
        if let (Some(t), true) = (tick, self.motion.is_some() && self.motions_logged < 6 && self.trace.len() < 200) {
            let ms = |a: f64, b: f64| ((b - a) * 1000.0) as f32;
            self.trace.push((t.periods, [ms(t.vblank, draw_from), ms(draw_from, drawn_at), ms(drawn_at, presented_at), ms(presented_at, clock::mono())]));
        }
        if !self.shown {
            self.shown = true;
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                let _ = SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
            self.gfx.set_shown(true);
        }

        match focus {
            Some(true) => self.focus_text(),
            Some(false) => self.blur_text(),
            None => {}
        }
        self.run_commands(deferred);
        self.paint_settings_if_dirty(deferred);

        // Decide whether to keep the frame clock running. Real motion runs
        // at the display's full refresh rate; ambient motion runs on every
        // n-th refresh (the playing island at its Frame rate setting, a
        // drifting progress bar near 30) and pauses while the session is locked or the display off.
        let ambient_ok = !self.locked && !self.display_off;
        let eq_live = ambient_ok && activities::is_live(&self.mode, &self.st);
        let drift_live = ambient_ok && ui_live;
        self.glass_moving = glass_moving;
        let moving = !self.island.settled()
            || anim
            || repaint
            || renderer_changed
            || glass_moving
            || !self.st.cmds.is_empty()
            || self.outgoing.is_some()
            || self.page_anim.is_some()
            || self.settings_moving();
        // The display mode's rate, not the measured period: a divisor that
        // flips between 1 and 2 would be the uneven cadence it exists to avoid.
        let hz = self.clock.nominal_hz();
        self.clock.set_divisor(if moving {
            1
        } else if eq_live {
            // The playing island at the frame rate picked in Settings
            // (0: every refresh).
            match self.st.settings.music_fps {
                0 => 1,
                fps => ambient_divisor(hz, f64::from(fps)),
            }
        } else {
            ambient_divisor(hz, 30.0)
        });
        self.clock.set_active(moving || eq_live || drift_live);
        self.schedule_wake();
        if !self.log_queue.is_empty() {
            let lines = std::mem::take(&mut self.log_queue);
            std::thread::spawn(move || lines.iter().for_each(|l| crate::plog::write("INFO", l)));
        }
    }

    /// GPU renderer: keep the window's invisible hit mask on the island.
    /// While the outline moves it follows at [`MASK_INTERVAL`]; at rest it
    /// matches exactly.
    fn update_hit_mask(&mut self, now: f64) {
        let mask = self.hit_mask();
        let due = match &self.mask {
            None => true,
            Some((m, at)) => *m != mask && (self.island.settled() || now - at >= MASK_INTERVAL),
        };
        if !due {
            return;
        }
        let prev = self.mask.map(|(m, _)| m);
        match self.gfx.push_hit_mask(self.surface.x, self.surface.y, &mask, prev.as_ref()) {
            Ok(()) => self.mask = Some((mask, now)),
            Err(e) => {
                pwarn!("hit mask update failed: {e}");
                self.mask = None;
            }
        }
    }

    /// Track how smoothly the outline moves. The first morphs after start
    /// are logged (the CI smoke test reads them); after that only morphs
    /// that dropped frames, at most one a minute.
    fn note_motion(&mut self, retarget: bool, t: f64, period: f64, dt: f64) {
        let moving = !self.island.settled();
        if retarget || !moving {
            if let Some(mut m) = self.motion.take() {
                if !moving {
                    m.frame(t);
                }
                let early = self.motions_logged < 20;
                if m.frames >= 4 && (early || (m.hitches > 0 && t - self.hitch_logged_at > 60.0)) {
                    let dwm = match (self.dwm_at_start, dwm_counters()) {
                        (Some(a), Some(b)) => format!(", dwm {} refreshes / {} composed / {} missed", b.0.wrapping_sub(a.0), b.1.wrapping_sub(a.1), b.2.wrapping_sub(a.2)),
                        _ => String::new(),
                    };
                    self.log_queue.push(format!("{} ({}{dwm})", m.summary(), self.renderer_name()));
                    if self.motions_logged < 6 && !self.trace.is_empty() {
                        let col = |i: usize| self.trace.iter().map(|f| format!("{:.1}", f.1[i])).collect::<Vec<_>>().join(" ");
                        let periods: Vec<String> = self.trace.iter().map(|f| f.0.to_string()).collect();
                        self.log_queue.push(format!("  vblanks {}; late ms {}; draw ms {}; present ms {}; mask ms {}", periods.join(" "), col(0), col(1), col(2), col(3)));
                    }
                    self.motions_logged += 1;
                    if !early {
                        self.hitch_logged_at = t;
                    }
                }
            }
        }
        if moving {
            match self.motion.as_mut() {
                Some(m) => m.frame(t),
                None => {
                    self.motion = Some(MotionStats::begin(self.motion_name, t, period, dt));
                    self.trace.clear();
                    self.dwm_at_start = dwm_counters();
                }
            }
        }
    }

    fn renderer_name(&self) -> &'static str {
        match self.gfx.renderer() {
            crate::gfx::Renderer::Gpu { warp: false } => "gpu",
            crate::gfx::Renderer::Gpu { warp: true } => "gpu-warp",
            crate::gfx::Renderer::Software => "software",
        }
    }

    /// Wake up for the next visible change (HUD expiry, countdown second, …).
    fn schedule_wake(&mut self) {
        let now = self.st.now;
        let mut next: Option<f64> = None;
        let mut consider = |t: f64| {
            if t > now {
                next = Some(next.map_or(t, |n: f64| n.min(t)));
            }
        };
        if let Some(h) = &self.st.hud {
            consider(h.until);
        }
        if let Some(n) = &self.st.notice {
            consider(n.until);
        }
        // While hidden for a full-screen app only the phase end matters.
        let visible = !self.fullscreen_hidden;
        if let Some(b) = self.st.pomodoro.next_second_boundary(now).filter(|_| visible) {
            consider(now + b + 0.005);
        }
        if let poni_core::pomodoro::RunState::Running { ends_at } = self.st.pomodoro.run {
            consider(ends_at + 0.005);
        }
        // The music pill wakes for every sung line (in every theme), and Pill's
        // pills for their own changes; nobody sees those while the session is
        // locked or the display is off, so they rest then (the lock and
        // display events ask for a frame, which picks them up again).
        let singing = self.mode == Mode::Activity(activities::Activity::Media);
        let watched = visible && !((pill::on(self.st.settings.theme) || singing) && (self.locked || self.display_off));
        if let Some(t) = activities::next_wake(&self.mode, &self.st).filter(|_| watched) {
            consider(t);
        }
        if let Mode::Expanded(p) = self.mode {
            // A page's own changes rest while nobody is at the PC (the Prayer
            // page's countdown ticks every second); the lock and display
            // events ask for a frame, which picks them up again.
            let away = self.locked || self.display_off;
            if let Some(t) = pages::next_wake(p, &self.st).filter(|_| visible && !away) {
                consider(t + 0.005);
            }
        }
        if let Some(t) = next {
            let ms = (((t - now) * 1000.0).ceil() as u32).clamp(10, 3_600_000);
            unsafe {
                SetTimer(self.hwnd, TIMER_WAKE, ms, None);
            }
        }
    }

    fn schedule_minute_timer(&mut self) {
        let t = clock::local_now();
        let ms = (60 - t.second as u32).max(1) * 1000 + 20;
        unsafe {
            SetTimer(self.hwnd, TIMER_MINUTE, ms, None);
        }
    }

    /// Run a notice's inline action in place (the island stays collapsed),
    /// then show a check mark in the pill briefly before the notice leaves.
    fn run_inline_action(&mut self, a: InlineAction) {
        self.refresh_time();
        if a == InlineAction::Open {
            // Another app's notification: that app opens and the notice goes.
            if let Some(Notice { action: Some(NoticeAction::OpenApp(id)), .. }) = self.st.notice.take() {
                shell::open_app(&id);
            }
            return;
        }
        if a == InlineAction::StopAdzan {
            self.adzan.stop();
            self.st.prayer.adzan = false;
        }
        if a == InlineAction::Hide {
            // Gone at once, the way a notice leaves when its time is up.
            if let Some(n) = self.st.notice.as_mut() {
                n.until = self.st.now;
            }
            return;
        }
        let st = &mut self.st;
        // Already done: the check mark is showing.
        if st.notice.as_ref().is_none_or(|n| n.icon == Icon::Check) {
            return;
        }
        if a == InlineAction::ShowAmount {
            // The sum and sender take the subtitle's place for a few seconds; the
            // pill turns into a check (activities.rs inline_done).
            let until = st.now + crate::state::SHOWN_S;
            if let Some(n) = st.notice.as_mut() {
                if let Some(text) = n.reveal.take() {
                    n.subtitle = text;
                    n.until = n.until.max(until);
                }
            }
            return;
        }
        match a {
            InlineAction::StartFocus => st.pomodoro.start(st.now),
            InlineAction::ShowAmount | InlineAction::StopAdzan | InlineAction::Hide | InlineAction::Open => {}
        }
        let until = st.now + INLINE_DONE_S;
        if let Some(n) = st.notice.as_mut() {
            n.icon = Icon::Check;
            n.until = until;
        }
        pinfo!("notice: inline action {a:?} done");
    }

    // ------------------------------------------------------------ prayer times

    /// Prayer times' files are in place: read its places, settle the place,
    /// and ask where the PC is when following the location.
    fn prayer_ready(&mut self) {
        let a = &poni_core::addons::PRAYER_TIMES;
        let path = paths::addons_dir().join(a.id).join(a.tag).join("places.json");
        match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| PlaceList::parse(&t)) {
            Ok(list) => {
                pinfo!("prayer: {} places", list.places.len());
                self.st.prayer.places = Some(std::sync::Arc::new(list));
            }
            Err(e) => {
                pwarn!("prayer: can't read the places ({e})");
                return;
            }
        }
        self.resolve_prayer_place();
        if self.st.settings.prayer.follow_location {
            self.locate();
        }
    }

    /// Prayer times switched off: nothing plays, nothing is kept.
    fn prayer_off(&mut self) {
        self.adzan.stop();
        self.st.prayer = PrayerState::default();
    }

    /// The place the times are for: the chosen one, or until there is one,
    /// the one the PC's time zone suggests. A place from the world list is
    /// kept as it is once settled; at start it comes from the lines settings
    /// keep of it (nothing online).
    fn resolve_prayer_place(&mut self) {
        let Some(list) = self.st.prayer.places.clone() else { return };
        let chosen = match self.st.settings.prayer.place.as_deref() {
            Some(id) if self.st.prayer.place.as_ref().is_some_and(|p| p.id == id) => self.st.prayer.place.clone(),
            Some(id) => poni_core::prayer::find(&list, self.st.settings.prayer.world_place.as_deref(), id),
            None => None,
        };
        let suggested = chosen.is_none();
        let place = chosen.or_else(|| poni_core::prayer::suggested_place_id(self.utc_offset_min()).and_then(|id| list.find(id)).cloned());
        let changed = place.as_ref().map(|p| &p.id) != self.st.prayer.place.as_ref().map(|p| &p.id);
        self.st.prayer.place = place;
        self.st.prayer.suggested = suggested;
        if changed {
            pinfo!("prayer: times for {}", self.st.prayer.place.as_ref().map_or("no place", |p| p.id.as_str()));
            // A new place: prayer times that passed there don't ring now.
            self.st.prayer.handled_until = Some(self.st.prayer_now());
        }
    }

    /// The PC's UTC offset in minutes, from its clock.
    fn utc_offset_min(&self) -> i32 {
        let l = &self.st.local;
        let local = poni_core::datetime::days_from_civil(l.year, l.month, l.day) * 86_400 + l.hour as i64 * 3600 + l.minute as i64 * 60 + l.second as i64;
        (((local - self.st.unix) as f64 / 900.0).round() * 15.0) as i32
    }

    /// Ask Windows where the PC is (the answer: `AppEvent::Located`).
    fn locate(&mut self) {
        if self.st.prayer.locate == Locate::Asking || self.selftest.is_some() {
            return;
        }
        self.st.prayer.locate = Locate::Asking;
        self.st.prayer.located_at = Some(self.st.unix);
        services::location::locate(self.sink.clone());
    }

    /// The nearest listed place becomes the place, while following the
    /// location (`pages::prayer::located`). The point is never saved.
    fn on_located(&mut self, found: services::location::Located) {
        use services::location::Located;
        match found {
            Located::Denied => self.st.prayer.locate = Locate::Denied,
            Located::Unknown => self.st.prayer.locate = Locate::Unknown,
            // Kemenag's places, or the world's cities around it (looked up online).
            Located::At(lat, lng) => pages::prayer::located(&mut self.st, lat, lng),
        }
    }

    /// Each frame (and each minute): the prayer times that just came, and
    /// asking where the PC is again every 3 hours while following it.
    fn prayer_tick(&mut self) {
        if !self.st.prayer_on() {
            return;
        }
        if self.st.settings.prayer.follow_location && self.st.prayer.located_at.is_none_or(|t| self.st.unix - t >= 3 * 3600) {
            self.locate();
        }
        let Some(site) = self.st.prayer_site() else { return };
        let now = self.st.prayer_now();
        let Some(from) = self.st.prayer.handled_until else {
            self.st.prayer.handled_until = Some(now);
            return;
        };
        if now <= from {
            return;
        }
        self.st.prayer.handled_until = Some(now);
        for m in poni_core::prayer::prayers_between(&site, from, now) {
            match poni_core::prayer::due(m.at, now) {
                Due::Ring => self.prayer_moment(m, true),
                Due::Quiet => self.prayer_moment(m, false),
                Due::Skip => {}
            }
        }
    }

    /// A prayer's time came: the island says so and plays the chosen sound.
    /// Quiet when it's late (after a sleep) or a full-screen app hides the island.
    fn prayer_moment(&mut self, m: PrayerMoment, ring: bool) {
        let Some(site) = self.st.prayer_site() else { return };
        let place = self.st.prayer.place.as_ref().map_or(String::new(), |p| p.label.clone());
        let quiet = !ring || self.fullscreen_hidden;
        let sound = if quiet { PrayerSound::Silent } else { self.st.settings.prayer.sound };
        let mut adzan = false;
        match sound {
            // The recording isn't Subuh's adzan (no "ash-shalatu khairum minan naum"): Subuh gets the tone.
            PrayerSound::Adzan if m.prayer != Prayer::Subuh => {
                adzan = self.play_adzan();
                if !adzan {
                    self.st.cmd(Command::Play(crate::commands::Sound::Chime));
                }
            }
            PrayerSound::Adzan | PrayerSound::Tone => self.st.cmd(Command::Play(crate::commands::Sound::Chime)),
            PrayerSound::Silent => {}
        }
        let title = pages::prayer::moment_title(m.prayer);
        let subtitle = format!("{} \u{b7} {place}", site.hhmm(m.at));
        let secs = if adzan { 200.0 } else { 60.0 };
        self.st.notify(Icon::Moon, self.st.accent, title, subtitle, secs, Some(NoticeAction::OpenPage(PageKind::Prayer)));
        self.st.ui.glow_until = self.st.now + activities::GLOW_S;
        if adzan {
            self.st.set_notice_inline(InlineAction::StopAdzan);
        }
        pinfo!("prayer: {} at {}{}", m.prayer.name(), site.hhmm(m.at), if quiet { ", quietly" } else { "" });
    }

    /// Start the adzan; music that plays pauses (and stays paused).
    fn play_adzan(&mut self) -> bool {
        let a = &poni_core::addons::PRAYER_TIMES;
        let path = paths::addons_dir().join(a.id).join(a.tag).join("adzan.m4a");
        match self.adzan.play(&path, &self.sink) {
            Ok(()) => {
                self.st.prayer.adzan = true;
                if self.st.settings.prayer.pause_music && self.st.media.as_ref().is_some_and(|m| m.is_playing()) {
                    self.st.media_cmd(MediaCmd::PlayPause);
                }
                true
            }
            Err(e) => {
                pwarn!("prayer: the adzan didn't play ({e}); the tone instead");
                false
            }
        }
    }

    /// OK on the countdown before a prayer: the closed island goes back to
    /// what it would show (the song and its sung line) until the prayer's
    /// time. Its moment still comes then: `prayer_tick` doesn't look at the
    /// countdown.
    fn prayer_countdown_ok(&mut self) {
        self.refresh_time();
        if let (Some(m), Some(site)) = (self.st.prayer_countdown_ok(), self.st.prayer_site()) {
            pinfo!("prayer: countdown put away until {} at {}", m.prayer.name(), site.hhmm(m.at));
        }
    }

    fn adzan_ended(&mut self) {
        self.adzan.stop();
        self.st.prayer.adzan = false;
        // The notice leaves soon after the adzan.
        let soon = self.st.now + 4.0;
        if let Some(n) = self.st.notice.as_mut().filter(|n| n.inline == Some(InlineAction::StopAdzan)) {
            n.until = n.until.min(soon);
        }
    }

    fn set_page(&mut self, p: PageKind) {
        if p != self.st.page {
            self.st.page = p;
            // Settings is never where the island opens next time.
            if p != PageKind::Settings {
                self.st.store.last_page = p.key().to_string();
                unsafe {
                    SetTimer(self.hwnd, TIMER_SAVE, 800, None);
                }
            } else {
                crate::autostart::report(self.sink.clone(), None);
            }
            if self.text_focus {
                self.blur_text();
            }
            if p == PageKind::Notifications {
                // Looked at: the badge goes.
                self.st.recent_unseen = 0;
            }
            if p == PageKind::Notes {
                self.st.notes_saved = services::notes::saved(&paths::notes_dir(), 20);
            }
        }
    }

    fn run_commands(&mut self, deferred: &mut Vec<Deferred>) {
        let cmds = std::mem::take(&mut self.st.cmds);
        for c in cmds {
            match c {
                Command::ScreenIcons(list) => self.screen_icons(list),
                Command::CopyText(t) => {
                    self.own_clip = Some(t.clone());
                    services::clipboard::set_text(self.helper, &t);
                }
                Command::Media(m) => {
                    if let Some(s) = self.services.as_ref() {
                        s.media_cmd(m.clone());
                    }
                    // Optimistic UI for play/pause.
                    if m == MediaCmd::PlayPause {
                        if let Some(ms) = self.st.media.as_mut() {
                            let now = self.st.now;
                            let pos = ms.timeline.position_at(now);
                            ms.timeline.position_s = pos;
                            ms.timeline.sampled_at = now;
                            ms.timeline.playing = !ms.timeline.playing;
                            // The player's confirmation will match this, so
                            // start (or end) the pause grace period here.
                            self.st.media_paused_at = (!ms.timeline.playing).then_some(now);
                        }
                    }
                }
                Command::OpenPath(p) => shell::open(&p),
                Command::RevealPath(p) => shell::reveal(&p),
                Command::DragOut(paths) => deferred.push(Deferred::DragOut(paths)),
                Command::LoadThumb(p) => {
                    if !self.st.shelf_thumbs.contains_key(&p) {
                        self.st.shelf_thumbs.insert(p.clone(), None);
                        services::imaging::load_thumb_async(p, self.sink.clone());
                    }
                }
                Command::FocusText => self.focus_text(),
                Command::BlurText => self.blur_text(),
                Command::ScriptPaste => {
                    // Read here, not through the text field path: that one drops line breaks.
                    let text = services::clipboard::read_text(self.hwnd).map(|t| poni_core::script::clean(&t)).filter(|t| !t.is_empty());
                    match text {
                        Some(text) => pages::script::set_text(&mut self.st, text),
                        None => pages::script::note(&mut self.st, "Copy your script first"),
                    }
                }
                Command::SaveSettings => {
                    let before = self.saved_settings.clone();
                    self.settings_changed(&before);
                    self.save_settings();
                }
                Command::OpenSettings => self.show_settings(deferred),
                Command::SaveStore => {
                    unsafe {
                        SetTimer(self.hwnd, TIMER_SAVE, 800, None);
                    }
                    // An event added or gone: the next reminder may have moved.
                    self.agenda_schedule();
                }
                Command::SetPage(p) => self.set_page(p),
                Command::SetPinned(b) => {
                    self.st.pinned = b;
                    if !b {
                        self.maybe_schedule_collapse();
                    }
                }
                Command::Play(s) => services::sound::play(s),
                Command::OpenUrl(u) => shell::open(&u),
                Command::SetAutostart(on) => {
                    self.st.ui.autostart = Some(on);
                    crate::autostart::report(self.sink.clone(), Some(on));
                }
                Command::Phone(c) => self.phone_cmd(c),
                Command::Pair(c) => self.pair_cmd(c),
                // The self-test's pictures use the files it seeds (none from the website).
                Command::GetCities(file) => {
                    if self.selftest.is_none() {
                        services::cities::fetch(file, self.sink.clone());
                    }
                }
                Command::Notify(c) => self.notify_cmd(c),
                Command::Notes(c) => self.notes_cmd(c),
                Command::PickMic => self.pick_mic(),
                Command::PickStartPage => self.pick_start_page(),
                Command::BackupExport => self.backup_export(),
                Command::BackupImport => self.backup_import(),
                Command::OpenApp(id) => {
                    shell::open_app(&id);
                    self.collapse();
                }
                Command::Menu(c) => self.on_menu(c),
            }
        }
        self.sync_phone_offer();
        self.sync_pair_offer();
    }

    // ------------------------------------------------------------ notifications

    /// Notifications came on (`ask`: by its chip in Settings' tabs row, so
    /// Windows' question comes now) or the app started with it on.
    fn start_notify(&mut self, ask: bool) {
        self.st.notify_access = NotifyAccess::Unknown;
        if self.notify.is_none() {
            self.notify = services::notifications::Listener::start(self.sink.clone(), ask);
        }
    }

    /// Off: stop listening (dropping the listener stops it), and Windows'
    /// pop-ups come back.
    fn stop_notify(&mut self) {
        self.notify = None;
        self.st.notify_access = NotifyAccess::Unknown;
        self.sync_banners();
    }

    /// Now, in minutes from 1970-01-01 local time (the calendar's clock).
    fn local_minute(&self) -> i64 {
        let t = self.st.local;
        poni_core::datetime::days_from_civil(t.year, t.month, t.day) * 1440 + i64::from(t.hour) * 60 + i64::from(t.minute)
    }

    /// Set the one-shot timer for the next event reminder, if there is one.
    fn agenda_schedule(&mut self) {
        unsafe {
            let _ = KillTimer(self.hwnd, TIMER_AGENDA);
        }
        let Some(at) = poni_core::agenda::next_reminder(&self.st.store.agenda, &self.st.store.tasks, self.agenda_done) else { return };
        let now = self.local_minute();
        let wait_ms = ((at - now) * 60 - i64::from(self.st.local.second)) * 1000 + 500;
        unsafe {
            SetTimer(self.hwnd, TIMER_AGENDA, wait_ms.clamp(1000, AGENDA_MAX_WAIT_MS) as u32, None);
        }
    }

    /// The reminders due by now, as notices; then the timer for the next.
    fn agenda_remind(&mut self) {
        self.st.local = clock::local_shown();
        let now = self.local_minute();
        let due: Vec<(String, String)> = poni_core::agenda::reminders(&self.st.store.agenda, &self.st.store.tasks, self.agenda_done, now)
            .into_iter()
            .map(|e| {
                let line = match (e.time, e.remind) {
                    (None, _) => "Today".to_string(),
                    (Some(t), Some(0)) | (Some(t), None) => format!("Now \u{b7} {}", poni_core::agenda::fmt_time(t)),
                    (Some(t), Some(m)) if m % 60 == 0 => format!("In {} h \u{b7} {}", m / 60, poni_core::agenda::fmt_time(t)),
                    (Some(t), Some(m)) => format!("In {m} min \u{b7} {}", poni_core::agenda::fmt_time(t)),
                };
                let line = if e.task { format!("Task \u{b7} {line}") } else { line };
                (e.title, line)
            })
            .collect();
        self.agenda_done = now;
        for (title, line) in due {
            pinfo!("agenda: a reminder");
            self.st.notify(Icon::Calendar, self.st.accent, title, line, 12.0, Some(crate::state::NoticeAction::OpenPage(PageKind::Today)));
            self.st.chime();
        }
        self.request_frame();
        self.agenda_schedule();
    }

    /// Windows' brightness pop-up out of sight, now and a few times in the
    /// second after, while the island shows brightness and it's set to.
    fn hide_windows_osd(&mut self) {
        let s = &self.st.settings;
        if !((s.hud_brightness || s.hud_volume) && s.hud_hide_windows_brightness) || self.selftest.is_some() {
            return;
        }
        services::osd::hide();
        self.osd_looks = OSD_LOOKS;
        unsafe {
            SetTimer(self.hwnd, TIMER_OSD, OSD_EVERY_MS, None);
        }
    }

    /// The keyboard hook for the pop-ups' keys: there while one of them is
    /// on; and the camera's watcher, while its pop-up is.
    fn sync_keys(&mut self) {
        if self.st.settings.hud_camera != self.camera.is_some() {
            self.camera = if self.st.settings.hud_camera { services::camera::Camera::start(self.sink.clone()) } else { None };
        }
        let s = &self.st.settings;
        services::keys::set(self.helper, s.hud_volume_keys, s.hud_locks, s.hud_language);
        if s.hud_language && self.keyboard_layout.is_none() {
            self.keyboard_layout = services::keys::language().map(|l| l.0);
        }
    }

    /// A key from the hook: a volume key to act on, a lock key just
    /// pressed (`on`: on now), or a language switch to look at in a moment.
    fn on_key(&mut self, vk: usize, on: bool) {
        use windows::Win32::UI::Input::KeyboardAndMouse::{VK_CAPITAL, VK_NUMLOCK, VK_SCROLL};
        if vk == services::keys::LANGUAGE {
            // The island's window: its timers are the ones the app hears.
            self.language_looks = LANGUAGE_LOOKS;
            unsafe {
                let _ = SetTimer(self.hwnd, TIMER_LANGUAGE, LANGUAGE_WAIT_MS, None);
            }
            return;
        }
        let key = match vk as u16 {
            v if v == VK_CAPITAL.0 => "Caps Lock",
            v if v == VK_NUMLOCK.0 => "Num Lock",
            v if v == VK_SCROLL.0 => "Scroll Lock",
            v => {
                services::audio::volume_key(v);
                return;
            }
        };
        if self.st.settings.hud_locks {
            self.st.show_hud(HudKind::Lock { key, on }, 1.2);
            self.sync_fullscreen_hud();
            self.request_frame();
        }
    }

    /// Windows' pop-ups hidden for exactly the apps the island shows, while
    /// it can read them and Hide Windows' pop-ups is on; back for the rest.
    fn sync_banners(&mut self) {
        let n = &self.st.settings.notifications;
        let want: std::collections::BTreeSet<String> = if self.st.notifications_on() && n.hide_banners && self.st.notify_access == NotifyAccess::Allowed && self.selftest.is_none() {
            let mut want: std::collections::BTreeSet<String> = n.shown().map(str::to_string).collect();
            // Every app: also those Windows knows that haven't sent one since
            // Notifications came on, so their next pop-up is the island's too.
            if n.every_app {
                want.extend(services::banners::known_apps());
            }
            want.retain(|id| !services::banners::keeps_its_popup(id));
            want
        } else {
            Default::default()
        };
        self.banners.set(&want);
    }

    fn notify_cmd(&mut self, c: NotifyCmd) {
        match c {
            NotifyCmd::Ask => {
                if let Some(l) = self.notify.as_mut() {
                    l.ask();
                }
            }
            NotifyCmd::Check => {
                if let Some(l) = self.notify.as_mut() {
                    l.check();
                }
            }
            NotifyCmd::OpenWindowsSettings => shell::open_notification_settings(),
        }
    }

    fn on_notify(&mut self, ev: NotifyEvent) {
        if !self.st.notifications_on() {
            return;
        }
        match ev {
            NotifyEvent::Access(a) => {
                self.st.notify_access = a;
                if let Some(l) = self.notify.as_mut() {
                    l.follow(a);
                }
                self.sync_banners();
            }
            NotifyEvent::Apps(apps) => {
                let mut changed = false;
                for (id, name) in apps {
                    changed |= self.st.settings.notifications.remember(&id, &name).1;
                }
                if changed {
                    self.st.cmd(Command::SaveSettings);
                }
                self.sync_banners();
            }
            NotifyEvent::Photo(id, img) => {
                if id == self.st.notice_card_id {
                    self.st.notice_photo = Some(img);
                    self.request_frame();
                }
            }
            NotifyEvent::Logo(app_id, img) => {
                self.st.notify_logos.insert(app_id, img);
                self.request_frame();
            }
            NotifyEvent::Seen(seen) => {
                let (_, changed) = self.st.settings.notifications.remember(&seen.app_id, &seen.app);
                let shows = self.st.settings.notifications.shows(&seen.app_id);
                if changed {
                    self.st.cmd(Command::SaveSettings);
                    // A new app: its next pop-ups are the island's.
                    self.sync_banners();
                }
                // Nobody would see it: Windows keeps it in its own list.
                if !shows || self.locked || self.display_off {
                    return;
                }
                if let Some(n) = pages::notifications::message_notice(&seen, &self.st.settings.notifications, self.st.accent, self.st.now) {
                    // Kept for the recent list, and counted until it's looked at.
                    let title = n.title.clone();
                    self.st.recent.push_front(crate::state::Recent { app_id: seen.app_id.clone(), app: seen.app.clone(), title, text: n.subtitle.clone(), at: self.st.unix });
                    self.st.recent.truncate(pages::notifications::KEEP);
                    if !(self.expanded && self.st.page == PageKind::Notifications) {
                        self.st.recent_unseen = self.st.recent_unseen.saturating_add(1);
                    }
                    // Its sender's picture comes a moment later, if it has one.
                    self.st.notice_card_id = seen.id;
                    self.st.notice_photo = None;
                    self.st.show_or_hold(n, self.fullscreen_hidden);
                }
            }
        }
    }

    // ------------------------------------------------------------ phone drop

    /// The QR code waits for a phone, or a phone waits for an answer (Shelf
    /// page only), or a reading is under way (Script page only).
    fn phone_holds(&self) -> bool {
        self.st.page_holds_island()
    }

    fn phone_cmd(&mut self, c: PhoneCmd) {
        match c {
            PhoneCmd::Open => self.open_phone(),
            PhoneCmd::Close => self.close_phone(),
            PhoneCmd::Answer { id, accept } => {
                if let Some(svc) = &self.phone {
                    svc.answer(id, accept);
                }
                let until = self.st.now + pages::shelf::PHONE_OUTCOME_S;
                if let Some(pd) = self.st.phone.as_mut() {
                    if let Some(a) = pd.ask.take_if(|a| a.id == id) {
                        if accept {
                            pd.transfer = Some(PhoneTransfer { id, from: a.from, file: 0, files: a.files.len(), done: 0, total: a.total });
                        } else {
                            pd.outcome = Some((format!("Declined {} from {}", files_label(a.files.len()), a.from), false, until));
                        }
                    }
                }
                self.maybe_schedule_collapse();
            }
        }
    }

    fn open_phone(&mut self) {
        if let Some(old) = self.phone.take() {
            old.stop();
        }
        let mut pd = PhoneDrop::default();
        if self.selftest.is_some() {
            pd.error = Some("Phone drop is off in the self-test.".into());
        } else {
            let offer = self.st.store.shelf.paths();
            match services::phonedrop::PhoneDrop::start(self.sink.clone(), offer.iter().map(PathBuf::from).collect()) {
                Ok((svc, url)) => {
                    pd.qr = QrMatrix::encode(&url);
                    pd.url = url;
                    self.phone_offer = offer;
                    self.phone = Some(svc);
                }
                Err(e) => {
                    pwarn!("phone drop: {e}");
                    pd.error = Some(e);
                }
            }
        }
        self.st.phone = Some(pd);
        self.st.page = PageKind::Shelf;
        self.expand();
    }

    fn close_phone(&mut self) {
        if let Some(svc) = self.phone.take() {
            svc.stop();
        }
        if matches!(self.st.phone.as_ref().map(|p| &p.view), Some(PhoneView::Pairing { .. })) {
            if let Some(s) = &self.services {
                s.pair_cancel();
            }
        }
        self.st.phone = None;
        self.phone_offer.clear();
        self.maybe_schedule_collapse();
        self.request_frame();
    }

    /// Minute tick: nobody has used the link for a while, so close it.
    fn close_idle_phone(&mut self) {
        let idle = self.phone.as_ref().is_some_and(|svc| !svc.busy() && svc.idle_secs() >= PHONE_IDLE_S);
        let quiet = self.st.phone.as_ref().is_some_and(|p| p.ask.is_none() && p.transfer.is_none());
        if idle && quiet {
            pinfo!("phone drop: closed after {:.0} minutes without a phone", PHONE_IDLE_S / 60.0);
            self.close_phone();
        }
    }

    /// Keep the files a phone can save in step with the Shelf.
    fn sync_phone_offer(&mut self) {
        let Some(svc) = &self.phone else { return };
        let paths = self.st.store.shelf.paths();
        if paths != self.phone_offer {
            svc.set_offer(paths.iter().map(PathBuf::from).collect());
            self.phone_offer = paths;
        }
    }

    fn on_phone(&mut self, e: PhoneEvent) {
        let now = self.st.now;
        let until = now + pages::shelf::PHONE_OUTCOME_S;
        let Some(pd) = self.st.phone.as_mut() else { return };
        match e {
            PhoneEvent::Visit { from } => {
                pd.visitor = Some(from);
                self.maybe_schedule_collapse();
            }
            PhoneEvent::Ask(a) => {
                pd.outcome = None;
                pd.visitor.get_or_insert_with(|| a.from.clone());
                let files: Vec<(String, u64)> = a.files.iter().map(|f| (f.name.clone(), f.size)).collect();
                pinfo!("phone drop: {} asks to send {}", a.from, files_label(files.len()));
                pd.ask = Some(PhoneAsk { id: a.id, from: a.from.clone(), total: a.total(), files });
                self.st.page = PageKind::Shelf;
                self.expand();
                self.st.chime();
            }
            PhoneEvent::AskEnded { id } => {
                if let Some(a) = pd.ask.take_if(|a| a.id == id) {
                    pd.outcome = Some((format!("{} stopped waiting", a.from), false, until));
                }
                self.maybe_schedule_collapse();
            }
            PhoneEvent::Progress { id, file, files, done, total } => match pd.transfer.as_mut() {
                Some(t) if t.id == id => {
                    t.file = file;
                    t.files = files;
                    t.done = done;
                    t.total = total;
                }
                _ => {
                    let from = pd.visitor.clone().unwrap_or_else(|| "Phone".into());
                    pd.transfer = Some(PhoneTransfer { id, from, file, files, done, total });
                }
            },
            PhoneEvent::Received { from, paths, .. } => {
                pd.transfer = None;
                let n = paths.len();
                let title = format!("{} from {from} {} on the Shelf", files_label(n), if n == 1 { "is" } else { "are" });
                pd.outcome = Some((title.clone(), true, until));
                let added: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
                self.st.store.shelf.add_paths(&added, self.st.unix);
                for p in added {
                    self.st.cmds.push(Command::LoadThumb(p));
                }
                self.st.save();
                pinfo!("phone drop: {} from {from}", files_label(n));
                if !self.expanded {
                    self.st.notify(Icon::Phone, palette::GREEN, title, "Open the Shelf to see them", 6.0, Some(NoticeAction::OpenPage(PageKind::Shelf)));
                }
                self.st.chime();
                self.maybe_schedule_collapse();
            }
            PhoneEvent::Failed { from, reason, detail, .. } => {
                pd.transfer = None;
                pd.outcome = Some((format!("Nothing kept from {from}. {reason}."), false, until));
                pwarn!("phone drop: transfer from {from} failed: {reason} {detail}");
                self.maybe_schedule_collapse();
            }
        }
    }

    // ------------------------------------------------------------ paired phones

    fn pair_cmd(&mut self, c: PairCmd) {
        let now = self.st.now;
        match c {
            PairCmd::Begin => {
                let r = match &self.services {
                    Some(s) => s.pair_begin(),
                    None => Err("Pairing is off in the self-test.".into()),
                };
                let Some(pd) = self.st.phone.as_mut() else { return };
                pd.outcome = None;
                match r {
                    Ok(url) => {
                        pinfo!("pair drop: showing a pairing code");
                        pd.view = PhoneView::Pairing { qr: QrMatrix::encode(&url), url, until: now + poni_core::pairdrop::meet::PAIRING_FOR.as_secs_f64() };
                    }
                    Err(e) => {
                        pwarn!("pair drop: {e}");
                        pd.outcome = Some((e, false, now + pages::shelf::PHONE_OUTCOME_S));
                    }
                }
            }
            PairCmd::Back => {
                if matches!(self.st.phone.as_ref().map(|p| &p.view), Some(PhoneView::Pairing { .. })) {
                    if let Some(s) = &self.services {
                        s.pair_cancel();
                    }
                }
                if let Some(pd) = self.st.phone.as_mut() {
                    pd.view = PhoneView::Lan;
                }
            }
            PairCmd::List => {
                if matches!(self.st.phone.as_ref().map(|p| &p.view), Some(PhoneView::Pairing { .. })) {
                    if let Some(s) = &self.services {
                        s.pair_cancel();
                    }
                }
                if let Some(pd) = self.st.phone.as_mut() {
                    pd.view = PhoneView::Paired;
                }
            }
            PairCmd::Remove(room) => {
                if let Some(s) = &self.services {
                    s.pair_remove(&room);
                }
                if let Some(i) = self.st.paired.iter().position(|p| p.room == room) {
                    let p = self.st.paired.remove(i);
                    pinfo!("pair drop: removed a paired phone");
                    if let Some(pd) = self.st.phone.as_mut() {
                        pd.outcome = Some((format!("{} can't send to this PC any more", p.name), false, now + pages::shelf::PHONE_OUTCOME_S));
                        pd.outcome_line = Some("Pair it again any time from Pair a phone.".into());
                    }
                }
            }
        }
        self.request_frame();
    }

    /// Keep the files paired phones may save in step with the Shelf.
    fn sync_pair_offer(&mut self) {
        let Some(svc) = &self.services else { return };
        let paths = self.st.store.shelf.paths();
        if self.pair_offer.as_ref() != Some(&paths) {
            svc.pair_shelf(paths.iter().map(PathBuf::from).collect());
            self.pair_offer = Some(paths);
        }
    }

    fn on_paired(&mut self, e: MeetEvent) {
        let now = self.st.now;
        let until = now + pages::shelf::PHONE_OUTCOME_S;
        match e {
            MeetEvent::Pairs(list) => self.st.paired = paired_phones(list),
            MeetEvent::Server { reachable } => {
                if self.st.pair_server.is_some() || !reachable {
                    pinfo!("pair drop: meeting server {}", if reachable { "reachable again" } else { "unreachable" });
                }
                self.st.pair_server = Some(reachable);
            }
            MeetEvent::Paired { name, .. } => {
                pinfo!("pair drop: a phone finished pairing");
                if let Some(pd) = self.st.phone.as_mut() {
                    pd.view = PhoneView::Lan;
                    pd.outcome = Some((format!("{name} is paired"), true, until));
                    pd.outcome_line = Some("It can now send from any network, even with this panel closed.".into());
                }
                if !self.hovering {
                    // The island closes now that nothing waits: say it on the pill.
                    self.st.notify(Icon::Phone, palette::GREEN, format!("{name} is paired"), "It can send from any network now", 6.0, Some(NoticeAction::OpenPage(PageKind::Shelf)));
                }
                self.st.chime();
                self.maybe_schedule_collapse();
            }
            MeetEvent::Phone { phone, ev, .. } => match ev {
                PeerEvent::Hello { .. } => pinfo!("pair drop: a paired phone connected"),
                PeerEvent::Progress { files, done: 0, .. } => {
                    if !self.expanded {
                        self.st.notify(Icon::Phone, palette::GREEN, format!("{phone} is sending {}", files_label(files)), "They go straight onto the Shelf", 4.0, None);
                    }
                }
                PeerEvent::Progress { .. } => {}
                PeerEvent::Received { paths } => {
                    let n = paths.len();
                    let title = format!("{} from {phone} {} on the Shelf", files_label(n), if n == 1 { "is" } else { "are" });
                    let added: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
                    self.st.store.shelf.add_paths(&added, self.st.unix);
                    for p in added {
                        self.st.cmds.push(Command::LoadThumb(p));
                    }
                    self.st.save();
                    pinfo!("pair drop: {} from a paired phone", files_label(n));
                    if let Some(pd) = self.st.phone.as_mut() {
                        pd.outcome = Some((title.clone(), true, until));
                        pd.outcome_line = None;
                    }
                    if !self.expanded {
                        self.st.notify(Icon::Phone, palette::GREEN, title, "Open the Shelf to see them", 6.0, Some(NoticeAction::OpenPage(PageKind::Shelf)));
                    }
                    self.st.chime();
                    self.maybe_schedule_collapse();
                }
                PeerEvent::Failed { reason, detail } => {
                    pwarn!("pair drop: a send from a paired phone failed: {reason} {detail}");
                    if !self.expanded {
                        self.st.notify(Icon::Phone, palette::TEXT, format!("Nothing kept from {phone}"), format!("{reason}."), 6.0, None);
                    } else if let Some(pd) = self.st.phone.as_mut() {
                        pd.outcome = Some((format!("Nothing kept from {phone}. {reason}."), false, until));
                        pd.outcome_line = None;
                    }
                }
                PeerEvent::Ended { .. } => {}
            },
        }
        self.request_frame();
    }

    fn save_now(&mut self) {
        if !self.store_writable {
            pwarn!("data.json couldn't be read at start; not overwriting it");
            return;
        }
        if let Err(e) = save_json_atomic(&paths::store_path(), &self.st.store) {
            pwarn!("saving data failed: {e}");
        }
    }

    // ------------------------------------------------------------ self-test hooks

    /// Self-test only: render as if the monitor had `dpi` (120 = 125 %, 144 = 150 %, 192 = 200 %).
    pub fn selftest_set_dpi(&mut self, dpi: u32) {
        let dpi = dpi.clamp(96, 480);
        self.monitor.dpi = dpi;
        self.set_dpi(dpi);
        self.request_frame();
    }

    pub fn selftest_set_expanded(&mut self, on: bool) {
        if on {
            self.expand();
        } else {
            self.collapse();
        }
    }

    /// Self-test: whether the island is open.
    pub fn selftest_expanded(&self) -> bool {
        self.expanded
    }

    /// Self-test: a maximized window across the top of the island's monitor
    /// (what the full-screen watcher reports for one), or no window there.
    pub fn selftest_window_under(&mut self, under: bool) {
        let m = &self.monitor;
        let px = (TOP_BAND * self.gfx.scale()).ceil() as i32;
        self.top_windows = if under { vec![[m.rect.left, m.top(), m.rect.right, m.top() + px]] } else { Vec::new() };
    }

    /// Self-test: what the closed island shows now, and whether it stepped aside.
    pub fn selftest_mode(&self) -> (Mode, bool) {
        (self.mode, self.st.aside)
    }

    /// Self-test only: start the real Phone drop service, fetch its page from
    /// the network address it picked, and stop it. Returns (ok, detail).
    pub fn selftest_phone_drop(&mut self) -> (bool, String) {
        match services::phonedrop::PhoneDrop::start(self.sink.clone(), Vec::new()) {
            Err(e) => (false, e),
            Ok((svc, url)) => {
                let r = selftest::fetch_phone_page(&url);
                svc.stop();
                r
            }
        }
    }

    /// At rest: outline settled, mode current, no page switch or outgoing fade.
    /// The Settings window is open.
    pub fn settings_window_open(&self) -> bool {
        self.settings_win.is_some()
    }

    /// The Settings window on `section`, light or dark, opened if it isn't:
    /// its pixels (premultiplied BGRA) and their size.
    pub fn selftest_settings_shot(&mut self, section: Section, light: bool) -> (i32, i32, Vec<u8>) {
        let mut deferred = Vec::new();
        if self.settings_win.is_none() {
            self.open_settings_window(Some(section), &mut deferred);
        }
        self.st.now = self.st.now.max(clock::mono());
        let now = self.st.now;
        let Some(w) = self.settings_win.as_mut() else { return (0, 0, Vec::new()) };
        w.view.show_section(section);
        w.view.input.mouse = None;
        w.set_light(light);
        let _ = w.paint(&mut self.st, now, None, true);
        w.take_capture()
    }

    pub fn selftest_close_settings(&mut self) {
        self.close_settings_window();
    }

    /// Draw the Settings window `n` times: whole (`only` None, as after a
    /// click) or only a part (as a frame of a glide): each frame's time to
    /// draw and show (ms), sorted.
    pub fn selftest_settings_frames(&mut self, n: usize, only: Option<Rect>) -> Vec<f64> {
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            self.st.now = self.st.now.max(clock::mono());
            let now = self.st.now;
            let Some(w) = self.settings_win.as_mut() else { break };
            let t = std::time::Instant::now();
            let _ = w.paint(&mut self.st, now, only, false);
            out.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        out.sort_by(|a, b| a.total_cmp(b));
        out
    }

    /// Film the Settings window as a person uses it (`settings_host::film`)
    /// on this monitor, at `hz`. Its clicks change a setting: it goes back
    /// as it was, unsaved, afterwards.
    pub fn selftest_settings_film(&mut self, hz: f64, on_frame: impl FnMut(usize, i32, i32, &[u8], Option<RECT>)) -> Option<settings_host::Film> {
        let (before, queued) = (self.st.settings.clone(), self.st.cmds.len());
        let film = settings_host::film(&mut self.st, self.monitor.work, self.gfx.dpi.round() as u32, hz, on_frame);
        self.st.settings = before;
        self.st.cmds.truncate(queued);
        self.st.now = clock::mono();
        film.inspect_err(|e| pwarn!("selftest: the Settings window's film couldn't open its windows ({e})")).ok()
    }

    /// The scale the Settings window draws at.
    pub fn selftest_settings_scale(&self) -> f32 {
        self.settings_win.as_ref().map_or(1.0, |w| w.scale())
    }

    /// Where its card is in its window (DIPs).
    pub fn selftest_settings_card(&self) -> Rect {
        self.settings_win.as_ref().map_or(settings_host::card_rect(crate::settings_window::CARD_H), |w| w.card())
    }

    pub fn selftest_settled(&self) -> bool {
        self.island.settled() && self.target_mode() == self.mode && self.page_anim.is_none() && self.outgoing.is_none() && !self.glass_moving
    }

    /// Self-test: the frame just drawn left the frame clock running, so
    /// something moves or asked for more frames (false = the app rests,
    /// drawing nothing until an event or a timer wakes it).
    pub fn selftest_frames_wanted(&self) -> bool {
        self.clock.active()
    }

    /// Self-test, Glass theme: `px` (BGRA, the window's size) is what's under
    /// the window, and Windows is in light (`app_light`) or dark mode.
    pub fn selftest_glass_backdrop(&mut self, px: Vec<u8>, w: i32, h: i32, app_light: bool) {
        self.glass.app_light = app_light;
        self.glass.inject(px, w, h, &mut self.gfx);
    }

    /// Self-test: a new scene starts (the glass takes its side at once).
    pub fn selftest_glass_reseed(&mut self) {
        self.glass.reseed();
    }

    /// Self-test: the lens strength, and whether the glass shows the backdrop.
    pub fn selftest_glass_switches(&mut self, lens: f32, use_copy: bool) {
        self.glass.lens = lens;
        self.glass.use_copy = use_copy;
    }

    /// Self-test: the last frame's glass came from the shader, the backdrop
    /// is on the GPU, and the look content was painted in.
    pub fn selftest_glass_state(&self) -> (bool, bool, &'static poni_core::look::Look) {
        (self.gfx.glass_drew(), self.gfx.glass_has_copy(), self.gfx.look())
    }

    /// Self-test: the captured rectangle (surface pixels).
    pub fn selftest_capture_rect(&self) -> RECT {
        self.capture_rect
    }

    /// Self-test: frames are drawn by Microsoft's software rasterizer, not a GPU.
    pub fn selftest_software_gpu(&self) -> bool {
        self.gfx.software_gpu()
    }

    /// Render a frame right now (bypassing the clock) and return the pixels
    /// of the island's canvas (island, shoulders and hit margin) and where
    /// that canvas sits on screen.
    pub fn selftest_capture(&mut self) -> (i32, i32, Vec<u8>, (i32, i32)) {
        let mut d = Vec::new();
        self.capture_next = true;
        self.frame(&mut d, None);
        let r = self.capture_rect;
        (r.right - r.left, r.bottom - r.top, self.gfx.take_capture(), (self.surface.x + r.left, self.surface.y + r.top))
    }

    /// The window's screen rectangle as laid out (left, top, width, height).
    pub fn selftest_surface(&self) -> (i32, i32, i32, i32) {
        (self.surface.x, self.surface.y, self.surface.w, self.surface.h)
    }

    pub fn renderer(&self) -> &'static str {
        self.renderer_name()
    }

    pub fn selftest_hover(&mut self, p: Option<Point>) {
        self.input.mouse = p;
        self.hovering = p.is_some();
    }

    /// The middle of a widget drawn in the last frame, found by its id.
    pub fn selftest_hit_center(&self, id: crate::ui::WidgetId) -> Option<Point> {
        self.hits.iter().find(|h| h.id == id).map(|h| h.rect.center())
    }

    pub fn selftest_click(&mut self, p: Point) {
        self.input.mouse = Some(p);
        if let Some(h) = self.hit_at(p) {
            self.input.clicked = Some((h.id, MouseButton::Left));
        }
    }

    /// A press and release at `p` (DIPs) through the island's own mouse
    /// messages, for what acts at mouse-down (`selftest_click` only sets the
    /// click). Returns whether the island opened.
    pub fn selftest_press(&mut self, p: Point) -> bool {
        let s = self.gfx.scale();
        let (x, y) = ((p.x * s).round() as i32 & 0xffff, (p.y * s).round() as i32 & 0xffff);
        let lp = LPARAM(((y << 16) | x) as isize);
        // Nothing a left press defers (the right button's menu is what does).
        let mut deferred = Vec::new();
        self.handle_island(WM_LBUTTONDOWN, WPARAM(0), lp, &mut deferred);
        self.handle_island(WM_LBUTTONUP, WPARAM(0), lp, &mut deferred);
        self.expanded
    }

    /// A press on the island at `p` (DIPs), a move `dx` DIPs sideways and
    /// the release there, through the island's own mouse messages. Returns
    /// whether the island opened.
    pub fn selftest_drag(&mut self, p: Point, dx: f32) -> bool {
        let s = self.gfx.scale();
        let lp = |p: Point| LPARAM(((((p.y * s).round() as i32 & 0xffff) << 16) | ((p.x * s).round() as i32 & 0xffff)) as isize);
        let to = Point::new(p.x + dx, p.y);
        let mut deferred = Vec::new();
        self.handle_island(WM_LBUTTONDOWN, WPARAM(0), lp(p), &mut deferred);
        self.handle_island(WM_MOUSEMOVE, WPARAM(0x0001), lp(to), &mut deferred);
        self.handle_island(WM_LBUTTONUP, WPARAM(0), lp(to), &mut deferred);
        self.expanded
    }

    /// The left button goes down at `p` (window DIPs) and stays down.
    pub fn selftest_button_down(&mut self, p: Point) {
        let lp = self.selftest_lparam(p);
        self.handle_island(WM_LBUTTONDOWN, WPARAM(0), lp, &mut Vec::new());
    }

    /// The pointer moves to `p` with the left button down.
    pub fn selftest_move(&mut self, p: Point) {
        let lp = self.selftest_lparam(p);
        self.handle_island(WM_MOUSEMOVE, WPARAM(0x0001), lp, &mut Vec::new());
    }

    /// The left button comes up at `p`.
    pub fn selftest_button_up(&mut self, p: Point) {
        let lp = self.selftest_lparam(p);
        self.handle_island(WM_LBUTTONUP, WPARAM(0), lp, &mut Vec::new());
    }

    fn selftest_lparam(&self, p: Point) -> LPARAM {
        let s = self.gfx.scale();
        LPARAM(((((p.y * s).round() as i32 & 0xffff) << 16) | ((p.x * s).round() as i32 & 0xffff)) as isize)
    }

    /// The island's centre, DIPs from its monitor's left edge.
    pub fn selftest_island_centre(&self) -> f32 {
        (self.surface.x - self.monitor.rect.left) as f32 / self.gfx.scale() + self.cx
    }

    /// The island's monitor's width in DIPs.
    pub fn selftest_monitor_width(&self) -> f32 {
        (self.monitor.rect.right - self.monitor.rect.left) as f32 / self.gfx.scale()
    }

    /// Room the island keeps beside its body (shoulders and hit margin, or the pill's shadow).
    pub fn selftest_side_room(&self) -> f32 {
        self.island.side_room()
    }

    /// The island body in window DIPs (where clicks land).
    pub fn island_body(&self) -> Rect {
        self.island.body_rect_at(self.cx)
    }

    /// The island body relative to the last captured frame (DIPs).
    pub fn island_body_in_capture(&self) -> Rect {
        let s = self.gfx.scale();
        let r = self.capture_rect;
        self.island.body_rect_at(self.cx).offset(-(r.left as f32) / s, -(r.top as f32) / s)
    }

    pub fn scale(&self) -> f32 {
        self.gfx.scale()
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }
}

/// Diorama's own accent (which its day and night looks paint as theirs),
/// Windows' accent colour otherwise.
fn theme_accent(theme: Theme) -> Color {
    if theme == Theme::Diorama {
        poni_core::look::DIORAMA_ACCENT
    } else if theme == Theme::Glass {
        services::system_accent_base().unwrap_or(palette::BLUE)
    } else {
        services::system_accent().unwrap_or(palette::BLUE)
    }
}

/// The island's current content: a page (expanded) or the collapsed activity.
/// Returns (repaint, anim, focus, live) as requested by the drawing code.
#[allow(clippy::too_many_arguments)]
fn draw_content(
    c: &mut Canvas,
    input: &mut Input,
    hits: &mut Vec<Hit>,
    st: &mut AppState,
    mode: Mode,
    lay: Rect,
    body: Rect,
    transition: Option<pages::PageTransition>,
    now: f64,
) -> (bool, bool, Option<bool>, bool) {
    // The closed island at another size (Settings > Island > Size when
    // closed): its content lays out at its own size and draws scaled about
    // the top centre, where the island hangs; the pointer is mapped into
    // that layout and the click targets back out of it.
    let k = activities::collapsed_scale(&mode, st);
    if k != 1.0 {
        let pivot = Point::new(body.center().x, body.y);
        let inward = |p: Point| Point::new(pivot.x + (p.x - pivot.x) / k, pivot.y + (p.y - pivot.y) / k);
        let own = Rect::new(pivot.x - body.w / k / 2.0, body.y, body.w / k, body.h / k);
        let saved = (input.mouse, input.press_pos, input.released);
        input.mouse = input.mouse.map(inward);
        input.press_pos = input.press_pos.map(inward);
        input.released = input.released.map(|(id, p)| (id, inward(p)));
        let before = hits.len();
        let xf = c.save_transform();
        c.scale_about(pivot, k);
        let out = {
            let mut ui = Ui::new(c, input, hits, now);
            activities::draw(&mut ui, st, &mode, own);
            (ui.repaint_requested(), ui.anim_requested(), ui.focus_request(), ui.live_requested())
        };
        c.restore_transform(xf);
        (input.mouse, input.press_pos, input.released) = saved;
        for h in &mut hits[before..] {
            let r = h.rect;
            h.rect = Rect::new(pivot.x + (r.x - pivot.x) * k, pivot.y + (r.y - pivot.y) * k, r.w * k, r.h * k);
        }
        return out;
    }
    let mut ui = Ui::new(c, input, hits, now);
    match mode {
        Mode::Expanded(page) => pages::draw_expanded(&mut ui, st, lay, page, transition),
        _ => activities::draw(&mut ui, st, &mode, body),
    }
    (ui.repaint_requested(), ui.anim_requested(), ui.focus_request(), ui.live_requested())
}

/// DWM's (refreshes, frames composed, frames missed) counters, if available.
fn dwm_counters() -> Option<(u64, u64, u64)> {
    use windows::Win32::Graphics::Dwm::{DwmGetCompositionTimingInfo, DWM_TIMING_INFO};
    let mut info = DWM_TIMING_INFO { cbSize: std::mem::size_of::<DWM_TIMING_INFO>() as u32, ..Default::default() };
    unsafe { DwmGetCompositionTimingInfo(HWND::default(), &mut info) }.ok()?;
    Some((info.cRefresh, info.cFrame, info.cFramesMissed))
}

/// The window's size in pixels for an island body up to `body_w`×`body_h`
/// DIPs at `scale`, on a monitor `mon_w` pixels wide: room for the spring's
/// overshoot, and the island's own room around the body (Classic: the
/// shoulders and hit margin; floating: the gap above, the shadow's room
/// beside and below).
fn surface_size(island: &Island, body_w: f32, body_h: f32, scale: f32, mon_w: i32) -> (i32, i32) {
    let mut w = ((body_w * OVERSHOOT + 2.0 * island.side_room()) * scale).ceil() as i32;
    // An even margin on both sides puts the centre on the monitor's centre.
    if (mon_w - w) % 2 != 0 {
        w += 1;
    }
    // The gap the island rests at, not where an outline sliding into or out
    // of it is now: the window never resizes during that motion.
    let top = if island.floating() { pill::TOP_GAP } else { 0.0 };
    let h = ((body_h * OVERSHOOT + island.bottom_room() + top) * scale).ceil() as i32;
    (w, h)
}

/// Where the island window takes the mouse (DIPs, window space). Open: the
/// whole canvas, a forgiving margin around the panel. Collapsed: the
/// island's own outline. While it floats (the Pill theme) the gap above it
/// takes the mouse too, down to the pill's middle, so a pointer thrown at
/// the top edge lands on it as it lands on Classic's island; beside and
/// below the pill (its shadow) clicks pass through. The Hidden idle style's
/// invisible island takes it only in a band at the top edge as tall as its
/// body, as Classic's hidden strip does, so what sits under the gap keeps
/// its clicks. The GPU renderer pushes this as the window's hit mask; the
/// software renderer paints it at alpha 1/255 under the island.
fn hit_mask_for(island: &Island, cx: f32, expanded: bool, hidden: bool) -> HitMask {
    if expanded {
        return HitMask { rect: Some(island.canvas_rect_at(cx)), notch: None, pill: None };
    }
    let body = island.body_rect_at(cx);
    if !island.floating() {
        return HitMask { rect: None, notch: Some((body, island.current().r, SHOULDER)), pill: None };
    }
    if hidden {
        return HitMask { rect: Some(Rect::new(body.x, 0.0, body.w, body.h)), notch: None, pill: None };
    }
    let gap = island.hover_rect_at(cx);
    HitMask { rect: Some(Rect::new(gap.x, gap.y, gap.w, body.y + body.h / 2.0 - gap.y)), notch: None, pill: Some((body, island.current().r)) }
}

/// Name of a morph for the motion log.
fn motion_name(from: &Mode, to: &Mode) -> &'static str {
    match (from, to) {
        (Mode::Expanded(_), Mode::Expanded(_)) => "page",
        (_, Mode::Expanded(_)) => "expand",
        (Mode::Expanded(_), _) => "collapse",
        _ => "pill",
    }
}

/// "1 file", "3 files".
/// The panel's list: most recently used first.
fn paired_phones(pairs: Vec<poni_core::pairdrop::Pair>) -> Vec<PairedPhone> {
    let mut v: Vec<PairedPhone> = pairs.into_iter().map(|p| PairedPhone { room: p.room(), name: p.name, seen: p.seen.max(p.added) }).collect();
    v.sort_by_key(|a| std::cmp::Reverse(a.seen));
    v
}

fn files_label(n: usize) -> String {
    format!("{n} {}", if n == 1 { "file" } else { "files" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::island::HIT_MARGIN;

    /// The window's centre in DIPs (any value: the geometry follows it).
    const CX: f32 = 336.6;
    const SCALES: [f32; 4] = [1.0, 1.25, 1.5, 2.0];

    fn island(g: Geom, floating: bool) -> Island {
        let mut i = Island::new(g);
        i.set_floating(floating.then_some(pill::TOP_GAP));
        i
    }

    fn pt(x: f32, y: f32) -> Point {
        Point::new(x, y)
    }

    /// The pixel centre `over_shadow` tests for the pixel at (x, y) px.
    fn px_centre(x: i32, y: i32, s: f32) -> Point {
        pt((x as f32 + 0.5) / s, (y as f32 + 0.5) / s)
    }

    #[test]
    fn classic_hangs_from_the_edge() {
        let i = island(Geom::new(84.0, 30.0, 14.0), false);
        let body = i.body_rect_at(CX);
        assert_eq!(body, Rect::new(CX - 42.0, 0.0, 84.0, 30.0));
        assert_eq!(i.hover_rect_at(CX), body, "Classic hovers on its body");
        let side = SHOULDER + HIT_MARGIN;
        assert_eq!(i.canvas_rect_at(CX), Rect::new(CX - 42.0 - side, 0.0, 84.0 + 2.0 * side, 30.0 + HIT_MARGIN));
        assert_eq!((i.top(), i.side_room(), i.bottom_room()), (0.0, side, HIT_MARGIN));
    }

    #[test]
    fn pill_floats_below_the_edge() {
        let i = island(Geom::new(152.0, 32.0, 16.0), true);
        let (gap, side, below) = (pill::TOP_GAP, i.side_room(), i.bottom_room());
        assert!(i.floating() && i.top() == gap && side > 0.0 && below > 0.0, "room for the gap and the shadow");
        let body = i.body_rect_at(CX);
        assert_eq!(body, Rect::new(CX - 76.0, gap, 152.0, 32.0));
        assert_eq!(i.hover_rect_at(CX), Rect::new(CX - 76.0, 0.0, 152.0, gap + 32.0), "the gap above it and the body");
        assert_eq!(i.canvas_rect_at(CX), Rect::new(CX - 76.0 - side, 0.0, 152.0 + 2.0 * side, gap + 32.0 + below));
    }

    #[test]
    fn classic_masks_are_unchanged() {
        let i = island(Geom::new(84.0, 30.0, 14.0), false);
        let body = i.body_rect_at(CX);
        assert_eq!(hit_mask_for(&i, CX, false, false), HitMask { rect: None, notch: Some((body, 14.0, SHOULDER)), pill: None });
        // Hidden is Classic's own strip there, with its shoulders.
        assert_eq!(hit_mask_for(&i, CX, false, true), HitMask { rect: None, notch: Some((body, 14.0, SHOULDER)), pill: None });
        assert_eq!(hit_mask_for(&i, CX, true, false), HitMask { rect: Some(i.canvas_rect_at(CX)), notch: None, pill: None });
    }

    #[test]
    fn pill_takes_the_gap_not_its_shadow() {
        let i = island(Geom::new(152.0, 32.0, 16.0), true);
        let body = i.body_rect_at(CX);
        let m = hit_mask_for(&i, CX, false, false);
        assert_eq!(m.pill, Some((body, 16.0)));
        // A pointer thrown at the top edge: anywhere above the pill.
        for x in [body.x + 0.1, CX, body.right() - 0.1] {
            assert!(m.contains(pt(x, 0.0)), "top edge at x {x}");
            assert!(m.contains(pt(x, pill::TOP_GAP - 0.1)), "just above the pill at x {x}");
        }
        assert!(m.contains(pt(body.x + 0.5, body.y + 0.5)), "the top corners: the gap reaches the pill's middle");
        assert!(m.contains(pt(CX, body.bottom() - 0.5)), "the pill's bottom edge");
        // Beside it at the top edge, beside and below it (its shadow): through.
        assert!(!m.contains(pt(body.x - 0.5, 0.0)) && !m.contains(pt(body.right() + 0.5, 0.0)));
        for d in [0.5, 3.0, 8.0] {
            assert!(!m.contains(pt(body.x - d, body.center().y)), "beside, {d} out");
            assert!(!m.contains(pt(body.right() + d, body.center().y)), "beside, {d} out");
            assert!(!m.contains(pt(CX, body.bottom() + d)), "below, {d} down");
        }
        assert!(!m.contains(pt(body.x + 1.0, body.bottom() - 1.0)), "the round bottom corners");
    }

    #[test]
    fn pill_open_takes_the_card_its_margin_and_the_gap() {
        let i = island(Geom::new(620.0, 216.0, 40.0), true);
        let canvas = i.canvas_rect_at(CX);
        let m = hit_mask_for(&i, CX, true, false);
        assert_eq!(m, HitMask { rect: Some(canvas), notch: None, pill: None });
        let body = i.body_rect_at(CX);
        assert!(m.contains(pt(CX, 0.0)), "the gap above the card");
        assert!(m.contains(pt(body.x - i.side_room() + 0.5, body.center().y)), "the margin beside it");
        assert!(m.contains(pt(CX, body.bottom() + i.bottom_room() - 0.5)), "the margin below it");
        assert!(!m.contains(pt(canvas.right() + 0.5, body.center().y)) && !m.contains(pt(CX, canvas.bottom() + 0.5)));
    }

    #[test]
    fn hidden_pill_takes_a_band_at_the_edge() {
        // Classic's hidden strip (150 x 5, drawn at least 8 tall), floating:
        // the mouse finds it at the top edge in a band as tall as Classic's,
        // not down the gap and the invisible body under it.
        let i = island(Geom::new(150.0, 5.0, 2.5), true);
        let classic = island(Geom::new(150.0, 5.0, 2.5), false).body_rect_at(CX);
        let body = i.body_rect_at(CX);
        let m = hit_mask_for(&i, CX, false, true);
        assert_eq!(m, HitMask { rect: Some(Rect::new(classic.x, 0.0, classic.w, classic.h)), notch: None, pill: None });
        assert!(m.contains(pt(CX, 0.0)) && m.contains(pt(body.x + 0.1, classic.h - 0.1)));
        assert!(!m.contains(pt(CX, classic.h)) && !m.contains(pt(CX, body.center().y)), "below the band, the invisible body included");
    }

    #[test]
    fn pixels_at_every_scale() {
        // What over_shadow sees, pixel by pixel, at 100 / 125 / 150 / 200 %:
        // every pixel row of the gap above the pill takes the mouse, the
        // pixels beside and under the pill don't.
        let i = island(Geom::new(84.0, 32.0, 16.0), true);
        let body = i.body_rect_at(CX);
        let m = hit_mask_for(&i, CX, false, false);
        for s in SCALES {
            let x = (CX * s) as i32;
            let top_px = (pill::TOP_GAP * s).round() as i32;
            assert_eq!(top_px as f32, pill::TOP_GAP * s, "the gap is whole pixels at {s}");
            for y in 0..top_px {
                assert!(m.contains(px_centre(x, y, s)), "gap row {y} at {s}");
            }
            let mid = ((body.y + body.h / 2.0) * s) as i32;
            let left = (body.x * s).floor() as i32 - 1;
            let right = (body.right() * s).ceil() as i32;
            assert!(!m.contains(px_centre(left, mid, s)) && !m.contains(px_centre(right, mid, s)), "beside at {s}");
            assert!(m.contains(px_centre(left + 2, mid, s)) && m.contains(px_centre(right - 2, mid, s)), "inside the ends at {s}");
            let below = (body.bottom() * s).ceil() as i32;
            assert!(!m.contains(px_centre(x, below, s)) && m.contains(px_centre(x, below - 1, s)), "the bottom edge at {s}");
            assert!(m.contains(px_centre(x, 0, s)), "the top edge at {s}");
        }
    }

    #[test]
    fn window_room_follows_the_theme_and_comes_back() {
        // A theme switch lays the window out again: Pill grows it for the gap
        // (and needs less room beside: no shoulders), Classic gets its exact
        // size back.
        let mut i = island(Geom::new(84.0, 30.0, 14.0), false);
        for s in SCALES {
            for mon_w in [1920, 1366, 2560, 3841] {
                let classic = surface_size(&i, 620.0, 400.0, s, mon_w);
                // The formula Classic has always laid out with.
                let mut w = ((620.0 * OVERSHOOT + 2.0 * (SHOULDER + HIT_MARGIN)) * s).ceil() as i32;
                if (mon_w - w) % 2 != 0 {
                    w += 1;
                }
                assert_eq!(classic, (w, ((400.0 * OVERSHOOT + HIT_MARGIN) * s).ceil() as i32), "Classic at {s} on {mon_w}");
                assert_eq!((mon_w - classic.0) % 2, 0, "centred on whole pixels");

                i.set_floating(Some(pill::TOP_GAP));
                let floating = surface_size(&i, 620.0, 400.0, s, mon_w);
                let want_h = ((400.0 * OVERSHOOT + i.bottom_room() + pill::TOP_GAP) * s).ceil() as i32;
                assert_eq!(floating.1, want_h, "Pill has room for the gap at {s}");
                assert_eq!((mon_w - floating.0) % 2, 0);
                i.set_floating(None);
                assert_eq!(surface_size(&i, 620.0, 400.0, s, mon_w), classic, "back to Classic at {s} on {mon_w}");
            }
        }
    }
}
