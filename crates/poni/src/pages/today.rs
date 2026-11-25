//! Today page: a month's calendar, and the day picked in it.
//!
//! On the left the month (Monday first): today filled, the day picked
//! ringed, Indonesia's national holidays and Sundays in red (a cuti bersama
//! in a softer red), a dot under a day with an event of the person's own;
//! the arrows go to the month before or after. On the right the countdown
//! to the next red dates (a few, when several come soon), then the day
//! picked: its holiday, its events (each can go), and a line to add one,
//! once or again every so many days, weeks, months or years. A time typed
//! first ("09:30 Rapat") is the event's time; the bell says when to be
//! reminded (on the island, as a notice: `App::agenda_*`).
//!
//! Everything is on this PC (`poni_core::agenda`; the events are saved
//! with the tasks). It asks for no frames of its own: the date moves at
//! midnight ([`next_wake`]).

use crate::gfx::{TextStyle, Weight};
use crate::icons::Icon;
use crate::state::AppState;
use crate::ui::{wid, wid_n, PillStyle, Ui, PILL_SMALL};
use poni_core::agenda::{self, Every};
use poni_core::datetime::{civil_from_days, days_from_civil, weekday_from_days, LocalDateTime};
use poni_core::geom::{palette, Color, Point, Rect};

const HEIGHT: f32 = 212.0;
/// The month: a cell at most this wide and tall.
const CELL_W_MAX: f32 = 38.0;
const CELL_H: f32 = 26.0;
const TITLE_H: f32 = 24.0;
const INITIALS_H: f32 = 16.0;
const COL_GAP: f32 = 22.0;
/// The countdown: red dates within this many days, at most this many.
const COUNTDOWN_DAYS: i64 = 60;
const COUNTDOWN_MAX: usize = 3;
const LINE_H: f32 = 18.0;
const EVENT_H: f32 = 22.0;
/// The time picker's step (minutes), by a button or a wheel notch.
const TIME_STEP: i32 = 5;
const MAX_TITLE: usize = 80;

const MONTH_LONG: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const MONTH_SHORT: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKDAY_SHORT: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
/// The Hijri months, short enough for a day's cell.
const HIJRI_SHORT: [&str; 12] = ["Muharram", "Safar", "R. Awal", "R. Akhir", "J. Awal", "J. Akhir", "Rajab", "Syakban", "Ramadan", "Syawal", "Zulkaidah", "Zulhijah"];
/// Weekday initials from Monday.
const INITIALS: [&str; 7] = ["M", "T", "W", "T", "F", "S", "S"];
/// A red date's red, and a cuti bersama's.
const RED: Color = Color::hex(0xFF6B61);
const LEAVE: Color = Color::hex(0xE8A29C);

pub fn height(_st: &AppState) -> f32 {
    HEIGHT
}

/// Midnight: today moves to the next day.
pub fn next_wake(st: &AppState) -> Option<f64> {
    let t = st.local;
    let since_midnight = t.hour as f64 * 3600.0 + t.minute as f64 * 60.0 + t.second as f64;
    Some(st.now + (86_400.0 - since_midnight).max(1.0))
}

/// "31 days", "Tomorrow", "Today".
fn left_words(days: i64) -> String {
    match days {
        0 => "Today".into(),
        1 => "Tomorrow".into(),
        n => format!("{n} days"),
    }
}

pub fn draw(ui: &mut Ui, st: &mut AppState, area: Rect) {
    let t = st.local;
    if !valid_date(&t) {
        return;
    }
    // Fixed layout height: the island's spring may still be opening.
    let area = Rect::new(area.x, area.y, area.w, HEIGHT);
    let today = (t.year, t.month, t.day);
    let (my, mm) = st.ui.cal_month.unwrap_or((t.year, t.month));
    let picked = st.ui.cal_day.unwrap_or(today);
    let cell_w = ((area.w - COL_GAP) * 0.5 / 7.0).min(CELL_W_MAX).floor();
    let grid = Rect::new(area.x, area.y, 7.0 * cell_w, area.h);
    draw_month(ui, st, grid, cell_w, (my, mm), today, picked);
    let px = grid.right() + COL_GAP;
    draw_panel(ui, st, Rect::new(px, area.y, (area.right() - px).max(0.0), area.h), today, picked);
}

