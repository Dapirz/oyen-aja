//! Screen time (the Screen time add-on): how long each app was in front,
//! day by day, for the last [`KEEP_DAYS`] days.
//!
//! The app adds a stretch of time to the app that was in front when it
//! ends: another window came to the front, the PC was locked or its screen
//! went off, or nobody touched it for a while. Days are local dates
//! ("2026-11-23"), so they sort as text.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Days kept: today and the six before it.
pub const KEEP_DAYS: usize = 7;

/// One app's time on one day.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppTime {
    /// The name people know it by ("Google Chrome").
    pub name: String,
    /// Where its program is, for its icon.
    #[serde(default)]
    pub path: String,
    pub secs: u64,
}

/// Every day kept: date -> app (its program's name, lowercase) -> time.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub days: BTreeMap<String, BTreeMap<String, AppTime>>,
}

/// A local date as the days are keyed.
pub fn day_key(year: i32, month: u8, day: u8) -> String {
    format!("{year:04}-{month:02}-{day:02}")
}

impl Usage {
    /// Add `secs` of `key` (named `name`, its program at `path`) to `day`.
    pub fn add(&mut self, day: &str, key: &str, name: &str, path: &str, secs: u64) {
        if secs == 0 || key.is_empty() {
            return;
        }
        let app = self.days.entry(day.to_string()).or_default().entry(key.to_string()).or_default();
        app.secs += secs;
        if !name.is_empty() {
            app.name = name.to_string();
        }
        if !path.is_empty() {
            app.path = path.to_string();
        }
    }

    /// Keep only the last [`KEEP_DAYS`] days up to `today` (and nothing
    /// dated after it, from a clock set wrong). True when any went.
    pub fn prune(&mut self, today: &str) -> bool {
        let before = self.days.len();
        self.days.retain(|d, _| d.as_str() <= today);
        while self.days.len() > KEEP_DAYS {
            let Some(first) = self.days.keys().next().cloned() else { break };
            self.days.remove(&first);
        }
        self.days.len() != before
    }

    /// `day`'s apps, most time first (ties by name).
    pub fn apps(&self, day: &str) -> Vec<(String, AppTime)> {
        let mut v: Vec<(String, AppTime)> = self.days.get(day).map(|m| m.iter().map(|(k, a)| (k.clone(), a.clone())).collect()).unwrap_or_default();
        v.sort_by(|a, b| b.1.secs.cmp(&a.1.secs).then_with(|| a.1.name.cmp(&b.1.name)));
        v
    }

    /// `day`'s whole screen time.
    pub fn total(&self, day: &str) -> u64 {
        self.days.get(day).map_or(0, |m| m.values().map(|a| a.secs).sum())
    }
}

/// "3h 24m", "45m", "under a minute" (`secs` 0: "0m").
pub fn fmt_dur(secs: u64) -> String {
    let m = secs / 60;
    match (m / 60, m % 60) {
        (0, 0) if secs > 0 => "<1m".to_string(),
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_adds_up_per_app_and_day() {
        let mut u = Usage::default();
        u.add("2026-11-23", "chrome.exe", "Google Chrome", "C:\\chrome.exe", 600);
        u.add("2026-11-23", "code.exe", "Visual Studio Code", "", 1200);
        u.add("2026-11-23", "chrome.exe", "Google Chrome", "", 300);
        u.add("2026-11-22", "chrome.exe", "Google Chrome", "", 60);
        u.add("2026-11-23", "idle.exe", "Nothing", "", 0);
        let apps = u.apps("2026-11-23");
        assert_eq!(apps.iter().map(|(k, a)| (k.as_str(), a.secs)).collect::<Vec<_>>(), [("code.exe", 1200), ("chrome.exe", 900)]);
        assert_eq!(apps[1].1.path, "C:\\chrome.exe", "a later stretch without a path keeps the one known");
        assert_eq!(u.total("2026-11-23"), 2100);
        assert_eq!(u.total("2026-11-21"), 0);
    }

    #[test]
    fn a_week_is_kept() {
        let mut u = Usage::default();
        for d in 10..=20 {
            u.add(&day_key(2026, 11, d), "a.exe", "A", "", 60);
        }
        u.add("2027-01-01", "a.exe", "A", "", 60);
        assert!(u.prune(&day_key(2026, 11, 20)));
        assert_eq!(u.days.keys().cloned().collect::<Vec<_>>(), (14..=20).map(|d| day_key(2026, 11, d)).collect::<Vec<_>>());
        assert!(!u.prune(&day_key(2026, 11, 20)));
    }

    #[test]
    fn durations_read_short() {
        assert_eq!(fmt_dur(0), "0m");
        assert_eq!(fmt_dur(30), "<1m");
        assert_eq!(fmt_dur(45 * 60), "45m");
        assert_eq!(fmt_dur(2 * 3600), "2h");
        assert_eq!(fmt_dur(3 * 3600 + 24 * 60 + 59), "3h 24m");
    }

    #[test]
    fn it_saves_and_comes_back() {
        let mut u = Usage::default();
        u.add("2026-11-23", "chrome.exe", "Google Chrome", "C:\\chrome.exe", 600);
        let back: Usage = serde_json::from_str(&serde_json::to_string(&u).unwrap()).unwrap();
        assert_eq!(back, u);
    }
}
