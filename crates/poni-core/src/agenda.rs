//! The Today page's calendar: the person's own events (once, or again every
//! few days, weeks, months or years), Indonesia's national holidays (the
//! red dates, from the government's SKB 3 Menteri) and the countdown to the
//! next ones. All of it on this PC: the events are saved with the tasks, the
//! holidays are written in here.

use crate::datetime::days_from_civil;
use serde::{Deserialize, Serialize};

/// How often an event comes again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Every {
    /// Only on its day.
    #[default]
    Once,
    Days(u16),
    Weeks(u16),
    Months(u16),
    Years(u16),
}

impl Every {
    /// "Once", "Every week", "Every 2 months".
    pub fn label(self) -> String {
        let (n, one, many) = match self {
            Every::Once => return "Once".into(),
            Every::Days(n) => (n, "day", "days"),
            Every::Weeks(n) => (n, "week", "weeks"),
            Every::Months(n) => (n, "month", "months"),
            Every::Years(n) => (n, "year", "years"),
        };
        if n <= 1 {
            format!("Every {one}")
        } else {
            format!("Every {n} {many}")
        }
    }

    /// Short, for a small button: "Once", "Daily", "Weekly", "2 wks".
    pub fn short(self) -> String {
        let (n, one, many) = match self {
            Every::Once => return "Once".into(),
            Every::Days(n) => (n, "Daily", "days"),
            Every::Weeks(n) => (n, "Weekly", "wks"),
            Every::Months(n) => (n, "Monthly", "mos"),
            Every::Years(n) => (n, "Yearly", "yrs"),
        };
        if n <= 1 {
            one.into()
        } else {
            format!("{n} {many}")
        }
    }

    /// The next kind (Once → days → weeks → months → years → Once), keeping
    /// the count.
    pub fn next_kind(self) -> Every {
        let n = self.count();
        match self {
            Every::Once => Every::Days(n),
            Every::Days(_) => Every::Weeks(n),
            Every::Weeks(_) => Every::Months(n),
            Every::Months(_) => Every::Years(n),
            Every::Years(_) => Every::Once,
        }
    }

    pub fn count(self) -> u16 {
        match self {
            Every::Once => 1,
            Every::Days(n) | Every::Weeks(n) | Every::Months(n) | Every::Years(n) => n.max(1),
        }
    }

    /// The same kind every `n` (1..=99).
    pub fn with_count(self, n: u16) -> Every {
        let n = n.clamp(1, 99);
        match self {
            Every::Once => Every::Once,
            Every::Days(_) => Every::Days(n),
            Every::Weeks(_) => Every::Weeks(n),
            Every::Months(_) => Every::Months(n),
            Every::Years(_) => Every::Years(n),
        }
    }
}

/// One of the person's events.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: u64,
    pub title: String,
    /// Its first day.
    pub year: i32,
    pub month: u8,
    pub day: u8,
    #[serde(default)]
    pub every: Every,
    /// Its time of day (minutes after midnight); None: all day.
    #[serde(default)]
    pub time: Option<u16>,
    /// A reminder this many minutes before (0: at its time; an all-day
    /// event's comes at [`MORNING`] on its day); None: none.
    #[serde(default)]
    pub remind: Option<u16>,
}

/// When an all-day event's reminder comes: 07:00.
pub const MORNING: u16 = 7 * 60;

/// The reminder choices the Today page cycles through (minutes before).
pub const REMINDERS: [Option<u16>; 5] = [None, Some(0), Some(10), Some(30), Some(60)];

/// A reminder's short name: "Off", "On time", "10m", "1h".
pub fn remind_label(r: Option<u16>) -> String {
    match r {
        None => "Off".into(),
        Some(0) => "On time".into(),
        Some(m) if m % 60 == 0 => format!("{}h", m / 60),
        Some(m) => format!("{m}m"),
    }
}