/// The month `ym` in `r`: its name and the arrows, the initials, the days.
fn draw_month(ui: &mut Ui, st: &mut AppState, r: Rect, cell_w: f32, (y, m): (i32, u8), today: (i32, u8, u8), picked: (i32, u8, u8)) {
    let title = format!("{} {y}", MONTH_LONG[(m.clamp(1, 12) - 1) as usize]);
    ui.c.text(&title, Rect::new(r.x + 4.0, r.y, r.w - 60.0, TITLE_H), &TextStyle::body().weight(Weight::Semibold));
    let arrow = |i: f32| Rect::new(r.right() - (2.0 - i) * 26.0, r.y, 24.0, 24.0);
    let mut go = 0;
    if ui.icon_button(wid("today.prev"), arrow(0.0), Icon::ChevronLeft, palette::TEXT_DIM, 16.0).clicked {
        go = -1;
    }
    if ui.icon_button(wid("today.next"), arrow(1.0), Icon::ChevronRight, palette::TEXT_DIM, 16.0).clicked {
        go = 1;
    }
    if go != 0 {
        let n = y * 12 + m as i32 - 1 + go;
        let to = (n.div_euclid(12), (n.rem_euclid(12) + 1) as u8);
        st.ui.cal_month = (to != (today.0, today.1)).then_some(to);
    }
    let initials = TextStyle::caption().weight(Weight::Semibold).center();
    for (col, s) in INITIALS.iter().enumerate() {
        let color = if col == 6 { RED.with_alpha(0.8) } else { palette::TEXT_FAINT };
        ui.c.text(s, Rect::new(r.x + col as f32 * cell_w, r.y + TITLE_H, cell_w, INITIALS_H), &initials.color(color));
    }
    let first = days_from_civil(y, m, 1);
    let start = first - (i64::from(weekday_from_days(first)) + 6) % 7;
    let today_n = days_from_civil(today.0, today.1, today.2);
    let picked_n = days_from_civil(picked.0, picked.1, picked.2);
    let top = r.y + TITLE_H + INITIALS_H;
    let num = TextStyle::footnote().tabular().center();
    // A Hijri month's first day carries its name, by the calendar Prayer
    // times follows (Kemenag or KHGT).
    let cal = st.settings.prayer.follow.calendar();
    let mark_st = TextStyle::new(9.5).weight(Weight::Semibold).center().color(st.accent);
    let radius = (CELL_H.min(cell_w) / 2.0 - 2.0).clamp(8.0, 11.0);
    for i in 0..42 {
        let day = start + i;
        let (dy, dm, dd) = civil_from_days(day);
        let (row, col) = ((i / 7) as f32, (i % 7) as usize);
        let cell = Rect::new(r.x + col as f32 * cell_w, top + row * CELL_H, cell_w, CELL_H);
        let c = cell.center();
        let inside = dm == m;
        if ui.button(wid_n("today.day", i as u64), cell).clicked {
            st.ui.cal_day = Some((dy, dm, dd)).filter(|p| *p != today);
            if !inside {
                st.ui.cal_month = Some((dy, dm)).filter(|p| *p != (today.0, today.1));
            }
        }
        let red = agenda::holiday(dy, dm, dd);
        let ink = match red {
            Some(h) if h.leave => LEAVE,
            Some(_) => RED,
            None if col == 6 => RED.with_alpha(0.85),
            None => Color::white(0.9),
        };
        let ink = if inside { ink } else { ink.mul_alpha(0.35) };
        // Worked out only while Hijri dates or Ramadan mode is on (audit F2).
        let hijri = (st.settings.cal_hijri || st.settings.ramadan_mode).then(|| poni_core::hijri::date(cal, day));
        // Ramadan mode: each fasting day carries its number, the 1st its month.
        let fasting = st.settings.ramadan_mode && hijri.is_some_and(|h| h.month == 9);
        let marked = hijri.filter(|h| (st.settings.cal_hijri || fasting) && (h.day == 1 || fasting));
        // A marked day's number sits a little higher, its month's name under it.
        let num_r = if marked.is_some() { Rect::new(cell.x, cell.y - 4.0, cell.w, cell.h) } else { cell };
        if day == today_n {
            ui.c.fill_circle(c, radius, st.accent);
            ui.c.text(&dd.to_string(), num_r, &num.weight(Weight::Bold).color(palette::ON_ACCENT));
        } else {
            if day == picked_n {
                ui.c.stroke_circle(c, radius, st.accent, 1.5);
            }
            ui.c.text(&dd.to_string(), num_r, &num.color(ink));
        }
        if let Some(hijri) = marked {
            let day_no = hijri.day.to_string();
            let name = if hijri.day == 1 { HIJRI_SHORT[(hijri.month.clamp(1, 12) - 1) as usize] } else { day_no.as_str() };
            let color = if day == today_n { palette::ON_ACCENT } else if inside { st.accent } else { st.accent.mul_alpha(0.4) };
            ui.c.text(name, Rect::new(cell.x - 8.0, cell.bottom() - 11.0, cell.w + 16.0, 10.0), &mark_st.color(color));
        }
        let task_due = st.calendar_tasks() && st.store.tasks.tasks.iter().any(|t| !t.done && t.due.is_some_and(|x| x.falls_on(dy, dm, dd)));
        if (task_due || !st.store.agenda.on(dy, dm, dd).is_empty()) && marked.is_none() {
            let dot = if day == today_n { palette::ON_ACCENT } else { st.accent };
            ui.c.fill_circle(Point::new(c.x, cell.bottom() - 3.0), 1.8, dot);
        }
    }
}

