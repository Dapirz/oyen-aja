//! All state shared by pages, activities and the app loop.
//!
//! Owned by the UI thread. Services never touch it directly; they send
//! [`crate::events::AppEvent`]s that the app applies here.

use crate::commands::{Command, MediaCmd, Sound};
use crate::gfx::Image;
use crate::icons::Icon;
use crate::pages::PageKind;
use poni_core::addons::Status as AddonStatus;
use poni_core::datetime::LocalDateTime;
use poni_core::geom::{palette, Color};
use poni_core::lrc::Lyrics;
use poni_core::phonedrop::QrMatrix;
use poni_core::pomodoro::Pomodoro;
use poni_core::prayer::{Place, PlaceList, PrayerMoment, Site, World};
use poni_core::romanize::Ruby;
use poni_core::settings::Settings;
use poni_core::store::Store;
use poni_core::timeline::Timeline;
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug)]
pub struct MediaSession {
    /// AppUserModelId of the player (e.g. `Spotify.exe`, `Chrome`).
    pub app_id: String,
    /// Friendly player name ("Spotify", "Chrome", …).
    pub app_name: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art: Option<Image>,
    /// Accent derived from the art (for progress bars / the activity glow).
    pub art_color: Option<Color>,
    pub timeline: Timeline,
    pub can_prev: bool,
    pub can_next: bool,
    pub can_play_pause: bool,
    pub can_seek: bool,
    /// Identity of the track: `title\u{1}artist\u{1}album`.
    pub track_key: String,
}

