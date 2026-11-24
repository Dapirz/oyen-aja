//! Notes page, the Meeting notes add-on's (off until switched on): one click
//! and the island records a meeting, then Gemini, with the person's own key,
//! writes its notes and transcript (`poni_core::notes`).
//!
//! - Before a meeting: the record button, what it hears (the microphone and
//!   the PC's own sound: a call in Meet, Zoom or Teams, with no bot joining
//!   it), the Gemini key when there is none yet, and the notes saved last
//!   with Open.
//! - Listening: the time under the button, a bar for each source, and Stop.
//! - After Stop: the meeting going up, then Gemini writing.
//! - The notes: the title, the summary's first lines, how many action
//!   items, and Open, Copy, the folder and a new meeting.
//! - What went wrong, in plain words, with Try again while the recording
//!   is kept.
//!
//! The page only draws and asks (`Command::Notes`); the app runs the
//! recording and the handing over (`services::notes`). It moves only while
//! it listens (the bars follow the voices), and rests otherwise. From
//! Oyenza's Notes page (removed in e031e17), without Oyen.

use crate::commands::{Command, NotesCmd};
use crate::gfx::{TextStyle, Weight};
use crate::icons::Icon;
use crate::state::AppState;
use crate::ui::{wid, wid_n, PillStyle, Ui, PILL_GAP, PILL_SMALL};
use poni_core::geom::{palette, Color, Point, Rect};
use poni_core::notes::{fmt_length, CaptureIssue, Phase, Problem};

/// Every state is this tall, so the island never jumps between them.
const HEIGHT: f32 = 172.0;
/// The button's column, and the gap to the text.
const LEFT_W: f32 = 120.0;
const COL_GAP: f32 = 18.0;
const BUTTON_R: f32 = 26.0;
const BUTTON_Y: f32 = 62.0;
/// The level bars' rows.
const BAR_H: f32 = 6.0;
const METER_ROW: f32 = 26.0;
const REC: Color = palette::RED;
const MIC_SETTINGS: &str = "ms-settings:privacy-microphone";
const TEXT_W: f32 = 440.0;

pub fn height(_st: &AppState) -> f32 {
    HEIGHT
}

/// The time under the button moves on each second while it listens.
pub fn next_wake(st: &AppState) -> Option<f64> {
    st.notes.next_second(st.now).map(|b| st.now + b)
}

pub fn draw(ui: &mut Ui, st: &mut AppState, area: Rect) {
    // Fixed layout height: the island's spring may still be opening.
    let area = Rect::new(area.x, area.y, area.w, HEIGHT);
    let left = Rect::new(area.x, area.y, LEFT_W, area.h);
    let right = Rect::new(area.x + LEFT_W + COL_GAP, area.y, (area.w - LEFT_W - COL_GAP).max(0.0), area.h);
    let phase = st.notes.phase.clone();
    draw_button(ui, st, &phase, Point::new(left.center().x, left.y + BUTTON_Y));
    match phase {
        Phase::Idle => draw_idle(ui, st, right),
        Phase::Listening { .. } => draw_listening(ui, st, right),
        Phase::Sending { sent, parts } => draw_sending(ui, right, sent, parts),
        Phase::Writing { since } => draw_writing(ui, st, right, since),
        Phase::Ready => draw_ready(ui, st, right),
        Phase::Failed(p) => draw_failed(ui, st, right, p),
    }
}