/// "09:30 Rapat tim" (or "9.30 …"): the time at the start of what's typed,
/// and the rest as the title.
pub fn split_time(text: &str) -> (Option<u16>, &str) {
    let t = text.trim_start();
    let end = t.find(char::is_whitespace).unwrap_or(t.len());
    let (head, rest) = t.split_at(end);
    let mut parts = head.splitn(2, [':', '.']);
    let (Some(h), Some(m)) = (parts.next(), parts.next()) else { return (None, text) };
    match (h.parse::<u16>(), m.parse::<u16>()) {
        (Ok(h), Ok(m)) if h < 24 && m < 60 && m.to_string().len() <= 2 && (1..=2).contains(&h.to_string().len()) && !rest.trim().is_empty() => (Some(h * 60 + m), rest.trim()),
        _ => (None, text),
    }
}

/// "09:30".
pub fn fmt_time(min: u16) -> String {
    format!("{:02}:{:02}", min / 60, min % 60)
}

impl Event {
    /// Whether it falls on `y-m-d`.
    pub fn on(&self, y: i32, m: u8, d: u8) -> bool {
        let start = days_from_civil(self.year, self.month, self.day);
        let at = days_from_civil(y, m, d);
        if at < start {
            return false;
        }
        let months = (y - self.year) as i64 * 12 + m as i64 - self.month as i64;
        match self.every {
            Every::Once => at == start,
            Every::Days(n) => (at - start) % n.max(1) as i64 == 0,
            Every::Weeks(n) => (at - start) % (7 * n.max(1) as i64) == 0,
            // A month without its day (the 31st in April) has none.
            Every::Months(n) => d == self.day && months % n.max(1) as i64 == 0,
            Every::Years(n) => d == self.day && m == self.month && (y - self.year) as i64 % n.max(1) as i64 == 0,
        }
    }
}

/// The person's events, saved with the tasks.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Agenda {
    pub events: Vec<Event>,
    pub next_id: u64,
}

impl Agenda {
    /// Add an event from what was typed: a time at its start ("09:30 …")
    /// becomes its time, else `time` (the page's time picker).
    pub fn add(&mut self, typed: &str, y: i32, m: u8, d: u8, every: Every, remind: Option<u16>, time: Option<u16>) -> Option<u64> {
        let (typed_time, title) = split_time(typed);
        let time = typed_time.or(time).map(|t| t.min(23 * 60 + 59));
        let title: String = title.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(80).collect();
        if title.is_empty() {
            return None;
        }
        self.next_id += 1;
        self.events.push(Event { id: self.next_id, title, year: y, month: m, day: d, every, time, remind });
        Some(self.next_id)
    }

    /// The reminders due after minute `after` up to and at minute `upto`
    /// (minutes counted from 1970-01-01 local time), soonest first.
    pub fn due(&self, after: i64, upto: i64) -> Vec<(&Event, i64)> {
        let mut out = Vec::new();
        // An hour's lead can reach into the day before.
        for day in (after.div_euclid(1440))..=(upto.div_euclid(1440) + 1) {
            let (y, m, d) = crate::datetime::civil_from_days(day);
            for e in self.events.iter().filter(|e| e.remind.is_some() && e.on(y, m, d)) {
                let at = day * 1440 + i64::from(e.time.unwrap_or(MORNING)) - if e.time.is_some() { i64::from(e.remind.unwrap_or(0)) } else { 0 };
                if at > after && at <= upto {
                    out.push((e, at));
                }
            }
        }
        out.sort_by_key(|(_, at)| *at);
        out
    }

    /// The next reminder after minute `after` (local), within a year.
    pub fn next_reminder(&self, after: i64) -> Option<i64> {
        // A week at a time, so a full year's look runs only when there's nothing soon.
        let mut from = after;
        for _ in 0..53 {
            let to = from + 7 * 1440;
            if let Some((_, at)) = self.due(from, to).first() {
                return Some(*at);
            }
            from = to;
        }
        None
    }

