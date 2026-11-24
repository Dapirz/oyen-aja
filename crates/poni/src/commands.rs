//! Side effects requested by pages; executed by the app after each frame.

use crate::pages::PageKind;

#[derive(Clone, Debug, PartialEq)]
pub enum MediaCmd {
    PlayPause,
    Next,
    Prev,
    /// Seek to absolute position in seconds.
    Seek(f64),
    /// The pointer reached the progress bar or the back button: get ready
    /// (Apple Music's own controls are looked up before the press, not during it).
    Prepare,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    /// Gentle chime (timer finished, reminder).
    Chime,
    /// Short tick (button feedback) — currently unused.
    Tick,
}

/// Phone drop (the Shelf's phone panel).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhoneCmd {
    /// Start it (or try again after an error).
    Open,
    /// Stop it; files still coming in are dropped.
    Close,
    /// Answer the phone's question.
    Answer { id: u64, accept: bool },
}

/// Paired phones (also in the Shelf's phone panel; `services::pairdrop`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PairCmd {
    /// Show a pairing QR code in the panel.
    Begin,
    /// Back to the same-Wi-Fi code (a pairing code stops working).
    Back,
    /// Show the paired phones.
    List,
    /// Forget a paired phone (by its room on the meeting server).
    Remove(String),
}

/// Requests for Notifications (`services::notifications`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotifyCmd {
    /// Ask Windows to let the island read notifications (its question).
    Ask,
    /// Look again at what Windows allows: the page opened, and the person
    /// may have changed it in Windows' settings meanwhile.
    Check,
    /// Open Windows' settings for which apps may read notifications.
    OpenWindowsSettings,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Media(MediaCmd),
    /// Fetch these apps' icons for the Screen time page (key, program path).
    ScreenIcons(Vec<(String, String)>),
    /// Put this text on the clipboard (a Clipboard page row), without
    /// recording it again.
    CopyText(String),
    /// Open a file/folder with its default handler.
    OpenPath(String),
    /// Show a file in Explorer.
    RevealPath(String),
    /// Begin an OLE drag of these paths out of the island.
    DragOut(Vec<String>),
    /// Request a thumbnail for a shelf item (async; arrives as `AppEvent::ShelfThumb`).
    LoadThumb(String),
    /// Give the island keyboard focus for a text field.
    FocusText,
    /// The Script page's Paste: the clipboard's text, line breaks kept,
    /// becomes the script.
    ScriptPaste,
    /// Drop keyboard focus.
    BlurText,
    /// Persist `AppState::store` soon.
    SaveStore,
    /// Persist `AppState::settings` now (after a page toggled a setting).
    SaveSettings,
    /// Open the Settings window, or bring it forward (the gear, Windows).
    OpenSettings,
    SetPage(PageKind),
    /// Keep the island open even when the cursor leaves.
    SetPinned(bool),
    Play(Sound),
    /// Open a URL in the default browser.
    OpenUrl(String),
    /// The song's lyrics are wrong: show another LRCLIB copy of it.
    OtherLyrics,
    /// Start with Windows on or off (the Settings page's switch).
    SetAutostart(bool),
    Phone(PhoneCmd),
    Pair(PairCmd),
    /// Get one of Prayer times' online city files ("ed", "near/55_-5"; the
    /// answer: `AppEvent::Cities`).
    GetCities(String),
    /// Notifications.
    Notify(NotifyCmd),
    /// The Notes page's buttons (Meeting notes).
    Notes(NotesCmd),
    /// Open the app with this AppUserModelID (a recent notification's).
    OpenApp(String),
    /// Settings > Meeting notes: the microphone menu.
    PickMic,
    /// Settings > Tabs: the Opens on menu.
    PickStartPage,
    /// Settings > About > Backup: write one, or put the newest one back.
    BackupExport,
    BackupImport,
    /// What the tray's menu used to do, from Settings (Apple Music's token).
    Menu(crate::tray::MenuCmd),
}

/// The Notes page's buttons (the Meeting notes add-on).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotesCmd {
    /// Take notes: the recording starts.
    Start,
    /// Stop: the meeting is over, its notes get written.
    Stop,
    /// Try again: hand the kept recording over again.
    Retry,
    /// Throw the kept recording away.
    Discard,
    /// Open the notes' file, or the notes folder.
    Open,
    Folder,
    /// Copy the notes (without the transcript) as Markdown.
    Copy,
    /// Back to Take notes.
    Done,
    /// The Gemini key: the one on the clipboard is saved (sealed), or the
    /// saved one goes.
    PasteKey,
    RemoveKey,
    /// A saved note from the list: open it, or to the Recycle Bin.
    OpenSaved(usize),
    RecycleSaved(usize),
}