/// The round button: record (Take notes), Stop while listening, faint
/// while the notes are on their way. Its line under it.
fn draw_button(ui: &mut Ui, st: &mut AppState, phase: &Phase, c: Point) {
    let listening = matches!(phase, Phase::Listening { .. });
    // A kept recording waits for Try again or Throw away before a new one.
    let busy = matches!(phase, Phase::Sending { .. } | Phase::Writing { .. }) || (matches!(phase, Phase::Failed(_)) && st.ui.notes_kept);
    let r = Rect::from_center(c, 2.0 * BUTTON_R, 2.0 * BUTTON_R);
    let resp = if busy { Default::default() } else { ui.button(wid("notes.button"), r) };
    let lift = |col: Color| if resp.pressed { Color::lerp(col, Color::white(1.0), 0.2) } else if resp.hovered { Color::lerp(col, Color::white(1.0), 0.1) } else { col };
    let under = Rect::new(c.x - 60.0, c.y + BUTTON_R + 10.0, 120.0, 18.0);
    if listening {
        let level = st.notes.mic.max(st.notes.pc).clamp(0.0, 1.0);
        ui.c.fill_circle(c, BUTTON_R + 4.0 + 10.0 * level.sqrt(), REC.with_alpha(0.22));
        ui.c.fill_circle(c, BUTTON_R, lift(REC));
        ui.c.icon(Icon::Stop, Rect::from_center(c, 22.0, 22.0), palette::ON_ACCENT);
        let t = fmt_length(st.notes.elapsed(st.now));
        ui.c.text(&t, under, &TextStyle::body().weight(Weight::Semibold).tabular().center().color(REC));
        if resp.clicked {
            st.cmd(Command::Notes(NotesCmd::Stop));
        }
        return;
    }
    let fill = if busy { palette::SURFACE } else { REC };
    ui.c.fill_circle(c, BUTTON_R, lift(fill));
    ui.c.fill_circle(c, 9.0, if busy { palette::TEXT_FAINT } else { palette::ON_ACCENT });
    let line = match phase {
        Phase::Sending { .. } | Phase::Writing { .. } => "Taking notes\u{2026}",
        _ if busy => "Recording kept",
        _ => "Take notes",
    };
    ui.c.text(line, under, &TextStyle::footnote().weight(Weight::Semibold).center().color(if busy { palette::TEXT_FAINT } else { palette::TEXT }));
    if resp.clicked {
        st.cmd(Command::Notes(NotesCmd::Start));
    }
}

fn headline(ui: &mut Ui, r: Rect, text: &str) -> f32 {
    let st = TextStyle::headline().wrap().top();
    let (_, h) = ui.c.measure(text, &st, r.w);
    ui.c.text(text, Rect::new(r.x, r.y, r.w, h + 1.0), &st);
    r.y + h
}

fn para(ui: &mut Ui, r: Rect, text: &str, color: Color) -> f32 {
    let st = TextStyle::footnote().color(color).wrap().top();
    let (_, h) = ui.c.measure(text, &st, r.w);
    ui.c.text(text, Rect::new(r.x, r.y, r.w, h + 1.0), &st);
    r.y + h
}

/// Capsules left to right from (`x`, `y`): each one's click, in order.
fn buttons(ui: &mut Ui, x: f32, y: f32, list: &[(&str, &str, Option<Icon>, PillStyle)]) -> Vec<bool> {
    let mut x = x;
    list.iter()
        .map(|(id, label, icon, style)| {
            let w = ui.pill_width(label, icon.is_some(), true);
            let clicked = ui.pill(wid(id), Rect::new(x, y, w, PILL_SMALL), label, *icon, *style).clicked;
            x += w + PILL_GAP;
            clicked
        })
        .collect()
}

/// Make a key, and paste it: for the page with no key yet, and a problem
/// with the key.
fn key_buttons(ui: &mut Ui, st: &mut AppState, x: f32, y: f32) {
    let accent = st.accent;
    let clicks = buttons(
        ui,
        x,
        y,
        &[("notes.paste-key", "Paste key", None, PillStyle::Primary(accent)), ("notes.get-key", "Get a key", Some(Icon::Open), PillStyle::Utility)],
    );
    if clicks[0] {
        st.cmd(Command::Notes(NotesCmd::PasteKey));
    }
    if clicks[1] {
        st.cmd(Command::OpenUrl(poni_core::notes::KEY_URL.into()));
    }
}

/// A short line by the buttons until its time ("Copied", "Key saved").
fn note(ui: &mut Ui, st: &AppState, r: Rect) {
    if let Some((text, until)) = st.ui.notes_note.clone() {
        if ui.now < until {
            let w = ui.c.measure(&text, &TextStyle::caption(), 240.0).0 + 2.0;
            ui.c.text(&text, Rect::new(r.right() - w, r.y, w, r.h), &TextStyle::caption().color(st.accent));
        }
    }
}