impl MediaSession {
    pub fn make_track_key(title: &str, artist: &str, album: &str) -> String {
        format!("{title}\u{1}{artist}\u{1}{album}")
    }
    pub fn is_playing(&self) -> bool {
        self.timeline.playing
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LyricsStatus {
    Idle,
    Loading,
    Found,
    Instrumental,
    NotFound,
    Error(String),
}

#[derive(Clone, Debug)]
pub struct LyricsState {
    pub track_key: String,
    pub status: LyricsStatus,
    pub lyrics: Option<Lyrics>,
    pub source: String,
    /// Each line's pieces with their readings in Latin letters (Japanese,
    /// Korean, Chinese); empty for lines that need none.
    pub ruby: Vec<Vec<Ruby>>,
    /// The song's kanji went unread because Windows has no Japanese reading
    /// data and the Japanese readings add-on isn't on this PC: the app
    /// switches the add-on on the first time, and the Music page shows how
    /// it comes in (or offers it, after a switch-off by hand).
    pub wants_ja_readings: bool,
}

impl Default for LyricsState {
    fn default() -> Self {
        Self { track_key: String::new(), status: LyricsStatus::Idle, lyrics: None, source: String::new(), ruby: Vec::new(), wants_ja_readings: false }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VolumeState {
    pub level: f32,
    pub muted: bool,
    pub known: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PowerState {
    pub has_battery: bool,
    /// 0..=100 (meaningful only with a battery).
    pub percent: u8,
    pub charging: bool,
    pub on_ac: bool,
    pub saver: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum HudKind {
    Volume { level: f32, muted: bool },
    Charging { percent: u8, plugged: bool },
    Bluetooth { name: String, connected: bool, battery: Option<u8>, audio: bool },
    LowBattery { percent: u8 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hud {
    pub kind: HudKind,
    /// Monotonic time when it disappears.
    pub until: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NoticeAction {
    OpenPage(PageKind),
    /// Open Focus and start the (idle) timer.
    StartFocus,
    /// Open the app another app's notification came from (its
    /// AppUserModelID; Notifications). The island stays closed.
    OpenApp(String),
}

/// A one-click action shown as a small pill inside a notice (doesn't expand).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineAction {
    /// Start the idle Pomodoro phase.
    StartFocus,
    /// Put the notice's hidden words in place: a message and who it's from (Notifications).
    ShowAmount,
    /// Stop the adzan (Prayer times).
    StopAdzan,
    /// Put the notice away now (a new version's: the menu and Settings
    /// keep offering it).
    Hide,
    /// Open the app a notification came from (the notice's `OpenApp`).
    Open,
}

/// Whether this copy may read other apps' notifications (the Notifications
/// add-on, `services::notifications`), for its page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NotifyAccess {
    /// Not looked at yet: the add-on is off, or just came on.
    #[default]
    Unknown,
    /// This copy has no package identity (the download from GitHub), and
    /// Windows lets only a packaged app read notifications.
    NoPackage,
    /// Windows hasn't been asked yet, or its question was closed unanswered.
    Ask,
    /// Windows' question is on screen.
    Asking,
    /// Asked, and Windows came back without an answer (the question was
    /// closed, or never showed): ask again, or turn it on in Windows' settings.
    Unanswered,
    /// Turned off for this app in Windows' settings.
    Denied,
    /// Windows lets it read them: new ones show on the island.
    Allowed,
    /// Windows' notification listener didn't answer (an older Windows, or a
    /// failure the log names).
    Failed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    pub icon: Icon,
    pub color: Color,
    pub title: String,
    /// Dimmer words after the title, on its line ("0.2.5" after "Update
    /// available"); empty for most notices.
    pub title_dim: String,
    pub subtitle: String,
    pub until: f64,
    pub action: Option<NoticeAction>,
    pub inline: Option<InlineAction>,
    /// The subtitle Show puts in place of `subtitle` ([`InlineAction::ShowAmount`]).
    pub reveal: Option<String>,
}

/// How long a notice with a sum or a message stays (seconds), and how much
/// longer once Show reveals it.
pub const NOTICE_S: f64 = 8.0;
pub const SHOWN_S: f64 = 6.0;
/// A notice held back by a full-screen app still shows within this time.
pub const HELD_MAX_S: f64 = 30.0 * 60.0;

impl Notice {
    /// No time limit (a new version's): it stays until clicked or put away,
    /// and lets a volume change show over it meanwhile.
    pub fn stays(&self) -> bool {
        self.until.is_infinite()
    }
}

/// Screen time's stretch under way: the app in front since `since` (unix
/// seconds), counted on `day`.
#[derive(Clone, Debug, PartialEq)]
pub struct ScreenLive {
    pub key: String,
    pub name: String,
    pub path: String,
    pub since: i64,
    pub day: String,
}

/// A phone wants to send these files; the Shelf asks the person at the PC.
#[derive(Clone, Debug, PartialEq)]
pub struct PhoneAsk {
    pub id: u64,
    /// "iPhone", "Android phone", …
    pub from: String,
    /// (name, size) in the order the phone sends them.
    pub files: Vec<(String, u64)>,
    pub total: u64,
}

/// Files coming in from the phone.
#[derive(Clone, Debug, PartialEq)]
pub struct PhoneTransfer {
    pub id: u64,
    pub from: String,
    /// The file being sent (0-based) and how many there are.
    pub file: usize,
    pub files: usize,
    pub done: u64,
    pub total: u64,
}

/// What the phone panel shows on its left and right.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum PhoneView {
    /// Today's same-Wi-Fi QR code.
    #[default]
    Lan,
    /// A pairing QR code (`services::pairdrop`): the link, its code and
    /// until when (app clock) it works.
    Pairing { url: String, qr: Option<QrMatrix>, until: f64 },
    /// The paired phones, each with Remove.
    Paired,
}

/// A paired phone as the panel lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairedPhone {
    /// Its room on the meeting server (what Remove names).
    pub room: String,
    pub name: String,
    /// Unix seconds it last connected (0 = not yet).
    pub seen: i64,
}

/// Phone drop while the Shelf's phone panel is open: the link, its QR code
/// and what the phone is doing (`services::phonedrop` runs the server).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PhoneDrop {
    pub view: PhoneView,
    /// A second line under `outcome` (None = the usual hint).
    pub outcome_line: Option<String>,
    /// The link the QR code carries (empty while starting or after an error).
    pub url: String,
    pub qr: Option<QrMatrix>,
    /// Why it couldn't start, as a sentence for the island.
    pub error: Option<String>,
    /// The kind of phone that last opened the page ("iPhone").
    pub visitor: Option<String>,
    pub ask: Option<PhoneAsk>,
    pub transfer: Option<PhoneTransfer>,
    /// The last result, whether it's good news, and until when it shows.
    pub outcome: Option<(String, bool, f64)>,
}

impl PhoneDrop {
    /// The island stays open while a QR code waits for its first phone and
    /// while a phone waits for an answer. Once the panel has shown a result
    /// (a phone paired, files arrived), or on the list of paired phones,
    /// nothing waits: the island closes when the pointer leaves.
    pub fn holds_island(&self) -> bool {
        let waiting = !matches!(self.view, PhoneView::Paired) && self.outcome.is_none();
        self.ask.is_some() || (waiting && self.error.is_none() && self.visitor.is_none() && self.transfer.is_none())
    }
}

/// Transient per-page UI state (scroll offsets, text being typed, …).
#[derive(Clone, Debug, Default)]
pub struct PageUiState {
    /// The Notes page: a short line beside the notes until a time
    /// ("Copied"), and whether a recording is kept on this PC for Try again.
    pub notes_note: Option<(String, f64)>,
    pub notes_kept: bool,
    /// The Gemini key's last four characters while one is saved (what
    /// Settings shows of it).
    pub notes_key: Option<String>,
    pub task_input: String,
    pub task_scroll: f32,
    pub shelf_scroll: f32,
    /// Smoothly animated lyric scroll position (line index as float).
    pub lyric_pos: f32,
    pub lyric_track: String,
    /// Volume HUD bar position, gliding toward the real level.
    pub hud_level: f32,
    /// Collapsed media pill shows the track title until this mono time.
    pub peek_until: f64,
    /// Header tab under the pointer and since when (for hover labels).
    pub tab_hover: Option<(usize, f64)>,
    /// The page the Settings button was pressed on (pressing it again goes back).
    pub before_settings: Option<PageKind>,
    /// Start with Windows, as last read off the UI thread (None until Settings first opens).
    pub autostart: Option<bool>,
    /// The Clipboard page's scroll, and the row that just got copied (index,
    /// mono time) for its "Copied" flash.
    pub clip_scroll: f32,
    pub clip_flash: Option<(usize, f64)>,
    /// The Eyes look on a MacBook's notch: the pointer the eyes follow and
    /// the side of the camera they peek out from (`mac/notch.rs`).
    #[cfg(target_os = "macos")]
    pub peek: poni_core::peek::Peek,
}

/// Where finding the PC's place stands.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Locate {
    /// Not asked (following the location is off, or not yet).
    #[default]
    Idle,
    Asking,
    /// Picked the nearest place, this far from the PC (km).
    Found { km: f64 },
    /// The nearest place in either list is too far away to use (km).
    Far { km: f64 },
    /// Away from Kemenag's places, and the cities around couldn't be looked
    /// up (offline): the place stays as it was.
    Offline,
    /// Windows' location is off, or not allowed for this app.
    Denied,
    /// Windows couldn't tell where the PC is.
    Unknown,
}

/// One of Prayer times' online city files, while it's wanted.
#[derive(Clone, Debug)]
pub enum CityFile {
    Asking,
    Here(std::sync::Arc<World>),
    /// Offline, or the website didn't answer.
    Failed,
}

/// The Prayer times add-on's live state.
#[derive(Clone, Debug, Default)]
pub struct PrayerState {
    /// The add-on's place list, read once its files are in place.
    pub places: Option<std::sync::Arc<PlaceList>>,
    /// The online city files asked for (`poni_core::prayer::world`): the
    /// picker's, dropped when it closes, and a location's squares, dropped
    /// once its place is found.
    pub cities: HashMap<String, CityFile>,
    /// A location waiting for its squares: the point (never saved) and the
    /// files it needs.
    pub fix: Option<(f64, f64, Vec<String>)>,
    /// The last location that went online and the place it found: the next
    /// one within a kilometre takes it again without asking.
    pub last_fix: Option<(f64, f64, Option<(Place, f64)>)>,
    /// The picker's last search: what was typed, and what it found.
    pub found: Option<(String, Vec<Place>)>,
    /// The place the times are for: the chosen one, or the time zone's
    /// suggestion while none is chosen.
    pub place: Option<Place>,
    /// `place` is the time zone's suggestion.
    pub suggested: bool,
    pub locate: Locate,
    /// When the PC's location was last asked (UTC seconds).
    pub located_at: Option<i64>,
    /// The place picker shows (Change), and what's typed in it.
    pub picking: bool,
    pub query: String,
    /// Prayer times up to this moment (UTC seconds) have had their moment.
    pub handled_until: Option<i64>,
    /// The adzan is playing.
    pub adzan: bool,
    /// OK on the countdown put it away until this prayer's time (UTC
    /// seconds): the closed island shows what it would otherwise (the song
    /// and its sung line) until then. Kept in memory only.
    pub countdown_ok: Option<i64>,
    /// Self-test only: moves the prayer clock by this many seconds.
    pub clock_shift: i64,
}

pub struct AppState {
    pub settings: Settings,
    pub store: Store,
    /// Monotonic seconds at the current frame.
    pub now: f64,
    pub local: LocalDateTime,
    pub unix: i64,
    pub media: Option<MediaSession>,
    pub lyrics: LyricsState,
    pub pomodoro: Pomodoro,
    pub volume: VolumeState,
    pub power: PowerState,
    pub hud: Option<Hud>,
    pub notice: Option<Notice>,
    /// Screen time: the days counted, the stretch under way (for the page's
    /// running total), and the apps' icons by key.
    pub screen: poni_core::screentime::Usage,
    pub screen_live: Option<ScreenLive>,
    pub screen_icons: std::collections::HashMap<String, crate::gfx::Image>,
    /// The Clipboard add-on's history.
    pub clip: poni_core::cliphist::ClipHistory,
    /// The Notes page's meeting (the Meeting notes add-on).
    pub notes: poni_core::notes::Taker,
    /// Apps' logos for the notification card, by AppUserModelID (each
    /// fetched once, the first time that app sends a notification).
    pub notify_logos: std::collections::HashMap<String, crate::gfx::Image>,
    /// The notification the card shows (Windows' id), and its sender's
    /// picture once read.
    pub notice_card_id: u32,
    pub notice_photo: Option<crate::gfx::Image>,
    pub shelf_thumbs: HashMap<String, Option<Image>>,
    pub ui: PageUiState,
    /// System accent colour (falls back to blue).
    pub accent: Color,
    /// The page currently shown when expanded.
    pub page: PageKind,
    /// When playback went from playing to paused (the pill lingers 30 s).
    pub media_paused_at: Option<f64>,
    pub pinned: bool,
    /// Stepped aside (Settings › Over maximized windows): an app window sits
    /// under where the island rests, so the closed island rests as an
    /// invisible strip at the top edge (`activities::Mode::Aside`). Windows
    /// sets it; the Mac never does.
    pub aside: bool,
    /// Files are being dragged over the island right now.
    pub drag_hover: bool,
    /// Phone drop, while it's open.
    pub phone: Option<PhoneDrop>,
    /// Paired phones, newest first (`services::pairdrop`).
    pub paired: Vec<PairedPhone>,
    /// Whether the meeting server answers (None = not asked yet: no pairs).
    pub pair_server: Option<bool>,
    /// A notice that came while a full-screen app hid the island, and when
    /// (mono seconds): it shows once the island is back, if no older than
    /// [`HELD_MAX_S`].
    pub held: Option<(Notice, f64)>,
    /// Where each add-on stands, by id (every one in `poni_core::addons::ALL`).
    pub addons: BTreeMap<&'static str, AddonStatus>,
    /// The Prayer times add-on, while it is on (its choices: `settings.prayer`).
    pub prayer: PrayerState,
    /// The Script page's reading (its script and pace are in `store.script`).
    pub script: poni_core::script::Reader,
    /// A message under the script until a time (mono seconds).
    pub script_note: Option<(&'static str, f64)>,
    /// Whether Windows leaves the island out of screen capture while the
    /// Script page shows (None = not asked yet, or not on Windows).
    pub script_hidden: Option<bool>,
    /// Whether Windows lets the island read notifications.
    pub notify_access: NotifyAccess,
    /// This copy can read other apps' notifications: Windows lets only an app
    /// with a package identity, the Microsoft Store copy, do it. The download
    /// can't, so it offers no Notifications at all. Set once at start (the
    /// self-test sets it per scene).
    pub notify_can: bool,
    /// Side effects requested during the frame.
    pub cmds: Vec<Command>,
}

impl AppState {
    /// The Shelf's phone panel keeps the open island from closing, only
    /// while the Shelf is the page on screen.
    pub fn phone_holds_island(&self) -> bool {
        self.page == PageKind::Shelf && self.phone.as_ref().is_some_and(|p| p.holds_island())
    }

    /// A reading under way on the Script page keeps the open island from
    /// closing, while that page is on screen.
    pub fn script_holds_island(&self) -> bool {
        self.page == PageKind::Script && self.script.active
    }

    /// Something on the page on screen keeps the open island from closing.
    pub fn page_holds_island(&self) -> bool {
        self.phone_holds_island() || self.script_holds_island()
    }

    pub fn new(settings: Settings, store: Store) -> Self {
        let clip_cap = settings.clipboard_max as usize;
        let pomodoro = Pomodoro::new(settings.pomodoro.clone());
        let page = PageKind::from_key(if settings.remember_page && !store.last_page.is_empty() {
            &store.last_page
        } else {
            &settings.start_page
        });
        let mut st = Self {
            settings,
            store,
            now: 0.0,
            local: LocalDateTime::default(),
            unix: 0,
            media: None,
            lyrics: LyricsState::default(),
            pomodoro,
            volume: VolumeState::default(),
            power: PowerState::default(),
            hud: None,
            notice: None,
            clip: poni_core::cliphist::ClipHistory::new(clip_cap),
            screen: poni_core::screentime::Usage::default(),
            screen_live: None,
            screen_icons: std::collections::HashMap::new(),
            notify_logos: std::collections::HashMap::new(),
            notice_card_id: 0,
            notice_photo: None,
            notes: Default::default(),
            shelf_thumbs: HashMap::new(),
            ui: PageUiState::default(),
            accent: palette::BLUE,
            page,
            media_paused_at: None,
            pinned: false,
            aside: false,
            drag_hover: false,
            phone: None,
            paired: Vec::new(),
            pair_server: None,
            held: None,
            addons: BTreeMap::new(),
            prayer: PrayerState::default(),
            script: poni_core::script::Reader::default(),
            script_note: None,
            script_hidden: None,
            notify_access: NotifyAccess::Unknown,
            notify_can: false,
            cmds: Vec::new(),
        };
        // On ones start by checking their files (the worker reports back);
        // one without files is ready at once.
        for a in poni_core::addons::ALL {
            let on = st.settings.addon_on(a.id);
            st.addons.insert(a.id, if !on { AddonStatus::Off } else if a.downloads() { AddonStatus::Checking } else { AddonStatus::Ready });
        }
        // Last open on the Script
        // page, since switched off, or on a tab since hidden: open on the
        // first tab.
        let off = match st.page {
            PageKind::Script => !st.script_on(),
            PageKind::Notifications => !st.notifications_on(),
            PageKind::Tasks => !st.tasks_on(),
            PageKind::Clipboard => !st.clipboard_on(),
            PageKind::Screentime => !st.screentime_on(),
            PageKind::Notes => !st.notes_on(),
            PageKind::Focus => !st.focus_on(),
            p => st.tab_hidden(p),
        };
        if off {
            st.page = crate::pages::first_tab(&st);
        }
        st
    }

    /// Where the add-on `id` stands (`Off` for one this version doesn't know).
    pub fn addon(&self, id: &str) -> AddonStatus {
        self.addons.get(id).copied().unwrap_or(AddonStatus::Off)
    }

    /// The Script reader add-on is on: the Script tab shows.
    pub fn script_on(&self) -> bool {
        self.addon(poni_core::addons::SCRIPT_READER.id) == AddonStatus::Ready
    }

    /// Notifications is on: its tab shows, and new notifications from the
    /// apps picked there show on the island. A built-in add-on, on until
    /// switched off, where this copy can read them (`notify_can`).
    pub fn notifications_on(&self) -> bool {
        self.notify_can && self.addon(poni_core::addons::NOTIFICATIONS.id) == AddonStatus::Ready
    }

    /// The Meeting notes add-on is on: the Notes tab shows.
    pub fn notes_on(&self) -> bool {
        self.addon(poni_core::addons::MEETING_NOTES.id) == AddonStatus::Ready
    }

    /// The Screen time add-on is on: it counts and its tab shows.
    pub fn screentime_on(&self) -> bool {
        self.addon(poni_core::addons::SCREEN_TIME.id) == AddonStatus::Ready
    }

    /// The Clipboard add-on is on: its listener runs and its tab shows.
    pub fn clipboard_on(&self) -> bool {
        self.addon(poni_core::addons::CLIPBOARD.id) == AddonStatus::Ready
    }

    /// The Tasks add-on is on (built in, on until switched off): its tab shows.
    pub fn tasks_on(&self) -> bool {
        self.addon(poni_core::addons::TASKS.id) == AddonStatus::Ready
    }

    /// The Focus add-on is on (built in, on until switched off): its tab
    /// shows and its timer may run.
    pub fn focus_on(&self) -> bool {
        self.addon(poni_core::addons::FOCUS.id) == AddonStatus::Ready
    }

    /// Settings took this page's tab off the header's row (Music, Today or
    /// Shelf; the add-ons' pages go with their switches).
    pub fn tab_hidden(&self, p: PageKind) -> bool {
        p.hideable() && self.settings.hidden_tabs.iter().any(|k| k == p.key())
    }

    /// Prayer times is on and its places are read: the tab and reminders run.
    pub fn prayer_on(&self) -> bool {
        self.addon(poni_core::addons::PRAYER_TIMES.id) == AddonStatus::Ready && self.prayer.places.is_some()
    }

    /// The place the times are for, as the engine needs it, with the
    /// person's choice of Kemenag's or Muhammadiyah's times.
    pub fn prayer_site(&self) -> Option<Site> {
        self.prayer.place.as_ref().and_then(|p| p.site_for(self.settings.prayer.follow))
    }

    /// The prayer clock, UTC seconds (the self-test moves it).
    pub fn prayer_now(&self) -> i64 {
        self.unix + self.prayer.clock_shift
    }

    /// OK on the countdown before a prayer: it goes away until that prayer's
    /// time (`activities::prayer_countdown`). The adzan still comes then,
    /// since what rings follows the prayer clock, not the countdown. Returns
    /// the prayer put away (none when no countdown runs).
    pub fn prayer_countdown_ok(&mut self) -> Option<PrayerMoment> {
        let site = self.prayer_site()?;
        let (m, _) = poni_core::prayer::countdown(&site, self.prayer_now(), self.settings.prayer.remind_before_min)?;
        self.prayer.countdown_ok = Some(m.at);
        Some(m)
    }

    /// Show `n` now or, while a full-screen app hides the island (`hidden`), keep
    /// it for when the island is back (`held`, shown then if no older than
    /// `HELD_MAX_S`). One notice waits at a time, and one already waiting stays.
    pub fn show_or_hold(&mut self, n: Notice, hidden: bool) {
        if !hidden {
            self.notice = Some(n);
        } else if self.held.as_ref().is_none_or(|(_, at)| self.now - at > HELD_MAX_S) {
            self.held = Some((n, self.now));
        }
    }

    pub fn cmd(&mut self, c: Command) {
        self.cmds.push(c);
    }

    pub fn media_cmd(&mut self, c: MediaCmd) {
        self.cmds.push(Command::Media(c));
    }

    /// Mark the persisted store as changed.
    pub fn save(&mut self) {
        if !self.cmds.contains(&Command::SaveStore) {
            self.cmds.push(Command::SaveStore);
        }
    }

    pub fn chime(&mut self) {
        if self.settings.sounds {
            self.cmds.push(Command::Play(Sound::Chime));
        }
    }

    /// Show a notice pill for `secs` seconds.
    pub fn notify(&mut self, icon: Icon, color: Color, title: impl Into<String>, subtitle: impl Into<String>, secs: f64, action: Option<NoticeAction>) {
        self.notice = Some(Notice { icon, color, title: title.into(), title_dim: String::new(), subtitle: subtitle.into(), until: self.now + secs, action, inline: None, reveal: None });
    }

    /// Give the current notice a one-click inline action pill.
    pub fn set_notice_inline(&mut self, a: InlineAction) {
        if let Some(n) = self.notice.as_mut() {
            n.inline = Some(a);
        }
    }

    pub fn show_hud(&mut self, kind: HudKind, secs: f64) {
        self.hud = Some(Hud { kind, until: self.now + secs });
    }

    pub fn day_key(&self) -> String {
        self.local.day_key()
    }

    pub fn day_start_unix(&self) -> i64 {
        crate::clock::day_start_unix(&self.local, self.unix)
    }

    /// Accent for media UI: art colour when available, else the system accent.
    pub fn media_accent(&self) -> Color {
        self.media.as_ref().and_then(|m| m.art_color).unwrap_or(self.accent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phone_panel_holds_only_while_a_code_waits_on_the_shelf() {
        let mut st = AppState::new(Settings::default(), Store::default());
        st.phone = Some(PhoneDrop::default());
        st.page = PageKind::Shelf;
        assert!(st.phone_holds_island(), "a fresh QR code waits for a phone");
        st.page = PageKind::Media;
        assert!(!st.phone_holds_island(), "another page: nothing to scan on screen");
        st.page = PageKind::Shelf;
        // A phone finished pairing: the panel is back on its Wi-Fi code with a result.
        st.phone.as_mut().unwrap().outcome = Some(("iPhone is paired".into(), true, 8.0));
        assert!(!st.phone_holds_island());
        let pd = st.phone.as_mut().unwrap();
        pd.outcome = None;
        pd.view = PhoneView::Paired;
        assert!(!st.phone_holds_island(), "the paired list has no code to scan");
        let pd = st.phone.as_mut().unwrap();
        pd.view = PhoneView::Lan;
        pd.visitor = Some("iPhone".into());
        pd.ask = Some(PhoneAsk { id: 1, from: "iPhone".into(), files: vec![], total: 0 });
        assert!(st.phone_holds_island(), "a phone waits for an answer");
    }

    #[test]
    fn a_notice_waits_while_a_full_screen_app_hides_the_island() {
        let mut st = AppState::new(Settings::default(), Store::default());
        st.now = 100.0;
        let notice = |title: &str| Notice {
            icon: Icon::Bell,
            color: palette::GREEN,
            title: title.into(),
            title_dim: String::new(),
            subtitle: String::new(),
            until: 0.0,
            action: None,
            inline: None,
            reveal: None,
        };
        let held = |st: &AppState| st.held.as_ref().map(|(n, at)| (n.title.clone(), *at));
        // Hidden: it waits for the island instead of expiring unseen.
        st.show_or_hold(notice("goal"), true);
        assert!(st.notice.is_none());
        assert_eq!(held(&st), Some(("goal".into(), 100.0)));
        // One already waiting stays...
        st.held = Some((notice("first"), 100.0));
        st.show_or_hold(notice("goal"), true);
        assert_eq!(held(&st), Some(("first".into(), 100.0)));
        // ...unless it's too old to show any more.
        st.now = 100.0 + HELD_MAX_S + 1.0;
        st.show_or_hold(notice("goal"), true);
        assert_eq!(held(&st).map(|h| h.0), Some("goal".into()));
        // The island on screen: it shows at once.
        st.show_or_hold(notice("now"), false);
        assert_eq!(st.notice.as_ref().map(|n| n.title.as_str()), Some("now"));
    }
}
