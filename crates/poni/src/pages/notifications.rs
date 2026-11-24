//! Notifications page: the notifications the island showed lately (the
//! newest few, in memory only), behind the bell in the header while there
//! are some. A row opens the app it came from; Clear empties the list.
//! What a notification shows here follows Settings > Notifications (who
//! it's from, its text), as its card did. Everything else about
//! Notifications is in the Settings window.

use crate::commands::Command;
use crate::gfx::{TextStyle, Weight};
use crate::icons::Icon;
use crate::state::{AppState, InlineAction, Notice, NoticeAction, NotifyAccess};
use crate::ui::{wid, wid_n, PillStyle, Ui, PILL_SMALL};
use poni_core::geom::{palette, Color, Rect};
use poni_core::notifications::{self, NotificationSettings, Seen};

/// The newest notifications kept (in memory) and shown.
pub const KEEP: usize = 5;
const HEAD_H: f32 = 34.0;
const ROW_H: f32 = 44.0;
const LOGO: f32 = 26.0;

/// The island's notice for one notification (None: it has no words): who
/// it's from and the message, or Show for what the person keeps hidden, as
/// in a Money in notice; Open, or a click on it, opens the app.
pub fn message_notice(seen: &Seen, choices: &NotificationSettings, accent: Color, now: f64) -> Option<Notice> {
    let said = notifications::compose(seen, choices.who, choices.text)?;
    Some(Notice {
        icon: Icon::Message,
        color: notifications::color(&seen.app_id, &seen.app).unwrap_or(accent),
        title: said.title,
        title_dim: said.title_dim,
        subtitle: said.subtitle,
        until: now + notifications::NOTICE_S,
        action: Some(NoticeAction::OpenApp(seen.app_id.clone())),
        inline: Some(if said.reveal.is_some() { InlineAction::ShowAmount } else { InlineAction::Open }),
        reveal: said.reveal,
    })
}

/// A button beside the line: its label, what it asks for, and whether it
/// is the one to press (the accent's colour).
pub type Button = (&'static str, crate::commands::NotifyCmd, bool);

const SETTINGS: Button = ("Windows settings", crate::commands::NotifyCmd::OpenWindowsSettings, false);

/// What Windows allows, in words, and the buttons for it (Settings > Notifications).
pub fn access_look(access: NotifyAccess) -> (&'static str, Vec<Button>) {
    use crate::commands::NotifyCmd;
    match access {
        NotifyAccess::Unknown => ("Checking with Windows\u{2026}", vec![]),
        NotifyAccess::NoPackage => ("Windows lets only the Microsoft Store copy read notifications.", vec![]),
        NotifyAccess::Ask => ("Windows asks you once before the island can read notifications.", vec![("Allow", NotifyCmd::Ask, true)]),
        NotifyAccess::Asking => ("Answer Windows' question to go on.", vec![]),
        NotifyAccess::Unanswered => ("Windows got no answer. Ask again, or turn it on in Windows' settings.", vec![("Ask again", NotifyCmd::Ask, true), SETTINGS]),
        NotifyAccess::Denied => ("Turned off for Hihu Island in Windows' settings.", vec![SETTINGS]),
        NotifyAccess::Allowed => ("On. New ones show on the island.", vec![]),
        NotifyAccess::Failed => ("Windows didn't let the island listen. Switch it off and on in Settings.", vec![]),
    }
}

pub fn height(st: &AppState) -> f32 {
    HEAD_H + st.recent.len().clamp(1, KEEP) as f32 * ROW_H
}

/// "now", "5 min", "2 h", "3 d".
fn ago(secs: i64) -> String {
    match secs.max(0) {
        s if s < 60 => "now".into(),
        s if s < 3600 => format!("{} min", s / 60),
        s if s < 86_400 => format!("{} h", s / 3600),
        s => format!("{} d", s / 86_400),
    }
}

pub fn draw(ui: &mut Ui, st: &mut AppState, area: Rect) {
    let head = Rect::new(area.x, area.y, area.w, HEAD_H - 6.0);
    ui.c.text("Recent notifications", head, &TextStyle::headline());
    if !st.recent.is_empty() {
        let w = ui.pill_width("Clear", false, true);
        let b = Rect::new(head.right() - w, head.center().y - PILL_SMALL / 2.0, w, PILL_SMALL);
        if ui.pill(wid("recent.clear"), b, "Clear", None, PillStyle::Utility).clicked {
            st.recent.clear();
            st.recent_unseen = 0;
        }
    }
    if st.recent.is_empty() {
        let r = Rect::new(area.x, area.y + HEAD_H, area.w, ROW_H);
        ui.c.text("Nothing lately. New ones show here after they show on the island.", r, &TextStyle::footnote().color(palette::TEXT_DIM));
        return;
    }
    let now = st.unix;
    let mut open = None;
    for (i, n) in st.recent.iter().take(KEEP).enumerate() {
        let row = Rect::new(area.x, area.y + HEAD_H + i as f32 * ROW_H, area.w, ROW_H);
        let resp = ui.button(wid_n("recent.row", i as u64), row);
        if resp.hovered || resp.pressed {
            ui.c.fill_rrect(row.inset(-6.0, 2.0), 10.0, Color::white(if resp.pressed { 0.10 } else { 0.06 }));
        }
        let logo = Rect::new(row.x, row.center().y - LOGO / 2.0, LOGO, LOGO);
        match st.notify_logos.get(&n.app_id) {
            Some(img) => ui.c.image(img, logo, 7.0),
            None => {
                ui.c.fill_rrect(logo, 7.0, st.accent.with_alpha(0.22));
                ui.c.icon(Icon::Message, Rect::from_center(logo.center(), 16.0, 16.0), st.accent);
            }
        }
        let tx = logo.right() + 12.0;
        let when = ago(now - n.at);
        let ws = TextStyle::caption().color(palette::TEXT_FAINT).right();
        let ww = ui.c.measure(&when, &ws, 100.0).0 + 2.0;
        let tw = (row.right() - tx - ww - 10.0).max(0.0);
        let title = if n.title.is_empty() { n.app.as_str() } else { n.title.as_str() };
        ui.c.text(title, Rect::new(tx, row.y + 5.0, tw, 18.0), &TextStyle::body().weight(Weight::Semibold));
        ui.c.text(&when, Rect::new(row.right() - ww, row.y + 6.0, ww, 16.0), &ws);
        let line = if n.text.is_empty() { n.app.as_str() } else { n.text.as_str() };
        ui.c.text(line, Rect::new(tx, row.y + 23.0, row.right() - tx, 16.0), &TextStyle::footnote().color(palette::TEXT_DIM));
        if resp.clicked {
            open = Some(n.app_id.clone());
        }
    }
    if let Some(id) = open {
        st.cmd(Command::OpenApp(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_short() {
        assert_eq!(ago(5), "now");
        assert_eq!(ago(300), "5 min");
        assert_eq!(ago(7300), "2 h");
        assert_eq!(ago(3 * 86_400 + 5), "3 d");
    }

    #[test]
    fn allow_shows_only_while_windows_can_be_asked() {
        use crate::commands::NotifyCmd;
        let asks = |a| access_look(a).1.into_iter().map(|b| b.1).collect::<Vec<_>>();
        assert_eq!(asks(NotifyAccess::Ask), [NotifyCmd::Ask]);
        assert_eq!(asks(NotifyAccess::Unanswered), [NotifyCmd::Ask, NotifyCmd::OpenWindowsSettings]);
        assert_eq!(asks(NotifyAccess::Denied), [NotifyCmd::OpenWindowsSettings]);
    }
}