fn draw_idle(ui: &mut Ui, st: &mut AppState, r: Rect) {
    let r = Rect::new(r.x, r.y + 4.0, r.w.min(TEXT_W), r.h);
    let foot = Rect::new(r.x, r.bottom() - PILL_SMALL - 6.0, r.w, PILL_SMALL);
    if st.ui.notes_key.is_none() {
        let y = headline(ui, r, "Meeting notes, written by Gemini");
        let body = "Paste your Gemini API key to start: make one free at Google AI Studio and copy it. It's sealed on this PC for your Windows account and only goes to Google.";
        para(ui, Rect::new(r.x, y + 6.0, r.w, 80.0), body, palette::TEXT_DIM);
        key_buttons(ui, st, foot.x, foot.y);
        note(ui, st, Rect::new(foot.x, foot.y - 18.0, foot.w, 15.0));
        return;
    }
    let y = headline(ui, r, "Take notes of a meeting");
    if st.notes_saved.is_empty() {
        let body = "Records your microphone and your PC's sound, so a call in Meet, Zoom or Teams is heard with no bot joining. Stop when it's over: Gemini writes the notes and the transcript, saved to Documents \u{203a} Hihu Island \u{203a} Notes.";
        let y = para(ui, Rect::new(r.x, y + 6.0, r.w, 80.0), body, palette::TEXT_DIM);
        para(ui, Rect::new(r.x, (y + 10.0).max(foot.y + 6.0), r.w, 30.0), "Let people know you're taking notes.", palette::TEXT_FAINT);
        return;
    }
    // The notes saved lately: open one, or send it to the Recycle Bin.
    let y = para(ui, Rect::new(r.x, y + 4.0, r.w, 40.0), "Your microphone and your PC's sound; Gemini writes the notes.", palette::TEXT_DIM) + 8.0;
    let row_h = 26.0;
    let room = ((r.bottom() - y - 18.0) / row_h).floor().max(1.0) as usize;
    let rows: Vec<(String, String)> = st.notes_saved.iter().take(room).map(|(_, w, t)| (w.clone(), t.clone())).collect();
    for (i, (when, title)) in rows.iter().enumerate() {
        let row = Rect::new(r.x, y + i as f32 * row_h, r.w, row_h);
        let resp = ui.button(wid_n("notes.saved", i as u64), row);
        if resp.hovered {
            ui.c.fill_rrect(row.inset(-6.0, 1.0), 7.0, Color::white(0.05));
        }
        let ws = TextStyle::caption().color(palette::TEXT_FAINT);
        ui.c.text(when, Rect::new(row.x, row.y, 86.0, row.h), &ws);
        let bin = Rect::new(row.right() - 22.0, row.y + 2.0, 22.0, 22.0);
        ui.c.text(title, Rect::new(row.x + 90.0, row.y, (bin.x - row.x - 96.0).max(0.0), row.h), &TextStyle::footnote());
        let mut used = false;
        if resp.hovered && ui.icon_button(wid_n("notes.saved.bin", i as u64), bin, Icon::Trash, palette::TEXT_DIM, 12.0).clicked {
            st.cmd(Command::Notes(NotesCmd::RecycleSaved(i)));
            used = true;
        }
        if resp.clicked && !used && !ui.hovered(bin) {
            st.cmd(Command::Notes(NotesCmd::OpenSaved(i)));
        }
    }
    let more = st.notes_saved.len().saturating_sub(rows.len());
    let link = if more > 0 { format!("+{more} more in the folder") } else { "Open the folder".to_string() };
    let lr = Rect::new(r.x, y + rows.len() as f32 * row_h + 2.0, r.w, 16.0);
    let lw = ui.c.measure(&link, &TextStyle::caption(), 300.0).0 + 4.0;
    let lrect = Rect::new(lr.x, lr.y, lw, lr.h);
    let lresp = ui.button(wid("notes.saved.folder"), lrect);
    ui.c.text(&link, lrect, &TextStyle::caption().color(if lresp.hovered { palette::TEXT } else { palette::TEXT_DIM }));
    if lresp.clicked {
        st.cmd(Command::Notes(NotesCmd::Folder));
    }
}

