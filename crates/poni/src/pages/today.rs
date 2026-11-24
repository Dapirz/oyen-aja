//! Today page: a month's calendar, and the day picked in it.
//!
//! On the left the month (Monday first): today filled, the day picked
//! ringed, Indonesia's national holidays and Sundays in red (a cuti bersama
//! in a softer red), a dot under a day with an event of the person's own;
//! the arrows go to the month before or after. On the right the countdown
//! to the next red dates (a few, when several come soon), then the day
//! picked: its holiday, its events (each can go), and a line to add one,
//! once or again every so many days, weeks, months or years.
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
const EVENTS_SHOWN: usize = 3;
const MAX_TITLE: usize = 80;

const MONTH_LONG: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const MONTH_SHORT: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKDAY_SHORT: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
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
        if day == today_n {
            ui.c.fill_circle(c, radius, st.accent);
            ui.c.text(&dd.to_string(), cell, &num.weight(Weight::Bold).color(palette::ON_ACCENT));
        } else {
            if day == picked_n {
                ui.c.stroke_circle(c, radius, st.accent, 1.5);
            }
            ui.c.text(&dd.to_string(), cell, &num.color(ink));
        }
        if !st.store.agenda.on(dy, dm, dd).is_empty() {
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
    if let Some(h) = agenda::holiday(py, pm, pd) {
        let color = if h.leave { LEAVE } else { RED };
        ui.c.text(h.name, Rect::new(r.x + tw + 8.0, y + 1.0, (r.w - tw - 8.0).max(0.0), 18.0), &TextStyle::footnote().color(color));
    }
    y += 22.0;
    let events: Vec<(u64, String, Every)> = st.store.agenda.on(py, pm, pd).into_iter().map(|e| (e.id, e.title.clone(), e.every)).collect();
    let mut gone = None;
    for (i, (id, name, every)) in events.iter().take(EVENTS_SHOWN).enumerate() {
        let row = Rect::new(r.x, y, r.w, EVENT_H);
        ui.c.fill_circle(Point::new(row.x + 4.0, row.center().y), 2.5, st.accent);
        let x = Rect::new(row.right() - 20.0, row.y + 1.0, 20.0, 20.0);
        let repeat = if *every == Every::Once { String::new() } else { every.label() };
        let rs = TextStyle::caption().color(palette::TEXT_FAINT).right();
        let rw = if repeat.is_empty() { 0.0 } else { ui.c.measure(&repeat, &rs, 140.0).0 + 8.0 };
        ui.c.text(name, Rect::new(row.x + 14.0, row.y, (x.x - rw - row.x - 18.0).max(0.0), row.h), &TextStyle::footnote());
        if !repeat.is_empty() {
            ui.c.text(&repeat, Rect::new(x.x - rw, row.y, rw - 4.0, row.h), &rs);
        }
        if ui.icon_button(wid_n("today.event.remove", i as u64), x, Icon::Close, palette::TEXT_FAINT, 10.0).clicked {
            gone = Some(*id);
        }
        y += EVENT_H;
    }
    if events.len() > EVENTS_SHOWN {
        ui.c.text(&format!("+{} more", events.len() - EVENTS_SHOWN), Rect::new(r.x + 14.0, y, r.w, 16.0), &TextStyle::caption().color(palette::TEXT_DIM));
    }
    if let Some(id) = gone {
        st.store.agenda.remove(id);
        st.save();
    }

    // The line to add one, and how often it comes.
    let add_y = r.bottom() - PILL_SMALL - 2.0;
    let every = st.ui.cal_every;
    let label = every.label();
    let pw = ui.pill_width(&label, false, true);
    let steps = every != Every::Once;
    let steps_w = if steps { 2.0 * 22.0 + 4.0 } else { 0.0 };
    let pill = Rect::new(r.right() - pw - steps_w, add_y, pw, PILL_SMALL);
    if ui.pill(wid("today.every"), pill, &label, None, PillStyle::Utility).clicked {
        st.ui.cal_every = every.next_kind();
    }
    if steps {
        let n = every.count();
        let minus = Rect::new(pill.right() + 4.0, add_y + 3.0, 22.0, 22.0);
        let plus = Rect::new(minus.right(), add_y + 3.0, 22.0, 22.0);
        if ui.icon_button(wid("today.every.minus"), minus, Icon::Minus, palette::TEXT_DIM, 11.0).clicked {
            st.ui.cal_every = every.with_count(n.saturating_sub(1));
        }
        if ui.icon_button(wid("today.every.plus"), plus, Icon::Plus, palette::TEXT_DIM, 11.0).clicked {
            st.ui.cal_every = every.with_count(n + 1);
        }
    }
    let input = Rect::new(r.x - 6.0, add_y, (pill.x - 8.0 - r.x + 6.0).max(0.0), PILL_SMALL);
    let accent = st.accent;
    let resp = ui.text_input_row(wid("today.input"), input, &mut st.ui.cal_input, "Add an event", MAX_TITLE, accent, Some(Icon::Plus));
    if resp.submitted && st.store.agenda.add(&st.ui.cal_input, py, pm, pd, st.ui.cal_every).is_some() {
        st.ui.cal_input.clear();
        st.save();
    }
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