    pub fn remove(&mut self, id: u64) {
        self.events.retain(|e| e.id != id);
    }

    /// The events on `y-m-d`, in the order they were added.
    pub fn on(&self, y: i32, m: u8, d: u8) -> Vec<&Event> {
        self.events.iter().filter(|e| e.on(y, m, d)).collect()
    }
}

/// A national holiday (a red date).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Holiday {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub name: &'static str,
    /// A cuti bersama (a collective leave day around a holiday), not a holiday itself.
    pub leave: bool,
}

const fn h(year: i32, month: u8, day: u8, name: &'static str) -> Holiday {
    Holiday { year, month, day, name, leave: false }
}

const fn c(year: i32, month: u8, day: u8, name: &'static str) -> Holiday {
    Holiday { year, month, day, name, leave: true }
}

/// Indonesia's national holidays and cuti bersama, as the SKB 3 Menteri set
/// them (setneg.go.id: Nomor 1497/2/5 Tahun 2025 for 2026, Nomor 1205/3/2
/// Tahun 2026 for 2027). Later years: only the fixed dates, until they're
/// added here.
pub const HOLIDAYS: &[Holiday] = &[
    h(2026, 1, 1, "Tahun Baru Masehi"),
    h(2026, 1, 16, "Isra Mikraj"),
    c(2026, 2, 16, "Cuti bersama Imlek"),
    h(2026, 2, 17, "Tahun Baru Imlek"),
    c(2026, 3, 18, "Cuti bersama Nyepi"),
    h(2026, 3, 19, "Hari Suci Nyepi"),
    c(2026, 3, 20, "Cuti bersama Idulfitri"),
    h(2026, 3, 21, "Idulfitri"),
    h(2026, 3, 22, "Idulfitri"),
    c(2026, 3, 23, "Cuti bersama Idulfitri"),
    c(2026, 3, 24, "Cuti bersama Idulfitri"),
    h(2026, 4, 3, "Wafat Yesus Kristus"),
    h(2026, 4, 5, "Paskah"),
    h(2026, 5, 1, "Hari Buruh"),
    h(2026, 5, 14, "Kenaikan Yesus Kristus"),
    c(2026, 5, 15, "Cuti bersama Kenaikan"),
    h(2026, 5, 27, "Iduladha"),
    c(2026, 5, 28, "Cuti bersama Iduladha"),
    h(2026, 5, 31, "Hari Raya Waisak"),
    h(2026, 6, 1, "Hari Lahir Pancasila"),
    h(2026, 6, 16, "Tahun Baru Islam"),
    h(2026, 8, 17, "Hari Kemerdekaan"),
    h(2026, 8, 25, "Maulid Nabi"),
    c(2026, 12, 24, "Cuti bersama Natal"),
    h(2026, 12, 25, "Natal"),
    h(2027, 1, 1, "Tahun Baru Masehi"),
    h(2027, 1, 5, "Isra Mikraj"),
    c(2027, 2, 5, "Cuti bersama Imlek"),
    h(2027, 2, 6, "Tahun Baru Imlek"),
    h(2027, 3, 8, "Hari Suci Nyepi"),
    c(2027, 3, 9, "Cuti bersama Idulfitri"),
    h(2027, 3, 10, "Idulfitri"),
    h(2027, 3, 11, "Idulfitri"),
    c(2027, 3, 12, "Cuti bersama Idulfitri"),
    c(2027, 3, 15, "Cuti bersama Idulfitri"),
    c(2027, 3, 25, "Cuti bersama Wafat Yesus"),
    h(2027, 3, 26, "Wafat Yesus Kristus"),
    h(2027, 3, 28, "Paskah"),
    h(2027, 5, 1, "Hari Buruh"),
    h(2027, 5, 6, "Kenaikan Yesus Kristus"),
    h(2027, 5, 17, "Iduladha"),
    c(2027, 5, 18, "Cuti bersama Iduladha"),
    c(2027, 5, 19, "Cuti bersama Waisak"),
    h(2027, 5, 20, "Hari Raya Waisak"),
    h(2027, 6, 1, "Hari Lahir Pancasila"),
    h(2027, 6, 6, "Tahun Baru Islam"),
    h(2027, 8, 15, "Maulid Nabi"),
    h(2027, 8, 17, "Hari Kemerdekaan"),
    c(2027, 12, 24, "Cuti bersama Natal"),
    h(2027, 12, 25, "Natal"),
    h(2027, 12, 26, "Isra Mikraj"),
];

