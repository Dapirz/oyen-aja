//! A backup of what Hihu keeps, to take to another PC or a fresh Windows:
//! the settings and the data (tasks, the calendar's events, the Shelf's list,
//! the Script, the Focus count), in one JSON file. Keys sealed for this PC's
//! Windows account (Gemini's, Apple Music's) and the paired phones' secrets
//! stay out: they can't be read anywhere else.

use crate::settings::Settings;
use crate::store::Store;
use serde::{Deserialize, Serialize};

/// What the file says it is, so another JSON file isn't taken for one.
pub const KIND: &str = "hihu-backup";

#[derive(Serialize, Deserialize)]
pub struct Backup {
    pub kind: String,
    pub version: u32,
    /// When it was made (unix seconds).
    pub made: i64,
    pub settings: Settings,
    pub store: Store,
}

/// The backup's text; Err rather than an empty file when it can't be
/// written out (audit H3).
pub fn make(settings: &Settings, store: &Store, now: i64) -> Result<String, String> {
    let b = Backup { kind: KIND.into(), version: 1, made: now, settings: settings.clone(), store: store.clone() };
    serde_json::to_string_pretty(&b).map_err(|e| e.to_string())
}

/// A backup read back, or why not (in plain words).
pub fn read(text: &str) -> Result<Backup, &'static str> {
    let b: Backup = serde_json::from_str(text).map_err(|_| "That file isn't a Hihu backup")?;
    if b.kind != KIND {
        return Err("That file isn't a Hihu backup");
    }
    Ok(Backup { settings: b.settings.sanitized(), ..b })
}

/// "Hihu backup 2026-11-24.json".
pub fn file_name(y: i32, m: u8, d: u8) -> String {
    format!("Hihu backup {y:04}-{m:02}-{d:02}.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backup_comes_back_whole() {
        let mut s = Settings::default();
        s.notes_lang = "en".into();
        let mut st = Store::default();
        st.tasks.add("Bayar listrik", 1);
        st.agenda.add("Rapat", 2026, 11, 24, crate::agenda::Every::Weeks(1), Some(10), None);
        let text = make(&s, &st, 99).unwrap();
        let b = read(&text).unwrap();
        assert_eq!(b.settings.notes_lang, "en");
        assert_eq!(b.store.tasks.tasks[0].text, "Bayar listrik");
        assert_eq!(b.store.agenda.events[0].title, "Rapat");
        assert!(read(r#"{"kind":"other"}"#).is_err());
        assert!(read("not json").is_err());
        assert_eq!(file_name(2026, 11, 4), "Hihu backup 2026-11-04.json");
    }
}
