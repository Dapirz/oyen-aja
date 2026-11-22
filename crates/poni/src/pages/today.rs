//! Today page: the date and five weeks from this Monday.
//!
//! On the left the weekday, the date, the year and the week number; on the
//! right the weeks, today filled and a step line where a month starts. On an
//! island narrower than [`COMPACT_BELOW`] the date goes on one line above
//! the weeks. The header leaves the date out on this page, so the page
//! always shows it.
//!
//! Nothing is read for it and it never asks for continuous frames: the
//! date moves once a day, at midnight ([`next_wake`]).

use crate::gfx::{TextStyle, Weight};
use crate::state::AppState;
use crate::ui::Ui;
use poni_core::datetime::{civil_from_days, days_from_civil, weekday_from_days, LocalDateTime};
use poni_core::geom::{palette, Color, Point, Rect};

const HEIGHT: f32 = 200.0;

/// Below this content width the date makes way for the weeks (the island
/// can be made as narrow as 480).
const COMPACT_BELOW: f32 = 480.0;
/// The date's column, left of the weeks.
const DATE_W: f32 = 168.0;
const COL_GAP: f32 = 20.0;
/// The weeks: five rows from this Monday, a cell at most this wide and tall.
const GRID_DAYS: i64 = 35;
const CELL_W_MAX: f32 = 44.0;
const CELL_H_MAX: f32 = 34.0;
const INITIALS_H: f32 = 18.0;
/// The compact layout's date line, above the weeks.
const LINE_H: f32 = 20.0;

const WEEKDAY_LONG: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MONTH_LONG: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
/// Weekday initials from Monday.
const INITIALS: [&str; 7] = ["M", "T", "W", "T", "F", "S", "S"];

pub fn height(_st: &AppState) -> f32 {
    HEIGHT
}

/// Midnight: the date and the ring move to the next day.
pub fn next_wake(st: &AppState) -> Option<f64> {
    let t = st.local;
    let since_midnight = t.hour as f64 * 3600.0 + t.minute as f64 * 60.0 + t.second as f64;
    Some(st.now + (86_400.0 - since_midnight).max(1.0))
}

pub fn draw(ui: &mut Ui, st: &mut AppState, area: Rect) {
    let t = st.local;
    if !valid_date(&t) {
        return;
    }
    // Fixed layout height: the island's spring may still be opening.
    let area = Rect::new(area.x, area.y, area.w, HEIGHT);
    let today = days_from_civil(t.year, t.month, t.day);
    let weekday = WEEKDAY_LONG[weekday_from_days(today) as usize % 7];
    let month = MONTH_LONG[(t.month.clamp(1, 12) - 1) as usize];
    if area.w < COMPACT_BELOW {
        let line = format!("{} \u{b7} {} {month} {}", weekday.to_uppercase(), t.day, t.year);
        ui.c.text(&line, Rect::new(area.x, area.y, area.w, LINE_H), &TextStyle::caption().weight(Weight::Bold).color(st.accent));
        let grid = Rect::new(area.x, area.y + LINE_H + 4.0, area.w, area.h - LINE_H - 4.0);
        draw_weeks(ui, st, grid, today);
        return;
    }
    let x = area.x;
    ui.c.text(&weekday.to_uppercase(), Rect::new(x, area.y, DATE_W, 16.0), &TextStyle::caption().weight(Weight::Bold).color(st.accent));
    ui.c.text(&format!("{} {month}", t.day), Rect::new(x, area.y + 16.0, DATE_W, 26.0), &TextStyle::value());
    let dim = TextStyle::footnote().color(palette::TEXT_DIM);
    ui.c.text(&t.year.to_string(), Rect::new(x, area.y + 46.0, DATE_W, 17.0), &dim);
    ui.c.text(&format!("Week {}", iso_week(today)), Rect::new(x, area.y + 64.0, DATE_W, 17.0), &dim);
    let gx = area.x + DATE_W + COL_GAP;
    draw_weeks(ui, st, Rect::new(gx, area.y, (area.right() - gx).max(0.0), area.h), today);
}

