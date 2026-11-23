//! The Smoke theme: the Classic island in translucent black, so what's
//! behind it shows through, and its own music layout.
//!
//! While music plays the closed island is two rows tall:
//!
//! * the album art on the left, the time in the middle, the equalizer in the
//!   album's colour on the right;
//! * under them the line being sung, or the song ("Title · artist") while
//!   nothing is sung and for a moment after a new track starts (the shared
//!   middle, `pill::media::middle`);
//! * along the bottom, how far the song has played.
//!
//! The island is as wide as its line needs, from [`MIN_W`] to [`MAX_W`].
//! Everything else (pages, pop-ups, the resting island) is Classic's, in the
//! same translucent black.

use crate::activities;
use crate::gfx::{TextStyle, Weight};
use crate::island::Geom;
use crate::state::AppState;
use crate::ui::Ui;
use poni_core::datetime::fmt_clock;
use poni_core::geom::{palette, Color, Rect};
use poni_core::settings::Theme;

/// How opaque the black is open, at least (the pages' text needs a
/// steadier ground than a glance at the song does). Closed, it's the
/// Transparency setting (`Settings::smoke_opacity_pct`).
pub const ALPHA_OPEN: f32 = 0.82;

/// The music island's width: at least this (room for the time, the art and
/// the bars, and a short title), at most that.
const MIN_W: f32 = 360.0;
const MAX_W: f32 = 520.0;
/// Widths go in steps, so lines a few DIPs apart don't each morph it.
const W_STEP: f32 = 8.0;
/// Space between the island's sides and what's in it.
const PAD: f32 = 18.0;
/// Two rows and the progress bar; a song sung with readings under its
/// lines needs a little more for the second row.
const H: f32 = 70.0;
const H_READS: f32 = 84.0;
/// One row (and the progress bar) while there's nothing to show under it:
/// a song without lyrics whose title shows only for a moment.
const H_ONE: f32 = 46.0;
const RADIUS_ONE: f32 = 18.0;
const RADIUS: f32 = 22.0;
/// The first row's middle, from the top.
const ROW1_Y: f32 = 19.0;
/// The second row's middle, from the top (and with readings).
const ROW2_Y: f32 = 45.0;
const ROW2_Y_READS: f32 = 52.0;
const ART: f32 = 24.0;
const EQ_W: f32 = 20.0;
const EQ_H: f32 = 16.0;
/// The progress bar: this far in from the sides (clear of the rounded
/// corners) and up from the bottom, this thick.
const BAR_INSET: f32 = 26.0;
const BAR_UP: f32 = 8.0;
const BAR_H: f32 = 3.0;
/// What the shared middle adds to its text for the Pill's art and bars.
const MIDDLE_SIDES: f32 = 88.0;

pub fn on(theme: Theme) -> bool {
    theme == Theme::Smoke
}

/// How opaque the closed island's black is (the Transparency setting).
pub fn alpha_closed(s: &poni_core::settings::Settings) -> f32 {
    s.smoke_opacity_pct as f32 / 100.0
}

/// How opaque the open island's black is: as closed, and never less than
/// [`ALPHA_OPEN`].
pub fn alpha_open(s: &poni_core::settings::Settings) -> f32 {
    alpha_closed(s).max(ALPHA_OPEN)
}

/// The island's black: translucent, more so while closed.
pub fn ground(s: &poni_core::settings::Settings, expanded: bool) -> Color {
    Color::black(if expanded { alpha_open(s) } else { alpha_closed(s) })
}

/// The music island's size.
pub fn media_geom(st: &AppState) -> Geom {
    let text = (crate::pill::media::fit_w() - MIDDLE_SIDES).max(0.0);
    let w = (((text + 2.0 * PAD) / W_STEP).ceil() * W_STEP).clamp(MIN_W, MAX_W);
    if !crate::pill::media::shows_song(st) {
        return Geom::new(w, H_ONE, RADIUS_ONE);
    }
    let h = if crate::pill::lyric::reads(st) { H_READS } else { H };
    Geom::new(w, h, RADIUS)
}

/// Music, closed, the Smoke way.
pub fn draw_media(ui: &mut Ui, st: &mut AppState, body: Rect) {
    let playing = st.media.as_ref().is_some_and(|m| m.is_playing());
    let reads = crate::pill::lyric::reads(st);
    // Row one: the art, the time, the bars.
    let cy = body.y + ROW1_Y;
    let art = Rect::new(body.x + PAD, cy - ART / 2.0, ART, ART);
    activities::art_or_icon(ui, st, art, playing);
    let eq = Rect::new(body.right() - PAD - EQ_W, cy - EQ_H / 2.0, EQ_W, EQ_H);
    activities::equalizer(ui, eq, st.media_accent(), playing, st.now);
    let clock = fmt_clock(&st.local, st.settings.use_24h);
    let clock_r = Rect::new(art.right() + 8.0, cy - 10.0, (eq.x - art.right() - 16.0).max(0.0), 20.0);
    ui.c.text(&clock, clock_r, &TextStyle::body().weight(Weight::Semibold).tabular().center());
    // Row two: the line being sung, or the song.
    let row_y = body.y + if reads { ROW2_Y_READS } else { ROW2_Y };
    let row_h = crate::pill::media::height(st);
    let row = Rect::new(body.x, row_y - row_h / 2.0, body.w, row_h);
    let alpha = if playing { 1.0 } else { crate::pill::media::PAUSED_TEXT };
    // The song gets the whole row (a long one runs across it).
    crate::pill::media::middle(ui, st, row, body.x + PAD, body.right() - PAD, alpha, body.w - 2.0 * PAD);
    // The progress bar.
    if let Some(m) = st.media.as_ref().filter(|m| st.settings.music_progress && m.timeline.has_duration()) {
        let track = Rect::new(body.x + BAR_INSET, body.bottom() - BAR_UP - BAR_H, (body.w - 2.0 * BAR_INSET).max(0.0), BAR_H);
        ui.c.fill_rrect(track, BAR_H / 2.0, Color::white(0.18));
        let done = m.timeline.progress_at(st.now).clamp(0.0, 1.0);
        if done > 0.0 {
            let fill = Rect::new(track.x, track.y, (track.w * done).max(BAR_H), BAR_H);
            ui.c.fill_rrect(fill, BAR_H / 2.0, palette::TEXT.mul_alpha(if playing { 0.85 } else { 0.5 }));
        }
    }
}