fn draw_listening(ui: &mut Ui, st: &mut AppState, r: Rect) {
    let r = Rect::new(r.x, r.y + 4.0, r.w.min(TEXT_W), r.h);
    let y = headline(ui, r, "Listening");
    let y = para(ui, Rect::new(r.x, y + 4.0, r.w, 40.0), "Stop when the meeting ends, and Gemini writes it up.", palette::TEXT_DIM) + 14.0;
    let accent = st.accent;
    let rows = [("You", "microphone", st.notes.mic_on, st.notes.mic), ("The call", "PC sound", st.notes.pc_on, st.notes.pc)];
    for (i, (name, source, on, level)) in rows.into_iter().enumerate() {
        let ry = y + i as f32 * METER_ROW;
        let label = TextStyle::footnote().weight(Weight::Semibold);
        ui.c.text(name, Rect::new(r.x, ry, 70.0, 16.0), &label);
        let bar = Rect::new(r.x + 74.0, ry + 5.0, (r.w - 74.0).max(0.0), BAR_H);
        if on {
            ui.c.fill_rrect(bar, BAR_H / 2.0, palette::SURFACE);
            // A square-root scale, so quiet speech still moves the bar.
            let w = (bar.w * level.clamp(0.0, 1.0).sqrt()).max(BAR_H);
            ui.c.fill_rrect(Rect::new(bar.x, bar.y, w, bar.h), BAR_H / 2.0, accent);
        } else {
            let off = format!("No {source}");
            ui.c.text(&off, Rect::new(bar.x, ry, bar.w, 16.0), &TextStyle::footnote().color(palette::TEXT_FAINT));
        }
    }
    let foot = y + 2.0 * METER_ROW + 6.0;
    para(ui, Rect::new(r.x, foot, r.w, 30.0), "Let people know you're taking notes.", palette::TEXT_FAINT);
    // The bars follow the voices.
    ui.request_live();
}

fn draw_sending(ui: &mut Ui, r: Rect, sent: u32, parts: u32) {
    let r = Rect::new(r.x, r.y + 4.0, r.w.min(TEXT_W), r.h);
    let y = headline(ui, r, "Sending the meeting to Gemini");
    let line = if parts > 1 { format!("Part {} of {parts}", (sent + 1).min(parts)) } else { "One part".to_string() };
    let y = para(ui, Rect::new(r.x, y + 4.0, r.w, 20.0), &line, palette::TEXT_DIM) + 12.0;
    let bar = Rect::new(r.x, y, r.w, BAR_H);
    ui.c.fill_rrect(bar, BAR_H / 2.0, palette::SURFACE);
    let done = if parts == 0 { 0.0 } else { sent as f32 / parts as f32 };
    ui.c.fill_rrect(Rect::new(bar.x, bar.y, (bar.w * done).max(BAR_H), bar.h), BAR_H / 2.0, palette::TEXT_DIM);
    para(ui, Rect::new(r.x, y + 20.0, r.w, 40.0), "You can close the island: the notes come by themselves.", palette::TEXT_FAINT);
}

fn draw_writing(ui: &mut Ui, st: &AppState, r: Rect, since: f64) {
    let r = Rect::new(r.x, r.y + 4.0, r.w.min(TEXT_W), r.h);
    let y = headline(ui, r, "Gemini is writing your notes\u{2026}");
    // About a minute for every half hour of meeting, as Flash takes it.
    let mins = (st.notes.secs / 1800.0).ceil().max(1.0) as u32;
    let line = format!("{} so far \u{b7} usually about {mins} min for a meeting this long", fmt_length(st.now - since));
    let y = para(ui, Rect::new(r.x, y + 4.0, r.w, 36.0), &line, palette::TEXT_DIM);
    para(ui, Rect::new(r.x, y + 12.0, r.w, 40.0), "You can close the island: the notes come by themselves.", palette::TEXT_FAINT);
}

