//! Settings page, behind the gear on the right of the header: the everyday
//! settings. Its last row, Advanced settings, opens the Settings window
//! (`settings_window`) with every setting, and the island closes as the
//! window opens, so the two never overlap. The window draws this page's
//! switch, picker and Check now.
//!
//! What the tray menu also changes comes first: the theme and
//! the idle look as pickers, four switches, and Hide or Stay on top in
//! full-screen apps; this copy's version; and which of the island's own pages
//! (Music, Today, Shelf, Tasks, Focus) have a tab.
//! A change saves at once and applies the way the tray's does
//! (`Command::SaveSettings`).

use crate::activities::text_with_dim;
use crate::commands::Command;
use crate::gfx::{TextStyle, Weight};
use crate::icons::Icon;
use crate::pill::status;
use crate::state::AppState;
use crate::ui::{wid, wid_n, Ui, PILL_SMALL};
use crate::version::VERSION;
use poni_core::geom::{palette, Color, Point, Rect};
use poni_core::settings::{IdleStyle, Theme};

/// The pickers under their labels (or a row each on a narrow island), then
/// the switches and choices (three rows), the tabs, and under a line
/// Advanced settings. Shorter than the System page (256),
/// so the window keeps its size: a new version's Update goes in the
/// version's row, not a row of its own.
const TABS_Y: f32 = SWITCHES_Y + ROWS as f32 * ROW_H;
const ADVANCED_Y: f32 = TABS_Y + ROW_H;
const HEIGHT: f32 = ADVANCED_Y + ROW_H;
const LABELS_Y: f32 = 2.0;
const PICKERS_Y: f32 = 20.0;
const SWITCHES_Y: f32 = 54.0;
const ROW_H: f32 = 28.0;
const COL_GAP: f32 = 24.0;
/// A row's hover wash reaches this far past its text and switch.
const ROW_BLEED: f32 = 8.0;
/// Pickers: the selection's inset in the track, and each choice's padding.
/// Side by side, the theme and idle look pickers may close their choices up
/// to SEG_PAD_MIN to fit their columns (the five themes, and the Pill
/// theme's five idle looks, at the default width).
const SEG_INSET: f32 = 2.0;
const SEG_PAD: f32 = 11.0;
const SEG_PAD_MIN: f32 = 7.5;
/// Where they still don't fit (a narrow island), each takes a row, its
/// label on the left, in the room the labels-above layout uses, so the page
/// keeps its height.
const STACK_Y: [f32; 2] = [0.0, 28.0];
const STACK_GAP: f32 = 16.0;
/// The full-screen choice's picker, inside its row.
const ROW_PICKER_H: f32 = 24.0;
const SWITCH_W: f32 = 30.0;
/// Advanced settings' icon: it opens a window of its own.
const ADVANCED_ICON: f32 = 15.0;
const SWITCH_H: f32 = 18.0;
const KNOB_INSET: f32 = 2.0;
/// The tabs row's chips, like the Record page's inputs.
const CHIP_H: f32 = 24.0;
const CHIP_PAD: f32 = 8.0;
const CHIP_ICON: f32 = 14.0;
const CHIP_ICON_GAP: f32 = 5.0;
const CHIP_GAP: f32 = 6.0;

// Glass: on Windows from a copy of the screen, on a Mac macOS's own glass.
const THEMES: [(Theme, &str); 5] = [(Theme::Classic, "Classic"), (Theme::Oyen, "Oyen"), (Theme::Glass, "Glass"), (Theme::Pill, "Pill"), (Theme::Diorama, "Diorama")];

/// An idle look's name in the picker (`eyes` is Oyen in his theme).
fn idle_name(s: IdleStyle, eyes: &'static str) -> &'static str {
    match s {
        IdleStyle::Clock => "Clock",
        IdleStyle::Eyes => eyes,
        IdleStyle::Minimal => "Minimal",
        IdleStyle::Hidden => "Hidden",
        IdleStyle::Status => "Status",
    }
}

/// The idle looks `theme` offers, as the picker names them.
fn idle_names(theme: Theme) -> Vec<&'static str> {
    // In the Oyen theme the eyes are Oyen's own (as in the tray menu).
    let eyes = if theme == Theme::Oyen { "Oyen" } else { "Eyes" };
    status::looks(theme).iter().map(|s| idle_name(*s, eyes)).collect()
}

