//! A small personal task list.

use serde::{Deserialize, Serialize};

pub const MAX_TASK_LEN: usize = 200;
pub const MAX_TASKS: usize = 200;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: u64,
    pub text: String,
    pub done: bool,
    pub created_unix: i64,
    #[serde(default)]
    pub done_unix: Option<i64>,
    /// When it's due (Today's calendar), if it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<crate::agenda::Due>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskList {
    pub tasks: Vec<Task>,
    pub next_id: u64,
}

impl Default for TaskList {
    fn default() -> Self {
        Self { tasks: Vec::new(), next_id: 1 }
    }
}

impl TaskList {
    /// Add a task; returns its id, or `None` for blank text / a full list.
    pub fn add(&mut self, text: &str, now_unix: i64) -> Option<u64> {
        let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.is_empty() || self.tasks.len() >= MAX_TASKS {
            return None;
        }
        let text: String = text.chars().take(MAX_TASK_LEN).collect();
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        self.tasks.push(Task { id, text, done: false, created_unix: now_unix, done_unix: None, due: None });
        Some(id)
    }

    /// Add a task due on a day (from Today's calendar).
    pub fn add_due(&mut self, text: &str, now_unix: i64, due: crate::agenda::Due) -> Option<u64> {
        let id = self.add(text, now_unix)?;
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
            t.due = Some(due);
        }
        Some(id)
    }

    /// The tasks due on `y-m-d`, open ones first.
    pub fn due_on(&self, y: i32, m: u8, d: u8) -> Vec<&Task> {
        let mut v: Vec<&Task> = self.tasks.iter().filter(|t| t.due.is_some_and(|x| (x.year, x.month, x.day) == (y, m, d))).collect();
        v.sort_by_key(|t| (t.done, t.due.and_then(|x| x.time).map_or(-1, i32::from)));
        v
    }

    pub fn toggle(&mut self, id: u64, now_unix: i64) -> bool {
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
            // A repeating task isn't done: it's due again on its next day.
            if let Some(next) = t.due.filter(|_| !t.done).and_then(|d| d.next()) {
                t.due = Some(next);
                return true;
            }
            t.done = !t.done;
            t.done_unix = if t.done { Some(now_unix) } else { None };
            true
        } else {
            false
        }
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.tasks.len();
        self.tasks.retain(|t| t.id != id);
        self.tasks.len() != before
    }

    pub fn clear_done(&mut self) -> usize {
        let before = self.tasks.len();
        self.tasks.retain(|t| !t.done);
        before - self.tasks.len()
    }

    pub fn open_count(&self) -> usize {
        self.tasks.iter().filter(|t| !t.done).count()
    }

    /// Tasks completed at or after `day_start_unix`.
    pub fn done_since(&self, day_start_unix: i64) -> usize {
        self.tasks.iter().filter(|t| t.done && t.done_unix.is_some_and(|d| d >= day_start_unix)).count()
    }

    /// "done today / (open + done today)" — e.g. 1/3.
    pub fn today_progress(&self, day_start_unix: i64) -> (usize, usize) {
        let done = self.done_since(day_start_unix);
        (done, done + self.open_count())
    }

    /// Display order: open tasks first, those due soonest at the top (so:
    /// overdue, today, upcoming), then the ones with no date (oldest first);
    /// then today's done tasks.
    pub fn display_order(&self, day_start_unix: i64) -> Vec<&Task> {
        let mut open: Vec<&Task> = self.tasks.iter().filter(|t| !t.done).collect();
        let mut done: Vec<&Task> = self
            .tasks
            .iter()
            .filter(|t| t.done && t.done_unix.is_some_and(|d| d >= day_start_unix))
            .collect();
        let due_key = |t: &Task| t.due.map(|d| crate::datetime::days_from_civil(d.year, d.month, d.day) * 1440 + i64::from(d.time.unwrap_or(24 * 60)));
        open.sort_by_key(|t| (due_key(t).is_none(), due_key(t).unwrap_or(0), t.created_unix, t.id));
        done.sort_by_key(|t| std::cmp::Reverse(t.done_unix));
        open.extend(done);
        open
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_toggle_remove() {
        let mut l = TaskList::default();
        assert_eq!(l.add("   ", 0), None);
        let a = l.add("  buy   milk ", 10).unwrap();
        let b = l.add("ship poni", 20).unwrap();
        assert_eq!(l.tasks[0].text, "buy milk");
        assert!(l.toggle(a, 100));
        assert_eq!(l.today_progress(50), (1, 2));
        assert_eq!(l.today_progress(200), (0, 1));
        let order: Vec<u64> = l.display_order(50).iter().map(|t| t.id).collect();
        assert_eq!(order, vec![b, a]);
        assert_eq!(l.clear_done(), 1);
        assert!(l.remove(b));
        assert!(!l.remove(b));
    }

    #[test]
    fn truncates_long_text() {
        let mut l = TaskList::default();
        let id = l.add(&"x".repeat(500), 0).unwrap();
        assert_eq!(l.tasks.iter().find(|t| t.id == id).unwrap().text.len(), MAX_TASK_LEN);
    }

    #[test]
    fn ids_survive_default_zero() {
        let mut l = TaskList { tasks: vec![], next_id: 0 };
        assert_eq!(l.add("a", 0), Some(1));
        assert_eq!(l.add("b", 0), Some(2));
    }
}
