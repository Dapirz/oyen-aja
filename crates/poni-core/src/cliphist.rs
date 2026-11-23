//! Clipboard history (the Clipboard add-on): the text copied lately,
//! newest first, with pinned entries that never leave by themselves.
//!
//! How many entries it keeps and for how long are settings
//! (`Settings::clipboard_max`, `Settings::clipboard_keep_s`). The app saves
//! the history sealed for the Windows account (`services::secret`), so it
//! outlives a restart for as long as it's set to keep entries.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const DEFAULT_CAP: usize = 25;
pub const MAX_CHARS: usize = 20_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipItem {
    pub text: String,
    pub at_unix: i64,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Clone, Debug)]
pub struct ClipHistory {
    items: VecDeque<ClipItem>,
    cap: usize,
    /// Bumped on every change, so the app knows when to save.
    rev: u64,
}

impl Default for ClipHistory {
    fn default() -> Self {
        Self::new(DEFAULT_CAP)
    }
}

impl ClipHistory {
    pub fn new(cap: usize) -> Self {
        Self { items: VecDeque::new(), cap: cap.max(1), rev: 0 }
    }

    /// The history from saved entries (newest first), kept to `cap`.
    pub fn from_items(items: Vec<ClipItem>, cap: usize) -> Self {
        let mut h = Self { items: items.into_iter().filter(|it| !it.text.trim().is_empty()).collect(), cap: cap.max(1), rev: 0 };
        h.evict();
        h
    }

    /// A change counter: different after any change since it was read.
    pub fn rev(&self) -> u64 {
        self.rev
    }

    /// Keep at most `cap` entries (pinned ones aside), dropping the oldest.
    pub fn set_cap(&mut self, cap: usize) {
        self.cap = cap.max(1);
        if self.evict() {
            self.rev += 1;
        }
    }

    /// Drop the oldest unpinned entries beyond the cap; true when any went.
    fn evict(&mut self) -> bool {
        let mut gone = false;
        while self.items.len() > self.cap {
            match self.items.iter().rposition(|it| !it.pinned) {
                Some(i) => {
                    self.items.remove(i);
                    gone = true;
                }
                None => break,
            }
        }
        gone
    }

    /// Drop unpinned entries copied more than `keep_s` seconds before
    /// `now_unix` (None: keep them for good). True when any went.
    pub fn prune(&mut self, now_unix: i64, keep_s: Option<u32>) -> bool {
        let Some(keep) = keep_s else { return false };
        let before = self.items.len();
        self.items.retain(|it| it.pinned || now_unix - it.at_unix <= i64::from(keep));
        let gone = self.items.len() != before;
        if gone {
            self.rev += 1;
        }
        gone
    }

    /// Record a copy. Blank text is ignored; a repeat moves to the front.
    pub fn push(&mut self, text: &str, now_unix: i64) -> bool {
        if text.trim().is_empty() {
            return false;
        }
        let text: String = if text.chars().count() > MAX_CHARS { text.chars().take(MAX_CHARS).collect() } else { text.to_string() };
        let mut pinned = false;
        if let Some(i) = self.items.iter().position(|it| it.text == text) {
            pinned = self.items.remove(i).map(|it| it.pinned).unwrap_or(false);
        }
        self.items.push_front(ClipItem { text, at_unix: now_unix, pinned });
        self.evict();
        self.rev += 1;
        true
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn get(&self, i: usize) -> Option<&ClipItem> {
        self.items.get(i)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ClipItem> {
        self.items.iter()
    }

    /// Every entry, newest first (for saving).
    pub fn items(&self) -> Vec<ClipItem> {
        self.items.iter().cloned().collect()
    }

    pub fn remove(&mut self, i: usize) -> Option<ClipItem> {
        let it = self.items.remove(i);
        if it.is_some() {
            self.rev += 1;
        }
        it
    }

    /// Put `item` back at `i` (Undo), or at the end past the last.
    pub fn insert(&mut self, i: usize, item: ClipItem) {
        let i = i.min(self.items.len());
        self.items.insert(i, item);
        self.rev += 1;
    }

    pub fn toggle_pin(&mut self, i: usize) {
        if let Some(it) = self.items.get_mut(i) {
            it.pinned = !it.pinned;
            self.rev += 1;
        }
    }

    /// Clear everything except pinned items.
    pub fn clear(&mut self) {
        self.items.retain(|it| it.pinned);
        self.rev += 1;
    }

    /// Forget every entry holding `secret` (a token the app read off the
    /// clipboard). True when any went.
    pub fn forget(&mut self, secret: &str) -> bool {
        if secret.is_empty() {
            return false;
        }
        let before = self.items.len();
        self.items.retain(|it| !it.text.contains(secret));
        let gone = self.items.len() != before;
        if gone {
            self.rev += 1;
        }
        gone
    }
}

/// Single-line preview: whitespace runs collapsed, truncated with an ellipsis.
pub fn preview(text: &str, max_chars: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        collapsed
    } else {
        let mut s: String = collapsed.chars().take(max_chars.saturating_sub(1)).collect();
        s.push('…');
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_dedupes_and_caps() {
        let mut h = ClipHistory::new(3);
        assert!(!h.push("   ", 0));
        h.push("a", 1);
        h.push("b", 2);
        h.push("a", 3);
        assert_eq!(h.len(), 2);
        assert_eq!(h.get(0).unwrap().text, "a");
        h.push("c", 4);
        h.push("d", 5);
        assert_eq!(h.len(), 3);
        assert_eq!(h.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(), vec!["d", "c", "a"]);
    }

    #[test]
    fn pinned_survive_eviction_and_clear() {
        let mut h = ClipHistory::new(2);
        h.push("keep", 0);
        h.toggle_pin(0);
        h.push("x", 1);
        h.push("y", 2);
        assert!(h.iter().any(|i| i.text == "keep"));
        h.clear();
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn old_entries_go_but_pinned_ones_stay() {
        let mut h = ClipHistory::new(10);
        h.push("old", 0);
        h.push("pinned old", 0);
        h.toggle_pin(0);
        h.push("new", 3_000);
        let rev = h.rev();
        assert!(h.prune(4_000, Some(3_600)));
        assert_ne!(h.rev(), rev);
        assert_eq!(h.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(), vec!["new", "pinned old"]);
        // Kept for good: nothing goes.
        assert!(!h.prune(1_000_000, None));
    }

    #[test]
    fn a_smaller_cap_drops_the_oldest_and_saved_entries_come_back() {
        let mut h = ClipHistory::new(5);
        for (i, t) in ["a", "b", "c", "d"].iter().enumerate() {
            h.push(t, i as i64);
        }
        h.set_cap(2);
        assert_eq!(h.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(), vec!["d", "c"]);
        let json = serde_json::to_string(&h.items()).unwrap();
        let back = ClipHistory::from_items(serde_json::from_str(&json).unwrap(), 5);
        assert_eq!(back.items(), h.items());
    }

    #[test]
    fn a_token_read_off_the_clipboard_is_forgotten() {
        let mut h = ClipHistory::new(5);
        h.push("hello", 0);
        h.push("token abc123", 1);
        assert!(h.forget("abc123"));
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn preview_collapses() {
        assert_eq!(preview("  hello\n\n  world  ", 50), "hello world");
        assert_eq!(preview("abcdefgh", 5), "abcd…");
    }
}