fn draw_ready(ui: &mut Ui, st: &mut AppState, r: Rect) {
    // Only what the page shows: a meeting's transcript can run to thousands of lines.
    let Some((title, secs, s, actions, summary)) = st.notes.last.as_ref().map(|l| (l.notes.title.clone(), l.notes.secs, l.started, l.notes.actions.len(), l.notes.summary.iter().take(6).cloned().collect::<Vec<_>>())) else {
        return;
    };
    let r = Rect::new(r.x, r.y + 2.0, r.w.min(TEXT_W), r.h);
    ui.c.text(&title, Rect::new(r.x, r.y, r.w, 20.0), &TextStyle::headline());
    let when = format!("{:02}:{:02} \u{b7} {} min \u{b7} {actions} action item{}", s.hour, s.minute, (secs as f32 / 60.0).round().max(1.0), if actions == 1 { "" } else { "s" });
    ui.c.text(&when, Rect::new(r.x, r.y + 22.0, r.w, 15.0), &TextStyle::caption().color(palette::TEXT_DIM));
    // The summary's first lines, as many as fit above the buttons.
    let buttons_y = r.bottom() - PILL_SMALL - 6.0;
    let mut y = r.y + 44.0;
    let body = TextStyle::footnote().wrap().top();
    ui.c.clip(Rect::new(r.x, y, r.w, buttons_y - 8.0 - y));
    for line in &summary {
        let text = format!("\u{2022}  {line}");
        let (_, h) = ui.c.measure(&text, &body, r.w);
        if y + h > buttons_y - 8.0 {
            break;
        }
        ui.c.text(&text, Rect::new(r.x, y, r.w, h + 1.0), &body);
        y += h + 3.0;
    }
    ui.c.unclip();
    let accent = st.accent;
    let clicks = buttons(
        ui,
        r.x,
        buttons_y,
        &[
            ("notes.open", "Open notes", Some(Icon::Open), PillStyle::Primary(accent)),
            ("notes.copy", "Copy", Some(Icon::Copy), PillStyle::Neutral),
            ("notes.folder", "", Some(Icon::Folder), PillStyle::Utility),
            ("notes.done", "Done", None, PillStyle::Utility),
        ],
    );
    for (clicked, cmd) in clicks.into_iter().zip([NotesCmd::Open, NotesCmd::Copy, NotesCmd::Folder, NotesCmd::Done]) {
        if clicked {
            st.cmd(Command::Notes(cmd));
        }
    }
    note(ui, st, Rect::new(r.x, r.y + 22.0, r.w, 15.0));
}

fn draw_failed(ui: &mut Ui, st: &mut AppState, r: Rect, p: Problem) {
    let r = Rect::new(r.x, r.y + 14.0, r.w.min(TEXT_W), r.h);
    let y = headline(ui, r, p.title());
    let y = para(ui, Rect::new(r.x, y + 4.0, r.w, 60.0), &p.line(), palette::TEXT_DIM) + 14.0;
    let accent = st.accent;
    let mut list: Vec<(&str, &str, Option<Icon>, PillStyle)> = Vec::new();
    let mut cmds = Vec::new();
    if p == Problem::Capture(CaptureIssue::MicDenied) {
        list.push(("notes.mic-settings", "Open microphone settings", Some(Icon::Open), PillStyle::Primary(accent)));
        cmds.push(Command::OpenUrl(MIC_SETTINGS.into()));
    }
    if matches!(p, Problem::NoKey | Problem::KeyRefused) {
        list.push(("notes.paste-key", "Paste key", None, PillStyle::Neutral));
        cmds.push(Command::Notes(NotesCmd::PasteKey));
    } else if p.in_settings() {
        list.push(("notes.settings", "Settings", Some(Icon::Gear), PillStyle::Neutral));
        cmds.push(Command::OpenSettings);
    }
    let kept = st.ui.notes_kept;
    if kept && p.retryable() {
        let label = if p == Problem::Unfinished { "Write it up" } else { "Try again" };
        list.push(("notes.retry", label, None, PillStyle::Primary(accent)));
        cmds.push(Command::Notes(NotesCmd::Retry));
    }
    if kept {
        list.push(("notes.discard", "Throw away", Some(Icon::Trash), PillStyle::Utility));
        cmds.push(Command::Notes(NotesCmd::Discard));
    } else {
        list.push(("notes.done", "OK", None, PillStyle::Utility));
        cmds.push(Command::Notes(NotesCmd::Done));
    }
    for (clicked, cmd) in buttons(ui, r.x, y, &list).into_iter().zip(cmds) {
        if clicked {
            st.cmd(cmd);
        }
    }
    note(ui, st, Rect::new(r.x, y + PILL_SMALL + 6.0, r.w, 15.0));
}
