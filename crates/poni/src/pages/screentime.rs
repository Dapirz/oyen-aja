//! Screen time page (the Screen time add-on): today's total, the last seven
//! days as bars, and today's apps, most time first, each with its icon.
//!
//! The stretch under way counts too, up to now, so the page agrees with the
//! clock while it's open (it asks for a frame each half minute then). The
//! apps' icons come from their programs, once each
//! (`Command::ScreenIcons`).

use crate::commands::Command;
use crate::gfx::{TextStyle, Weight};
use crate::icons::Icon;
use crate::state::AppState;
use crate::ui::Ui;
use poni_core::geom::{palette, Color, Rect};
use poni_core::screentime::{fmt_dur, Usage, KEEP_DAYS};

const H: f32 = 230.0;
/// The top: today's total on the left, the week's bars on the right.
const TOP_H: f32 = 64.0;
const BARS_W: f32 = 196.0;
const BAR_GAP: f32 = 8.0;
const BAR_H_MAX: f32 = 40.0;
/// Today's apps under it.
const ROW_H: f32 = 28.0;
const ROWS: usize = 5;
const ICON: f32 = 18.0;
/// The page asks for a frame this often while open, so the minutes keep up.
const REFRESH_S: f64 = 30.0;

pub fn height(_st: &AppState) -> f32 {
    H
}

pub fn next_wake(st: &AppState) -> Option<f64> {
    st.screen_live.as_ref().map(|_| st.now + REFRESH_S)
}

/// The kept days with the stretch under way added up to now.
fn with_live(st: &AppState) -> Usage {
    let mut u = st.screen.clone();
    if let Some(l) = &st.screen_live {
        u.add(&l.day, &l.key, &l.name, &l.path, (st.unix - l.since).max(0) as u64);
    }
    u
}

pub fn draw(ui: &mut Ui, st: &mut AppState, area: Rect) {
    let u = with_live(st);
    let today = st.local.day_key();
    let total = u.total(&today);
    let apps = u.apps(&today);

    // Today's total, and the week's under it.
    let week: Vec<(String, u64)> = (0..KEEP_DAYS as i64).rev().map(|d| st.local.add_days(-d)).map(|d| {
        let k = d.day_key();
        let t = u.total(&k);
        (k, t)
    }).collect();
    let week_total: u64 = week.iter().map(|(_, t)| t).sum();
    ui.c.text("Today", Rect::new(area.x, area.y, 160.0, 16.0), &TextStyle::footnote().color(palette::TEXT_DIM));
    ui.c.text(&fmt_dur(total), Rect::new(area.x, area.y + 16.0, 220.0, 32.0), &TextStyle::new(26.0).weight(Weight::Semibold).tabular());
    ui.c.text(&format!("{} this week", fmt_dur(week_total)), Rect::new(area.x, area.y + 48.0, 220.0, 16.0), &TextStyle::footnote().color(palette::TEXT_DIM));

    // The week's bars, today last, in the accent.
    let most = week.iter().map(|(_, t)| *t).max().unwrap_or(0).max(1);
    let bw = (BARS_W - BAR_GAP * (KEEP_DAYS as f32 - 1.0)) / KEEP_DAYS as f32;
    let x0 = area.right() - BARS_W;
    let base = area.y + 6.0 + BAR_H_MAX;
    const LETTERS: [&str; 7] = ["S", "M", "T", "W", "T", "F", "S"];
    for (i, (_, t)) in week.iter().enumerate() {
        let x = x0 + i as f32 * (bw + BAR_GAP);
        let h = (BAR_H_MAX * *t as f32 / most as f32).max(if *t > 0 { 3.0 } else { 2.0 });
        let is_today = i == KEEP_DAYS - 1;
        let color = if is_today { st.accent } else if *t > 0 { Color::white(0.32) } else { Color::white(0.10) };
        ui.c.fill_rrect(Rect::new(x, base - h, bw, h), (bw / 2.0).min(3.0), color);
        let wd = st.local.add_days(i as i64 - (KEEP_DAYS as i64 - 1)).weekday() as usize;
        let style = TextStyle::caption().center().color(if is_today { palette::TEXT } else { palette::TEXT_DIM });
        ui.c.text(LETTERS[wd % 7], Rect::new(x - 4.0, base + 3.0, bw + 8.0, 14.0), &style);
    }

    // Today's apps.
    let list = Rect::new(area.x, area.y + TOP_H + 8.0, area.w, area.h - TOP_H - 8.0);
    if apps.is_empty() {
        ui.c.text("Nothing counted yet today", Rect::new(list.x, list.y + 20.0, list.w, 20.0), &TextStyle::body().center().color(palette::TEXT_DIM));
        return;
    }
    let most = apps.first().map_or(1, |(_, a)| a.secs.max(1));
    let mut missing = Vec::new();
    for (i, (key, a)) in apps.iter().take(ROWS).enumerate() {
        let y = list.y + i as f32 * ROW_H;
        let cy = y + ROW_H / 2.0;
        let icon = Rect::new(list.x, cy - ICON / 2.0, ICON, ICON);
        match st.screen_icons.get(key) {
            Some(img) => ui.c.image(img, icon, 4.0),
            None => {
                ui.c.fill_rrect(icon, 4.0, palette::SURFACE_HOVER);
                ui.c.icon(Icon::AppWindow, icon.inset(3.0, 3.0), palette::TEXT_DIM);
                if !a.path.is_empty() {
                    missing.push((key.clone(), a.path.clone()));
                }
            }
        }
        let dur = fmt_dur(a.secs);
        let dur_st = TextStyle::footnote().weight(Weight::Semibold).tabular().right();
        let dur_w = 64.0;
        let name_x = icon.right() + 10.0;
        let bar_x = list.x + list.w * 0.45;
        let bar_w = (list.right() - dur_w - 10.0 - bar_x).max(0.0);
        ui.c.text(&a.name, Rect::new(name_x, y, (bar_x - 12.0 - name_x).max(0.0), ROW_H), &TextStyle::body());
        ui.c.fill_rrect(Rect::new(bar_x, cy - 3.0, bar_w, 6.0), 3.0, Color::white(0.08));
        let fill = (bar_w * a.secs as f32 / most as f32).max(6.0).min(bar_w);
        ui.c.fill_rrect(Rect::new(bar_x, cy - 3.0, fill, 6.0), 3.0, if i == 0 { st.accent } else { Color::white(0.45) });
        ui.c.text(&dur, Rect::new(list.right() - dur_w, y, dur_w, ROW_H), &dur_st);
    }
    if apps.len() > ROWS {
        let more = format!("+{} more", apps.len() - ROWS);
        ui.c.text(&more, Rect::new(list.x, list.y + ROWS as f32 * ROW_H, list.w, 16.0), &TextStyle::caption().color(palette::TEXT_DIM));
    }
    if !missing.is_empty() {
        st.cmd(Command::ScreenIcons(missing));
    }
}