/// The countdown, then the day picked: its holiday, its events, the line
/// to add one.
fn draw_panel(ui: &mut Ui, st: &mut AppState, r: Rect, today: (i32, u8, u8), picked: (i32, u8, u8)) {
    let mut y = r.y + 2.0;
    let next = agenda::upcoming(today, COUNTDOWN_DAYS, COUNTDOWN_MAX);
    for (h, days) in &next {
        ui.c.fill_circle(Point::new(r.x + 4.0, y + LINE_H / 2.0), 3.0, RED);
        let when = left_words(*days);
        let ws = TextStyle::footnote().weight(Weight::Semibold).color(RED).right();
        let ww = ui.c.measure(&when, &ws, 120.0).0 + 2.0;
        ui.c.text(h.name, Rect::new(r.x + 14.0, y, (r.w - 14.0 - ww - 8.0).max(0.0), LINE_H), &TextStyle::footnote());
        ui.c.text(&when, Rect::new(r.right() - ww, y, ww, LINE_H), &ws);
        y += LINE_H;
    }
    if next.is_empty() {
        ui.c.text("No red dates in the next two months", Rect::new(r.x, y, r.w, LINE_H), &TextStyle::footnote().color(palette::TEXT_DIM));
        y += LINE_H;
    }
    y += 6.0;
    let px = 1.0 / ui.c.scale().max(0.5);
    ui.c.fill_rect(Rect::new(r.x, y, r.w, px), Color::white(0.10));
    y += 8.0;

    // The day picked.
    let (py, pm, pd) = picked;
    let wd = WEEKDAY_SHORT[weekday_from_days(days_from_civil(py, pm, pd)) as usize % 7];
    let mut title = format!("{wd} {pd} {}", MONTH_SHORT[(pm.clamp(1, 12) - 1) as usize]);
    if py != today.0 {
        title += &format!(" {py}");
    }
    let tw = ui.c.measure(&title, &TextStyle::body().weight(Weight::Semibold), 300.0).0 + 2.0;
    ui.c.text(&title, Rect::new(r.x, y, tw, 20.0), &TextStyle::body().weight(Weight::Semibold));
    let mut hw = 0.0;
    if st.settings.cal_hijri {
        let hijri = poni_core::hijri::date(st.settings.prayer.follow.calendar(), days_from_civil(py, pm, pd));
        let hl = format!("{} {}", hijri.day, poni_core::hijri::MONTHS[(hijri.month.clamp(1, 12) - 1) as usize]);
        let hs = TextStyle::caption().color(palette::TEXT_FAINT).right();
        hw = ui.c.measure(&hl, &hs, 200.0).0 + 2.0;
        ui.c.text(&hl, Rect::new(r.right() - hw, y + 2.0, hw, 18.0), &hs);
    }
    if let Some(h) = agenda::holiday(py, pm, pd) {
        let color = if h.leave { LEAVE } else { RED };
        ui.c.text(h.name, Rect::new(r.x + tw + 8.0, y + 1.0, (r.w - tw - hw - 16.0).max(0.0), 18.0), &TextStyle::footnote().color(color));
    }
    y += 22.0;
    // The day's events (all-day first, then by time) and the tasks due
    // that day, in the room down to the controls; it scrolls when they don't fit.
    let mut events: Vec<(u64, String, Every, Option<u16>)> = st.store.agenda.on(py, pm, pd).into_iter().map(|e| (e.id, e.title.clone(), e.every, e.time)).collect();
    events.sort_by_key(|e| e.3.map_or(-1, i32::from));
    let tasks: Vec<(u64, String, bool, Option<u16>)> = if st.calendar_tasks() {
        st.store.tasks.due_on(py, pm, pd).into_iter().map(|t| (t.id, t.text.clone(), t.done, t.due.and_then(|d| d.time))).collect()
    } else {
        Vec::new()
    };
    let list = Rect::new(r.x - 4.0, y, r.w + 4.0, (r.bottom() - 2.0 * PILL_SMALL - 14.0 - y).max(0.0));
    let rows = events.len() + tasks.len();
    let max_scroll = (rows as f32 * EVENT_H - list.h).max(0.0);
    if st.ui.cal_scroll_day != Some(picked) {
        st.ui.cal_scroll_day = Some(picked);
        st.ui.cal_scroll = 0.0;
    }
    // The wheel is the list's only while it has more than it shows: else it
    // goes on to the time picker under the pointer.
    if max_scroll > 0.0 {
        let wheel = ui.wheel_in(list);
        if wheel != 0.0 {
            st.ui.cal_scroll -= wheel * EVENT_H;
        }
    }
    st.ui.cal_scroll = st.ui.cal_scroll.clamp(0.0, max_scroll);
    let offset = st.ui.cal_scroll;
    ui.c.clip(list);
    let mut gone = None;
    let mut gone_task = None;
    let mut checked = None;
    let mut edit_row: Option<(bool, u64)> = None;
    for i in 0..rows {
        let row = Rect::new(r.x, list.y + i as f32 * EVENT_H - offset, r.w, EVENT_H);
        // Only rows (partly) in the list take the pointer.
        let shown = row.bottom() > list.y && row.y < list.bottom();
        if !shown {
            continue;
        }
        let hit = Rect::new(row.x, row.y.max(list.y), row.w, (row.bottom().min(list.bottom()) - row.y.max(list.y)).max(0.0));
        if i < events.len() {
            let (id, name, every, time) = &events[i];
            let name = match time {
                Some(t) => format!("{}  {name}", agenda::fmt_time(*t)),
                None => name.clone(),
            };
            let over = ui.hovered(hit);
            if over {
                ui.c.fill_rrect(row.inset(-4.0, 1.0), 6.0, Color::white(0.05));
            }
            ui.c.fill_circle(Point::new(row.x + 4.0, row.center().y), 2.5, st.accent);
            let x = Rect::new(row.right() - 20.0, row.y + 1.0, 20.0, 20.0);
            let pen = Rect::new(x.x - 22.0, x.y, 20.0, 20.0);
            if over && in_list(pen, list) && ui.icon_button(wid_n("today.event.edit", *id), pen, Icon::Pencil, palette::TEXT_DIM, 12.0).clicked {
                edit_row = Some((false, *id));
            }
            let repeat = if *every == Every::Once { String::new() } else { every.label() };
            let rs = TextStyle::caption().color(palette::TEXT_FAINT).right();
            let rw = if repeat.is_empty() { 0.0 } else { ui.c.measure(&repeat, &rs, 140.0).0 + 8.0 };
            let end = if over { pen.x } else { x.x };
            ui.c.text(&name, Rect::new(row.x + 14.0, row.y, (end - rw - row.x - 18.0).max(0.0), row.h), &TextStyle::footnote());
            if !repeat.is_empty() {
                ui.c.text(&repeat, Rect::new(end - rw, row.y, rw - 4.0, row.h), &rs);
            }
            let xr = Rect::new(x.x, x.y.max(list.y), x.w, (x.bottom().min(list.bottom()) - x.y.max(list.y)).max(0.0));
            if xr.h > 4.0 && ui.icon_button(wid_n("today.event.remove", *id), xr, Icon::Close, palette::TEXT_FAINT, 10.0).clicked {
                gone = Some(*id);
            }
        } else {
            let (id, name, done, time) = &tasks[i - events.len()];
            let resp = ui.button(wid_n("today.task", *id), hit);
            if resp.hovered {
                ui.c.fill_rrect(row.inset(-4.0, 1.0), 6.0, Color::white(0.05));
            }
            let mark = Rect::new(row.x - 2.0, row.center().y - 7.0, 14.0, 14.0);
            if *done {
                ui.c.icon(Icon::CheckCircle, mark, palette::GREEN);
            } else {
                ui.c.icon(Icon::Circle, mark, palette::TEXT_DIM);
            }
            let name = match time {
                Some(t) => format!("{}  {name}", agenda::fmt_time(*t)),
                None => name.clone(),
            };
            let ink = if *done { palette::TEXT_FAINT } else { palette::TEXT };
            let x = Rect::new(row.right() - 20.0, row.y + 1.0, 20.0, 20.0);
            let pen = Rect::new(x.x - 22.0, x.y, 20.0, 20.0);
            let end = if resp.hovered { pen.x - 4.0 } else { row.right() };
            ui.c.text(&name, Rect::new(row.x + 16.0, row.y, (end - row.x - 20.0).max(0.0), row.h), &TextStyle::footnote().color(ink));
            let mut used = false;
            if resp.hovered && in_list(x, list) {
                if ui.icon_button(wid_n("today.task.edit", *id), pen, Icon::Pencil, palette::TEXT_DIM, 12.0).clicked {
                    edit_row = Some((true, *id));
                    used = true;
                }
                if ui.icon_button(wid_n("today.task.remove", *id), x, Icon::Trash, palette::TEXT_DIM, 12.0).clicked {
                    gone_task = Some(*id);
                    used = true;
                }
            }
            if resp.clicked && !used && !ui.hovered(pen) && !ui.hovered(x) {
                checked = Some(*id);
            }
        }
    }
    ui.c.unclip();
    // More below (or above): a thin bar on the right says where the list is.
    if max_scroll > 0.0 {
        let track = Rect::new(r.right() + 4.0, list.y + 2.0, 2.0, list.h - 4.0);
        let thumb_h = (track.h * list.h / (rows as f32 * EVENT_H)).max(10.0);
        let thumb_y = track.y + (track.h - thumb_h) * (offset / max_scroll);
        ui.c.fill_rrect(Rect::new(track.x, thumb_y, track.w, thumb_h), 1.0, Color::white(0.25));
    }
    // A repeating task's later day only shows it: checking happens on the
    // day it's due now (then it moves on to its next).
    if let Some(id) = checked.filter(|id| st.store.tasks.tasks.iter().any(|t| t.id == *id && t.due.is_some_and(|d| (d.year, d.month, d.day) == (py, pm, pd)))) {
        st.store.tasks.toggle(id, st.unix);
        st.save();
    }
    if let Some(id) = gone {
        st.store.agenda.remove(id);
        st.save();
    }
    if let Some(id) = gone_task {
        st.store.tasks.remove(id);
        st.save();
    }
    // Edit: the row's words, time, reminder and repeat go into the add line.
    if let Some((task, id)) = edit_row {
        if task {
            if let Some(tk) = st.store.tasks.tasks.iter().find(|x| x.id == id) {
                st.ui.cal_input = tk.text.clone();
                st.ui.cal_time = tk.due.and_then(|d| d.time);
                st.ui.cal_remind = Some(tk.due.and_then(|d| d.remind));
                st.ui.cal_every = tk.due.map_or(Every::Once, |d| d.every);
            }
        } else if let Some(e) = st.store.agenda.events.iter().find(|x| x.id == id) {
            st.ui.cal_input = e.title.clone();
            st.ui.cal_time = e.time;
            st.ui.cal_remind = Some(e.remind);
            st.ui.cal_every = e.every;
        }
        st.ui.cal_task = task;
        st.ui.cal_editing = Some((task, id));
    }

    // The line to add one; over it, left to right: its time, its reminder
    // and how often it comes, each where the one before ends.
    let add_y = r.bottom() - PILL_SMALL - 2.0;
    let ctl_y = add_y - PILL_SMALL - 6.0;
    // What it makes: an event, or (with tasks on the calendar) a task due this day.
    let x0 = if st.calendar_tasks() {
        let kind = Rect::new(r.x - 2.0, ctl_y, PILL_SMALL + 4.0, PILL_SMALL);
        let kind_icon = if st.ui.cal_task { Icon::ListChecks } else { Icon::Calendar };
        if ui.pill(wid("today.kind"), kind, "", Some(kind_icon), PillStyle::Utility).clicked {
            st.ui.cal_task = !st.ui.cal_task;
        }
        kind.right() + 6.0
    } else {
        st.ui.cal_task = false;
        r.x - 2.0
    };
    let x = draw_time_picker(ui, st, x0, ctl_y);
    // The reminder: off, at its time, or a while before (an all-day
    // event's in the morning).
    let remind = st.ui.cal_remind.unwrap_or(Some(10));
    let rlabel = agenda::remind_label(remind);
    let bw = ui.pill_width(&rlabel, true, true);
    let bell = Rect::new(x + 6.0, ctl_y, bw, PILL_SMALL);
    let bell_style = if remind.is_some() { PillStyle::UtilityAccent(st.accent) } else { PillStyle::Utility };
    if ui.pill(wid("today.remind"), bell, &rlabel, Some(Icon::Bell), bell_style).clicked {
        let i = agenda::REMINDERS.iter().position(|r| *r == remind).unwrap_or(0);
        st.ui.cal_remind = Some(agenda::REMINDERS[(i + 1) % agenda::REMINDERS.len()]);
    }
    // How often (a task's too: checked, it moves on to its next day).
    let every = st.ui.cal_every;
    let label = every.short();
    let pw = ui.pill_width(&label, false, true);
    let pill = Rect::new(bell.right() + 6.0, ctl_y, pw, PILL_SMALL);
    if ui.pill(wid("today.every"), pill, &label, None, PillStyle::Utility).clicked {
        st.ui.cal_every = every.next_kind();
    }
    // Its count, where there's room for it.
    if every != Every::Once && pill.right() + 2.0 + 44.0 <= r.right() + 4.0 {
        let n = every.count();
        let minus = Rect::new(pill.right() + 2.0, ctl_y + 3.0, 22.0, 22.0);
        let plus = Rect::new(minus.right(), ctl_y + 3.0, 22.0, 22.0);
        if ui.icon_button(wid("today.every.minus"), minus, Icon::Minus, palette::TEXT_DIM, 11.0).clicked {
            st.ui.cal_every = every.with_count(n.saturating_sub(1));
        }
        if ui.icon_button(wid("today.every.plus"), plus, Icon::Plus, palette::TEXT_DIM, 11.0).clicked {
            st.ui.cal_every = every.with_count(n + 1);
        }
    }
    let input = Rect::new(r.x - 6.0, add_y, (r.right() - r.x + 6.0).max(0.0), PILL_SMALL);
    let accent = st.accent;
    let placeholder = match (st.ui.cal_editing, st.ui.cal_task) {
        (Some(_), _) => "Edit, Enter to save",
        (None, true) => "Add a task due this day",
        (None, false) => "Add an event",
    };
    let resp = ui.text_input_row(wid("today.input"), input, &mut st.ui.cal_input, placeholder, MAX_TITLE, accent, Some(Icon::Plus));
    if resp.cancelled {
        st.ui.cal_editing = None;
        st.ui.cal_input.clear();
    }
    if resp.submitted {
        // Editing one that's gone meanwhile (deleted): the title is added
        // as a new one below, not dropped.
        let editing = st.ui.cal_editing.take().filter(|&(task, id)| {
            if task {
                st.store.tasks.tasks.iter().any(|x| x.id == id)
            } else {
                st.store.agenda.events.iter().any(|x| x.id == id)
            }
        });
        if let Some((task, id)) = editing {
            let (typed, title) = agenda::split_time(&st.ui.cal_input);
            let title: String = title.split_whitespace().collect::<Vec<_>>().join(" ");
            let time = typed.or(st.ui.cal_time);
            if !title.is_empty() {
                if task {
                    if let Some(tk) = st.store.tasks.tasks.iter_mut().find(|x| x.id == id) {
                        tk.text = title;
                        let d = tk.due.unwrap_or(agenda::Due::on(py, pm, pd));
                        tk.due = Some(agenda::Due { time, remind, every: st.ui.cal_every, ..d });
                    }
                } else if let Some(e) = st.store.agenda.events.iter_mut().find(|x| x.id == id) {
                    e.title = title;
                    e.time = time;
                    e.remind = remind;
                    e.every = st.ui.cal_every;
                }
                st.ui.cal_input.clear();
                st.save();
            }
            return;
        }
        let added = if st.ui.cal_task {
            let (typed, title) = agenda::split_time(&st.ui.cal_input);
            let due = agenda::Due { time: typed.or(st.ui.cal_time), remind, every: st.ui.cal_every, ..agenda::Due::on(py, pm, pd) };
            let title = title.to_string();
            st.store.tasks.add_due(&title, st.unix, due).is_some()
        } else {
            st.store.agenda.add(&st.ui.cal_input, py, pm, pd, st.ui.cal_every, remind, st.ui.cal_time).is_some()
        };
        if added {
            st.ui.cal_input.clear();
            st.save();
        }
    }
}