/// The switches, as the tray menu names them.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Switch {
    Lyrics,
    Pronunciation,
    Sounds,
    Autostart,
}

/// What a row of the two columns holds: a switch, the full-screen choice
/// (the tray menu's "In full-screen apps" submenu), or this copy's version
/// with Check now.
#[derive(Clone, Copy)]
enum Row {
    Switch(Switch, &'static str),
    Fullscreen,
    Version,
}

const LEFT: &[Row] = &[
    Row::Switch(Switch::Lyrics, "Synced lyrics"),
    Row::Switch(Switch::Pronunciation, "Lyrics pronunciation"),
    Row::Switch(Switch::Sounds, "Sounds"),
];
// The version took the row of the new versions' switch (Clipboard history's
// before it), so the columns stay even.
const RIGHT: &[Row] = &[Row::Version, Row::Fullscreen, Row::Switch(Switch::Autostart, "Start with Windows")];
const COLUMNS: [&[Row]; 2] = [LEFT, RIGHT];
/// Rows in the taller column.
const ROWS: usize = if LEFT.len() > RIGHT.len() { LEFT.len() } else { RIGHT.len() };

/// The full-screen choices as the tray menu names them, and the
/// `hide_in_fullscreen` each one sets.
const FULLSCREEN: [(bool, &str); 2] = [(true, "Hide"), (false, "Stay on top")];

/// Advanced settings, the last row (the self-test presses it).
pub const ADVANCED_ID: &str = "settings.advanced";

pub fn height(_st: &AppState) -> f32 {
    HEIGHT
}

pub fn draw(ui: &mut Ui, st: &mut AppState, area: Rect) {
    let col_w = ((area.w - COL_GAP) / 2.0).max(0.0);
    let cols = [area.x, area.x + col_w + COL_GAP];
    let label_st = TextStyle::caption().weight(Weight::Semibold).color(palette::TEXT_FAINT);

    // Pickers.
    let theme_names: Vec<&str> = THEMES.iter().map(|t| t.1).collect();
    let theme_sel = THEMES.iter().position(|t| t.0 == st.settings.theme).unwrap_or(0);
    let looks = status::looks(st.settings.theme);
    let look_names = idle_names(st.settings.theme);
    let look_sel = looks.iter().position(|s| *s == status::shown(st.settings.theme, st.settings.idle_style)).unwrap_or(0);
    // Side by side under their labels when both fit their columns, each
    // closed up only as far as it needs; else a row each. Whether they fit is
    // asked of the five themes and of this theme's own looks: at the default
    // width both do in every theme, and an island narrower than about 560
    // gives each a row, where every name has its room.
    let theme_pad = pad_to_fit(ui, &theme_names, col_w);
    let side_by_side = theme_pad >= SEG_PAD_MIN && pad_to_fit(ui, &look_names, col_w) >= SEG_PAD_MIN;
    let (theme_pick, look_pick) = if side_by_side {
        ui.c.text("THEME", Rect::new(cols[0], area.y + LABELS_Y, col_w, 14.0), &label_st);
        ui.c.text("IDLE LOOK", Rect::new(cols[1], area.y + LABELS_Y, col_w, 14.0), &label_st);
        let y = area.y + PICKERS_Y;
        let look_pad = pad_to_fit(ui, &look_names, col_w);
        (
            picker_padded(ui, "settings.theme", cols[0], y, PILL_SMALL, &theme_names, theme_sel, theme_pad),
            picker_padded(ui, "settings.idle", cols[1], y, PILL_SMALL, &look_names, look_sel, look_pad),
        )
    } else {
        // A row each, the labels in a column of their own and both pickers
        // starting after it.
        let labels = ["THEME", "IDLE LOOK"];
        let label_w = labels.iter().map(|l| ui.c.measure(l, &label_st, 300.0).0.ceil()).fold(0.0f32, f32::max);
        let x = area.x + label_w + STACK_GAP;
        let pick = |ui: &mut Ui, i: usize, id: &str, names: &[&str], sel: usize| {
            let row = Rect::new(area.x, area.y + STACK_Y[i], area.w, ROW_H);
            ui.c.text(labels[i], Rect::new(row.x, row.y, label_w + 2.0, row.h), &label_st);
            let pad = pad_to_fit(ui, names, row.right() - x).max(0.0);
            picker_padded(ui, id, x, row.center().y - ROW_PICKER_H / 2.0, ROW_PICKER_H, names, sel, pad)
        };
        (pick(ui, 0, "settings.theme", &theme_names, theme_sel), pick(ui, 1, "settings.idle", &look_names, look_sel))
    };
    if let Some(i) = theme_pick {
        st.settings.theme = THEMES[i].0;
        st.cmd(Command::SaveSettings);
    }
    if let Some(i) = look_pick {
        st.settings.idle_style = looks[i];
        st.cmd(Command::SaveSettings);
    }

    // Switches and the two choices, in two columns.
    let accent = st.accent;
    let card = super::card::pill_card(st, area);
    for (c, column) in COLUMNS.iter().enumerate() {
        for (r, &kind) in column.iter().enumerate() {
            let row = Rect::new(cols[c], area.y + SWITCHES_Y + r as f32 * ROW_H, col_w, ROW_H);
            match kind {
                Row::Switch(sw, label) => {
                    // A Mac starts apps at login, as its menu bar says.
                    let label = if cfg!(target_os = "macos") && sw == Switch::Autostart { "Start at login" } else { label };
                    let on = switch_on(st, sw);
                    let id = wid_n("settings.switch", (c * ROWS + r) as u64);
                    if switch_row_on(ui, id, row, label, on, accent, card) {
                        flip(st, sw, on);
                    }
                }
                Row::Version => {
                    text_with_dim(ui, "Version", VERSION, row, TextStyle::body());
                }
                Row::Fullscreen => {
                    let sel = FULLSCREEN.iter().position(|f| f.0 == st.settings.hide_in_fullscreen).unwrap_or(0);
                    if let Some(i) = choice_row(ui, row, "In full-screen apps", "settings.fullscreen", &FULLSCREEN.map(|f| f.1), sel) {
                        st.settings.hide_in_fullscreen = FULLSCREEN[i].0;
                        st.cmd(Command::SaveSettings);
                    }
                }
            }
        }
    }
    draw_tabs(ui, st, Rect::new(area.x, area.y + TABS_Y, area.w, ROW_H));
    draw_advanced(ui, st, Rect::new(area.x, area.y + ADVANCED_Y, area.w, ROW_H), card);
}

/// The last row, under a line: Advanced settings, the window with every
/// setting (`Command::OpenSettings`; the island closes as it opens). The
/// whole row is the button, as a switch's row is.
fn draw_advanced(ui: &mut Ui, st: &mut AppState, r: Rect, card: Option<(Rect, f32)>) {
    let px = 1.0 / ui.c.scale().max(0.5);
    ui.c.fill_rect(Rect::new(r.x, r.y, r.w, px), Color::white(0.10));
    let hit = Rect::new(r.x - ROW_BLEED, r.y, r.w + 2.0 * ROW_BLEED, r.h);
    let resp = ui.button(wid(ADVANCED_ID), hit);
    let lit = resp.hovered || resp.pressed;
    if lit {
        super::card::fill_box(ui, card, hit.inset(0.0, 1.0), 8.0, Color::white(if resp.pressed { 0.10 } else { 0.06 }));
    }
    let icon = Rect::new(r.right() - ADVANCED_ICON, r.center().y - ADVANCED_ICON / 2.0, ADVANCED_ICON, ADVANCED_ICON);
    ui.c.icon(Icon::Open, icon, if lit { palette::TEXT } else { palette::TEXT_DIM });
    ui.c.text("Advanced settings", Rect::new(r.x, r.y, (icon.x - 10.0 - r.x).max(0.0), r.h), &TextStyle::body());
    if resp.clicked {
        st.cmd(Command::OpenSettings);
    }
}

/// "Tabs", then a chip for each of the island's own pages: on, its icon and
/// name; off (no tab in the row), the icon struck through and dim, as the
/// Record page marks an input that's off. A click flips it. Icons only
/// where the names don't fit. Notifications' chip shows only where the copy
/// can read them, and switches the whole of it off, not just its tab.
fn draw_tabs(ui: &mut Ui, st: &mut AppState, r: Rect) {
    let body = TextStyle::body();
    let label_w = ui.c.measure("Tabs", &body, 200.0).0.ceil();
    ui.c.text("Tabs", Rect::new(r.x, r.y, label_w + 1.0, r.h), &body);
    let chip_st = TextStyle::footnote().weight(Weight::Semibold);
    let pages = super::hideable_tabs(st);
    let gaps = CHIP_GAP * (pages.len() - 1) as f32;
    let full: Vec<f32> =
        pages.iter().map(|p| (CHIP_PAD + CHIP_ICON + CHIP_ICON_GAP + ui.c.measure(p.title(), &chip_st, 200.0).0 + CHIP_PAD).ceil()).collect();
    let names = full.iter().sum::<f32>() + gaps <= r.w - label_w - 12.0;
    let widths: Vec<f32> = if names { full } else { vec![CHIP_H + 8.0; pages.len()] };
    let mut x = r.right() - widths.iter().sum::<f32>() - gaps;
    let y = r.center().y - CHIP_H / 2.0;
    let mut flip = None;
    for (i, (&p, w)) in pages.iter().zip(widths).enumerate() {
        let c = Rect::new(x, y, w, CHIP_H);
        x += w + CHIP_GAP;
        let on = !st.tab_hidden(p);
        let resp = ui.button(wid_n("settings.tab", i as u64), c);
        if resp.clicked {
            flip = Some(p);
        }
        let fill = match (on, resp.pressed, resp.hovered) {
            (_, true, _) => Color::white(0.22),
            (true, _, true) => Color::white(0.20),
            (true, _, false) => Color::white(0.16),
            (false, _, true) => Color::white(0.12),
            (false, _, false) => palette::SURFACE,
        };
        ui.c.fill_rrect(c, CHIP_H / 2.0, fill);
        let color = if on { palette::TEXT } else { palette::TEXT_DIM };
        let ix = if names { c.x + CHIP_PAD } else { c.center().x - CHIP_ICON / 2.0 };
        let ir = Rect::new(ix, c.center().y - CHIP_ICON / 2.0, CHIP_ICON, CHIP_ICON);
        ui.c.icon(p.icon(), ir, color);
        if !on {
            ui.c.line(Point::new(ir.x, ir.bottom()), Point::new(ir.right(), ir.y), color, 1.5);
        }
        if names {
            let tx = ix + CHIP_ICON + CHIP_ICON_GAP;
            ui.c.text(p.title(), Rect::new(tx, c.y, (c.right() - tx).max(0.0), c.h), &chip_st.color(color));
        }
    }
    if let Some(p) = flip {
        let hidden = &mut st.settings.hidden_tabs;
        match hidden.iter().position(|k| k == p.key()) {
            Some(i) => {
                hidden.remove(i);
            }
            None => hidden.push(p.key().to_string()),
        }
        st.cmd(Command::SaveSettings);
    }
}

/// A choice's label with its picker where the other rows have their
/// switch. Returns the choice clicked, if any.
fn choice_row(ui: &mut Ui, r: Rect, label: &str, id: &str, names: &[&str], sel: usize) -> Option<usize> {
    let x = r.right() - picker_width(ui, names);
    ui.c.text(label, Rect::new(r.x, r.y, (x - 10.0 - r.x).max(0.0), r.h), &TextStyle::body());
    picker(ui, id, x, r.center().y - ROW_PICKER_H / 2.0, ROW_PICKER_H, names, sel)
}

/// Where a switch reads from. Start with Windows is read off the UI thread
/// when the page opens; until then it shows off, dimmed.
fn switch_on(st: &AppState, sw: Switch) -> Option<bool> {
    let s = &st.settings;
    match sw {
        Switch::Lyrics => Some(s.lyrics),
        Switch::Pronunciation => Some(s.lyrics_pronunciation),
        Switch::Sounds => Some(s.sounds),
        Switch::Autostart => st.ui.autostart,
    }
}

fn flip(st: &mut AppState, sw: Switch, on: Option<bool>) {
    let s = &mut st.settings;
    match sw {
        Switch::Lyrics => s.lyrics = !s.lyrics,
        Switch::Pronunciation => s.lyrics_pronunciation = !s.lyrics_pronunciation,
        Switch::Sounds => s.sounds = !s.sounds,
        Switch::Autostart => {
            if let Some(on) = on {
                st.cmd(Command::SetAutostart(!on));
            }
            return;
        }
    }
    st.cmd(Command::SaveSettings);
}

// ---------------------------------------------------------------- widgets

/// The widest padding (up to SEG_PAD) that fits a picker of `names` into `w`.
fn pad_to_fit(ui: &mut Ui, names: &[&str], w: f32) -> f32 {
    let n = names.len().max(1) as f32;
    let text: f32 = names.iter().map(|s| ui.c.measure(s, &picker_text(), 300.0).0).sum();
    // Each choice's width is rounded up: up to a DIP each.
    ((w - 2.0 * SEG_INSET - text - n) / (2.0 * n)).min(SEG_PAD)
}

/// Choices side by side in a track `h` tall, the chosen one lifted (the
/// Focus page's picker, a size smaller). Returns the choice clicked, if any.
pub(crate) fn picker(ui: &mut Ui, id: &str, x: f32, y: f32, h: f32, names: &[&str], sel: usize) -> Option<usize> {
    picker_padded(ui, id, x, y, h, names, sel, SEG_PAD)
}

/// [`picker`] with `pad` around each choice's name.
#[allow(clippy::too_many_arguments)]
fn picker_padded(ui: &mut Ui, id: &str, x: f32, y: f32, h: f32, names: &[&str], sel: usize, pad: f32) -> Option<usize> {
    let st = picker_text();
    let inner_h = h - 2.0 * SEG_INSET;
    let mut cx = x + SEG_INSET;
    let cells: Vec<Rect> = names
        .iter()
        .map(|n| {
            let w = choice_width(ui, n, pad);
            let cell = Rect::new(cx, y + SEG_INSET, w, inner_h);
            cx += w;
            cell
        })
        .collect();
    ui.c.fill_rrect(Rect::new(x, y, cx + SEG_INSET - x, h), h / 2.0, palette::SURFACE);
    let mut picked = None;
    for (i, (name, cell)) in names.iter().zip(&cells).enumerate() {
        let color = if i == sel {
            ui.c.fill_rrect(*cell, inner_h / 2.0, Color::white(0.16));
            palette::TEXT
        } else {
            let resp = ui.button(wid_n(id, i as u64), Rect::new(cell.x, y, cell.w, h));
            if resp.clicked {
                picked = Some(i);
            }
            if resp.pressed || resp.hovered {
                ui.c.fill_rrect(*cell, inner_h / 2.0, Color::white(if resp.pressed { 0.10 } else { 0.06 }));
                palette::TEXT
            } else {
                palette::TEXT_DIM
            }
        };
        ui.c.text(name, *cell, &st.color(color).center());
    }
    picked
}

fn picker_text() -> TextStyle {
    TextStyle::footnote().weight(Weight::Semibold)
}

/// One choice in a picker: its name and the padding around it.
fn choice_width(ui: &mut Ui, name: &str, pad: f32) -> f32 {
    (2.0 * pad + ui.c.measure(name, &picker_text(), 300.0).0).ceil()
}

/// A whole picker: its choices and the track's inset.
pub(crate) fn picker_width(ui: &mut Ui, names: &[&str]) -> f32 {
    2.0 * SEG_INSET + names.iter().map(|n| choice_width(ui, n, SEG_PAD)).sum::<f32>()
}

/// A label with a switch at the row's right end; the whole row toggles.
/// `None` = not known yet: drawn off and dimmed, and it doesn't click.
/// Returns true when clicked.
pub(crate) fn switch_row(ui: &mut Ui, id: u64, r: Rect, label: &str, on: Option<bool>, accent: Color) -> bool {
    switch_row_on(ui, id, r, label, on, accent, None)
}

/// [`switch_row`] on the open card (`card` from `card::pill_card`): in
/// Pill, a bottom row's hover wash follows the card's rounder corners.
fn switch_row_on(ui: &mut Ui, id: u64, r: Rect, label: &str, on: Option<bool>, accent: Color, card: Option<(Rect, f32)>) -> bool {
    let hit = Rect::new(r.x - ROW_BLEED, r.y, r.w + 2.0 * ROW_BLEED, r.h);
    let resp = if on.is_some() { ui.button(id, hit) } else { Default::default() };
    if resp.hovered || resp.pressed {
        super::card::fill_box(ui, card, hit.inset(0.0, 1.0), 8.0, Color::white(if resp.pressed { 0.10 } else { 0.06 }));
    }
    let sw = Rect::new(r.right() - SWITCH_W, r.center().y - SWITCH_H / 2.0, SWITCH_W, SWITCH_H);
    ui.c.text(label, Rect::new(r.x, r.y, (sw.x - 10.0 - r.x).max(0.0), r.h), &TextStyle::body());
    let alpha = if on.is_some() { 1.0 } else { 0.5 };
    ui.c.with_alpha(alpha, |c| draw_switch(c, sw, on == Some(true), accent));
    resp.clicked
}

/// A switch's track and knob (also the Add-ons page's).
pub(crate) fn draw_switch(c: &mut crate::gfx::Canvas, r: Rect, on: bool, accent: Color) {
    c.fill_rrect(r, r.h / 2.0, if on { accent } else { palette::TRACK });
    let d = r.h - 2.0 * KNOB_INSET;
    let x = if on { r.right() - KNOB_INSET - d / 2.0 } else { r.x + KNOB_INSET + d / 2.0 };
    c.fill_circle(Point::new(x, r.center().y), d / 2.0, palette::ON_ACCENT);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where a theme comes in the picker. No `_` arm: a theme added to the
    /// enum stops this compiling until it has a place, and then the test
    /// below asks the picker for it.
    fn place(t: Theme) -> usize {
        match t {
            Theme::Classic => 0,
            Theme::Oyen => 1,
            Theme::Glass => 2,
            Theme::Pill => 3,
            Theme::Diorama => 4,
        }
    }

    #[test]
    fn the_theme_picker_has_every_theme_by_its_name() {
        assert_eq!(THEMES.map(|t| t.1), ["Classic", "Oyen", "Glass", "Pill", "Diorama"]);
        for (i, (theme, name)) in THEMES.iter().enumerate() {
            assert_eq!(place(*theme), i, "{name}");
        }
    }

    #[test]
    fn the_idle_look_picker_names_each_themes_own_looks() {
        for theme in [Theme::Classic, Theme::Glass] {
            assert_eq!(idle_names(theme), ["Clock", "Eyes", "Minimal", "Hidden"], "{theme:?}");
        }
        // Diorama rests as its picture, with the time or alone: no eyes.
        assert_eq!(idle_names(Theme::Diorama), ["Clock", "Minimal", "Hidden"]);
        // In his theme the eyes are Oyen's own, and the status bar is Pill's.
        assert_eq!(idle_names(Theme::Oyen), ["Clock", "Oyen", "Minimal", "Hidden"]);
        assert_eq!(idle_names(Theme::Pill), ["Clock", "Eyes", "Minimal", "Hidden", "Status"]);
        // A name for every look a theme offers, one choice each.
        for (theme, _) in THEMES {
            assert_eq!(idle_names(theme).len(), status::looks(theme).len(), "{theme:?}");
        }
    }
}