/// The last year [`HOLIDAYS`] has in full.
pub const LISTED_UNTIL: i32 = 2027;

/// The holidays that don't move, for the years past [`LISTED_UNTIL`].
const FIXED: [(u8, u8, &str); 5] = [(1, 1, "Tahun Baru Masehi"), (5, 1, "Hari Buruh"), (6, 1, "Hari Lahir Pancasila"), (8, 17, "Hari Kemerdekaan"), (12, 25, "Natal")];

/// The holiday or cuti bersama on `y-m-d`, if any (a holiday before a leave day).
pub fn holiday(y: i32, m: u8, d: u8) -> Option<Holiday> {
    if y > LISTED_UNTIL {
        return FIXED.iter().find(|f| f.0 == m && f.1 == d).map(|f| h(y, m, d, f.2));
    }
    let on = |x: &&Holiday| x.year == y && x.month == m && x.day == d;
    HOLIDAYS.iter().filter(on).find(|x| !x.leave).or_else(|| HOLIDAYS.iter().find(on)).copied()
}

/// The next national holidays from `today` (not cuti bersama), within
/// `days` days, at most `max`, each once (a two-day Idulfitri counts once,
/// by its first day): the holiday and how many days are left (0: today).
pub fn upcoming(today: (i32, u8, u8), days: i64, max: usize) -> Vec<(Holiday, i64)> {
    let start = days_from_civil(today.0, today.1, today.2);
    let mut out: Vec<(Holiday, i64)> = Vec::new();
    for i in 0..=days {
        let (y, m, d) = crate::datetime::civil_from_days(start + i);
        if let Some(hd) = holiday(y, m, d).filter(|x| !x.leave) {
            if out.last().is_some_and(|(p, left)| p.name == hd.name && *left == i - 1) {
                continue;
            }
            out.push((hd, i));
            if out.len() >= max {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(every: Every) -> Event {
        Event { id: 1, title: "x".into(), year: 2026, month: 1, day: 31, every, time: None, remind: None }
    }

    #[test]
    fn events_come_again_as_set() {
        assert!(ev(Every::Once).on(2026, 1, 31) && !ev(Every::Once).on(2026, 2, 1));
        assert!(!ev(Every::Days(1)).on(2026, 1, 30), "not before it starts");
        assert!(ev(Every::Days(3)).on(2026, 2, 3) && !ev(Every::Days(3)).on(2026, 2, 2));
        assert!(ev(Every::Weeks(2)).on(2026, 2, 14) && !ev(Every::Weeks(2)).on(2026, 2, 7));
        // Every month on the 31st: none in February, March's 31st.
        assert!(!ev(Every::Months(1)).on(2026, 2, 28) && ev(Every::Months(1)).on(2026, 3, 31));
        assert!(ev(Every::Months(2)).on(2026, 3, 31) && !ev(Every::Months(2)).on(2026, 4, 30));
        assert!(ev(Every::Years(1)).on(2027, 1, 31) && !ev(Every::Years(2)).on(2027, 1, 31));
        assert_eq!(Every::Weeks(2).label(), "Every 2 weeks");
        assert_eq!(Every::Weeks(2).short(), "2 wks");
        assert_eq!(Every::Days(1).short(), "Daily");
        assert_eq!(Every::Months(1).label(), "Every month");
        assert_eq!(Every::Once.next_kind(), Every::Days(1));
        assert_eq!(Every::Days(3).next_kind(), Every::Weeks(3));
    }

    #[test]
    fn the_agenda_keeps_its_events() {
        let mut a = Agenda::default();
        assert_eq!(a.add("  ", 2026, 11, 24, Every::Once, None, None), None);
        let id = a.add("Rapat  tim", 2026, 11, 24, Every::Weeks(1), None, None).unwrap();
        assert_eq!(a.on(2026, 12, 1)[0].title, "Rapat tim");
        a.remove(id);
        assert!(a.on(2026, 12, 1).is_empty());
        let old: Agenda = serde_json::from_str(r#"{"events":[{"id":3,"title":"a","year":2026,"month":1,"day":2}]}"#).unwrap();
        assert_eq!(old.events[0].every, Every::Once);
    }

    #[test]
    fn times_typed_first_and_reminders_when_due() {
        assert_eq!(split_time("09:30 Rapat tim"), (Some(570), "Rapat tim"));
        assert_eq!(split_time("7.05 Bangun"), (Some(425), "Bangun"));
        assert_eq!(split_time("25:00 x").0, None);
        assert_eq!(split_time("12:30").0, None, "a time alone is the title");
        assert_eq!(split_time("Rapat 09:30").0, None);
        assert_eq!(remind_label(Some(60)), "1h");
        let mut a = Agenda::default();
        a.add("09:30 Rapat", 2026, 11, 24, Every::Days(1), Some(30), Some(600)).unwrap();
        a.add("Ulang tahun", 2026, 11, 25, Every::Years(1), Some(10), None).unwrap();
        // A time from the picker, when none is typed.
        let id = a.add("Picked", 2027, 1, 2, Every::Once, None, Some(615)).unwrap();
        assert_eq!(a.events.iter().find(|e| e.id == id).and_then(|e| e.time), Some(615));
        a.remove(id);
        a.add("No reminder", 2026, 11, 24, Every::Once, None, None).unwrap();
        let day = days_from_civil(2026, 11, 24) * 1440;
        // 09:00 today: the meeting's reminder.
        assert_eq!(a.next_reminder(day + 8 * 60), Some(day + 9 * 60));
        let due = a.due(day + 8 * 60, day + 9 * 60);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].0.title, "Rapat");
        // After it: the birthday's morning tomorrow, at 07:00 (an all-day event).
        assert_eq!(a.next_reminder(day + 9 * 60), Some(day + 1440 + MORNING as i64));
        assert_eq!(fmt_time(570), "09:30");
    }

    #[test]
    fn red_dates_and_their_countdown() {
        assert_eq!(holiday(2026, 8, 17).map(|x| x.name), Some("Hari Kemerdekaan"));
        assert!(holiday(2026, 12, 24).is_some_and(|x| x.leave));
        assert_eq!(holiday(2030, 12, 25).map(|x| x.name), Some("Natal"));
        assert_eq!(holiday(2030, 3, 10), None);
        // From 24 Nov 2026: Natal in 31 days, then New Year, then Isra Mikraj.
        let next = upcoming((2026, 11, 24), 60, 3);
        assert_eq!(next.iter().map(|(x, d)| (x.name, *d)).collect::<Vec<_>>(), [("Natal", 31), ("Tahun Baru Masehi", 38), ("Isra Mikraj", 42)]);
        // Idulfitri's two days count once.
        let lebaran = upcoming((2026, 3, 20), 5, 5);
        assert_eq!(lebaran.iter().filter(|(x, _)| x.name == "Idulfitri").count(), 1);
        for x in HOLIDAYS {
            assert!(days_from_civil(x.year, x.month, x.day) > 0 && (1..=12).contains(&x.month) && (1..=31).contains(&x.day));
        }
    }
}