/// The next event's time: "All day" until clicked (then the next 5-minute
/// mark from now), − and + a step each, the wheel over it too; a click on
/// the time goes back to all day.
/// Returns where it ends.
fn draw_time_picker(ui: &mut Ui, st: &mut AppState, x: f32, y: f32) -> f32 {
    let time = st.ui.cal_time;
    let label = time.map_or("All day".to_string(), agenda::fmt_time);
    let w = ui.pill_width(&label, true, true);
    let r = Rect::new(x, y, w, PILL_SMALL);
    let style = if time.is_some() { PillStyle::UtilityAccent(st.accent) } else { PillStyle::Utility };
    if ui.pill(wid("today.time"), r, &label, Some(Icon::Clock), style).clicked {
        st.ui.cal_time = match time {
            Some(_) => None,
            None => {
                let now = st.local.hour as i32 * 60 + st.local.minute as i32;
                Some((((now / TIME_STEP) + 1) * TIME_STEP).rem_euclid(1440) as u16)
            }
        };
    }
    let Some(t) = st.ui.cal_time else { return r.right() };
    let mut step = 0;
    let minus = Rect::new(r.right() + 2.0, y + 3.0, 22.0, 22.0);
    let plus = Rect::new(minus.right(), y + 3.0, 22.0, 22.0);
    if ui.icon_button(wid("today.time.minus"), minus, Icon::Minus, palette::TEXT_DIM, 11.0).clicked {
        step = -1;
    }
    if ui.icon_button(wid("today.time.plus"), plus, Icon::Plus, palette::TEXT_DIM, 11.0).clicked {
        step = 1;
    }
    let wheel = ui.wheel_in(Rect::new(r.x, r.y, plus.right() - r.x, r.h));
    if wheel != 0.0 {
        step += wheel.signum() as i32;
    }
    if step != 0 {
        st.ui.cal_time = Some((t as i32 + step * TIME_STEP).rem_euclid(1440) as u16);
    }
    plus.right()
}

/// A button inside the day's list as it's scrolled (not cut by its edge).
fn in_list(b: Rect, list: Rect) -> bool {
    b.y >= list.y && b.bottom() <= list.bottom()
}

/// The app fills `local` from GetLocalTime; guard against the zeroed default.
fn valid_date(t: &LocalDateTime) -> bool {
    (1..=12).contains(&t.month) && t.day >= 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_left_read_short() {
        assert_eq!(left_words(0), "Today");
        assert_eq!(left_words(1), "Tomorrow");
        assert_eq!(left_words(31), "31 days");
    }
}
