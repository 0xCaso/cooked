//! The notice card on top of the popover (Swift `NoticeCard`, DESIGN.md): what is missing in one sentence, and its
//! one fix. Title in emphasis, body at 72%, both wrap; one white button at most, "Not now" beside it.

use cooked_model::notice::Notice;
use rune::{Color, Cursor, El, col, row, text};

use super::{CARD_RADIUS, Popover};
use crate::theme::{Motion, Palette, Rgb, TextStyle, card};

/// The buttons of a notice: (label, primary), in order. The first acts (`IslandState::act`), "Not now" puts it
/// off.
pub fn buttons(notice: &Notice, copied: bool) -> Vec<(&'static str, bool)> {
    match notice {
        Notice::NoAccounts => vec![("Set up…", true)],
        Notice::FormatChanged { .. } => {
            vec![(if copied { "Copied" } else { "Copy details" }, false)]
        }
        Notice::Paused => vec![("Connect again", true)],
        Notice::NoPercent { .. } => vec![("Connect", true), ("Not now", false)],
        Notice::ApiKey { .. } => vec![("OK", false)],
    }
}

pub fn card_for(notice: &Notice, copied: bool) -> El {
    let mut actions = row().key("actions").gap(8.0).mt(10.0);
    for (i, (title, primary)) in buttons(notice, copied).into_iter().enumerate() {
        let n = notice.clone();
        let button = button(title, primary);
        actions = actions.child(if i == 0 {
            button.on_press(move |s: &mut Popover| s.act(&n))
        } else {
            button.on_press(move |s: &mut Popover| s.put_off(&n))
        });
    }
    card(
        col()
            .key("notice")
            .w_full()
            .pt(12.0)
            .px(14.0)
            .pb(12.0)
            .child(
                text(notice.title())
                    .key("notice-title")
                    .font(TextStyle::Emphasis.font())
                    .color(Palette::ink())
                    .w_full()
                    .wrap(),
            )
            .child(
                text(notice.body())
                    .key("notice-body")
                    .font(TextStyle::Body.font())
                    .color(Palette::ink2())
                    .w_full()
                    .wrap()
                    .mt(2.0),
            )
            .child(actions),
        CARD_RADIUS,
    )
}

/// 28 pt capsules (Swift `PrimaryButtonStyle`, `SecondaryButtonStyle`): white to ink with dark text, or a quiet
/// ring with ink text.
fn button(title: &'static str, primary: bool) -> El {
    let b = row()
        .key(title)
        .h(28.0)
        .px(12.0)
        .center_y()
        .radius(14.0)
        .hit()
        .cursor(Cursor::Pointer)
        .press_scale(Motion::BUTTON_SCALE)
        .label(title);
    if primary {
        b.fill(Palette::ink())
            .gradient(Color::WHITE, Palette::ink())
            .border(0.5, Color::WHITE.alpha(0.4))
            .child(
                text(title)
                    .key("label")
                    .font(TextStyle::Emphasis.font())
                    .color(Rgb::hex(0x111111).color()),
            )
    } else {
        b.fill(Color::WHITE.alpha(0.06))
            .hover(Color::WHITE.alpha(0.06))
            .border(0.5, Color::WHITE.alpha(0.16))
            .child(
                text(title)
                    .key("label")
                    .font(TextStyle::Emphasis.font())
                    .color(Palette::ink()),
            )
    }
}