/// Five weeks from this Monday in `r`: the weekday initials, then the
/// dates, today filled, days gone by dimmer, a step line where a month
/// starts.
fn draw_weeks(ui: &mut Ui, st: &AppState, r: Rect, today: i64) {
    let cell_w = (r.w / 7.0).min(CELL_W_MAX).floor();
    let cell_h = ((r.h - INITIALS_H) / 5.0).min(CELL_H_MAX).floor();
    if cell_w < 14.0 || cell_h < 14.0 {
        return;
    }
    let grid_w = 7.0 * cell_w;
    // Every initial in one tone; weekend days are told apart by the dimmer numbers.
    let initials_st = TextStyle::caption().weight(Weight::Semibold).color(palette::TEXT_FAINT).center();
    for (col, s) in INITIALS.iter().enumerate() {
        ui.c.text(s, Rect::new(r.x + col as f32 * cell_w, r.y, cell_w, INITIALS_H), &initials_st);
    }
    let top = r.y + INITIALS_H;
    let start = grid_start(today);
    let num_st = TextStyle::footnote().tabular().center();
    let step = Color::white(0.2);
    let radius = (cell_h.min(cell_w) / 2.0 - 3.0).clamp(8.0, 12.0);
    for i in 0..GRID_DAYS {
        let day = start + i;
        let (row, col) = ((i / 7) as f32, (i % 7) as usize);
        let cell = Rect::new(r.x + col as f32 * cell_w, top + row * cell_h, cell_w, cell_h);
        let c = cell.center();
        let (_, _, d) = civil_from_days(day);
        let label = d.to_string();
        if day == today {
            ui.c.fill_circle(c, radius, st.accent);
            ui.c.text(&label, cell, &num_st.weight(Weight::Bold).color(ink_on(st.accent)));
        } else {
            let ink = if day < today {
                Color::white(0.32)
            } else if col >= 5 {
                palette::TEXT_DIM
            } else {
                Color::white(0.9)
            };
            let st_num = if d == 1 { num_st.weight(Weight::Bold) } else { num_st };
            ui.c.text(&label, cell, &st_num.color(ink));
        }
        // Where a month starts: a step line between its 1st and the days before it.
        if d == 1 && i > 0 {
            let y = cell.y;
            if row > 0.0 {
                ui.c.line(Point::new(cell.x, y), Point::new(r.x + grid_w, y), step, 1.0);
            }
            if col > 0 {
                ui.c.line(Point::new(cell.x, y), Point::new(cell.x, y + cell_h), step, 1.0);
                if row < 4.0 {
                    ui.c.line(Point::new(r.x, y + cell_h), Point::new(cell.x, y + cell_h), step, 1.0);
                }
            }
        }
    }
}

/// This week's Monday, the weeks' first day.
fn grid_start(today: i64) -> i64 {
    today - (i64::from(weekday_from_days(today)) + 6) % 7
}

/// The ISO 8601 week number of `day` (days since 1970-01-01): the week
/// that holds its Thursday, counted in that Thursday's year.
fn iso_week(day: i64) -> i64 {
    let thursday = day + 3 - (i64::from(weekday_from_days(day)) + 6) % 7;
    let (year, _, _) = civil_from_days(thursday);
    (thursday - days_from_civil(year, 1, 1)) / 7 + 1
}

/// The app fills `local` from GetLocalTime; guard against the zeroed default.
fn valid_date(t: &LocalDateTime) -> bool {
    (1..=12).contains(&t.month) && t.day >= 1
}

/// Text colour that stays legible on an accent fill.
fn ink_on(fill: Color) -> Color {
    if fill.luminance() > 0.6 {
        palette::ON_ACCENT_DARK
    } else {
        palette::ON_ACCENT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_weeks_start_on_monday() {
        // Sunday 22 Nov 2026: the week began on Monday 16 Nov.
        let sunday = days_from_civil(2026, 11, 22);
        assert_eq!(civil_from_days(grid_start(sunday)), (2026, 11, 16));
        let monday = days_from_civil(2026, 11, 16);
        assert_eq!(grid_start(monday), monday);
    }

    #[test]
    fn week_numbers_follow_iso_8601() {
        assert_eq!(iso_week(days_from_civil(2026, 11, 22)), 47);
        // 1 Jan 2027 is a Friday: still week 53 of 2026.
        assert_eq!(iso_week(days_from_civil(2027, 1, 1)), 53);
        // 29 Dec 2025 is a Monday: week 1 of 2026.
        assert_eq!(iso_week(days_from_civil(2025, 12, 29)), 1);
    }
}
