//! What burns tokens (mockup B `.burn`, Swift `SourcesCard`): the top sources of the last 5 minutes, project and
//! rate over thread and account. A click opens the source's thread (`Request::OpenSession`, Task 18). The first
//! three show; "N more" opens the rest. In urgency the rate of the account that fills is red.

use cooked_model::token_format;
use cooked_model::token_sources::TokenSource;
use cooked_model::usage_snapshot::UsageSnapshot;
use rune::{Color, Cursor, El, col, el, row, text};

use super::{CARD_RADIUS, Popover, Request};
use crate::theme::{Palette, TextStyle, card};

/// Lines shown until "N more" opens the rest.
pub const COLLAPSED: usize = 3;

/// The sources to show: the first `COLLAPSED`, or all once opened.
pub fn shown(sources: &[TokenSource], expanded: bool) -> &[TokenSource] {
    if expanded {
        sources
    } else {
        &sources[..sources.len().min(COLLAPSED)]
    }
}

/// The source whose rate is red: in urgency, the top source of the account that fills.
pub fn hot(snapshot: &UsageSnapshot) -> Option<&str> {
    snapshot
        .is_urgent()
        .then(|| snapshot.peak().map(|s| s.id.as_str()))
        .flatten()
}

pub fn card_for(snapshot: &UsageSnapshot, expanded: bool) -> El {
    let sources = shown(&snapshot.sources, expanded);
    let hot = hot(snapshot);
    let mut list = col().key("burners").w_full().pt(10.0).px(12.0).pb(8.0);
    for (i, source) in sources.iter().enumerate() {
        let first_ended =
            source.ended_ago.is_some() && (i == 0 || sources[i - 1].ended_ago.is_none());
        if first_ended {
            list = list.child(
                text("Ended")
                    .key(format!("ended-{i}"))
                    .font(TextStyle::Label.font())
                    .color(Palette::ink3())
                    .pt(6.0)
                    .px(4.0),
            );
        } else if i > 0 && source.ended_ago.is_none() {
            list = list.child(hairline(i));
        }
        list = list.child(line(source, hot == Some(source.id.as_str())));
    }
    let hidden = snapshot.sources.len().saturating_sub(COLLAPSED);
    if hidden > 0 {
        let title = if expanded {
            "Show less".to_owned()
        } else {
            format!("{hidden} more")
        };
        list = list.child(hairline(sources.len())).child(more(title));
    }
    card(list, CARD_RADIUS)
}

/// A 0.5 pt line between two sources (mockup B `.b-row + .b-row`).
fn hairline(i: usize) -> El {
    el().key(format!("line-{i}"))
        .w_full()
        .h(0.5)
        .fill(Color::WHITE.alpha(Palette::LINE_OPACITY))
}

/// One source: project and tokens/min, then the thread and its account; an ended one, one quiet line.
fn line(source: &TokenSource, hot: bool) -> El {
    let target = source.target();
    let mut line = row()
        .key(format!("source-{}", source.id))
        .w_full()
        .px(2.0)
        .py(7.0)
        .gap(8.0)
        .radius(10.0)
        .hit()
        .hover(Color::WHITE.alpha(0.04))
        .label(source.project.clone());
    if let Some(target) = target {
        line = line
            .cursor(Cursor::Pointer)
            .on_press(move |s: &mut Popover| s.request(Request::OpenSession(target.clone())));
    }
    match source.ended_ago {
        Some(ago) => line
            .center_y()
            .child(
                text(format!("{}  {}", source.project, source.detail))
                    .key("ended-text")
                    .font(TextStyle::Body.font())
                    .color(Palette::ink2())
                    .truncate()
                    .shrink()
                    .grow(1.0),
            )
            .child(
                text(format!("{ago} min ago"))
                    .key("ago")
                    .font(TextStyle::Body.font())
                    .color(Palette::ink3()),
            ),
        None => line
            .child(
                col()
                    .key("left")
                    .grow(1.0)
                    .shrink()
                    .min_w(0.0)
                    .child(
                        text(source.project.clone())
                            .key("project")
                            .font(TextStyle::Emphasis.font())
                            .color(Palette::ink())
                            .truncate(),
                    )
                    .child(
                        text(source.detail.clone())
                            .key("detail")
                            .font(TextStyle::Body.font())
                            .color(Palette::ink2())
                            .truncate(),
                    ),
            )
            .child(
                col()
                    .key("right")
                    .items_end()
                    .child(
                        row()
                            .key("rate-row")
                            .child(
                                text(token_format::short(source.rate))
                                    .key("rate")
                                    .font(TextStyle::Number.font())
                                    .color(if hot { Palette::red() } else { Palette::ink() }),
                            )
                            .child(
                                text("/min")
                                    .key("rate-unit")
                                    .font(TextStyle::Number.font())
                                    .color(Palette::ink3()),
                            ),
                    )
                    .child(
                        text(source.account.clone())
                            .key("account")
                            .font(TextStyle::Body.font())
                            .color(Palette::ink2()),
                    ),
            ),
    }
}

/// "2 more" under the first lines, "Show less" once open: quieter than a source, brighter on hover.
fn more(title: String) -> El {
    el().key("more")
        .w_full()
        .px(2.0)
        .py(6.0)
        .radius(10.0)
        .hit()
        .cursor(Cursor::Pointer)
        .hover(Color::WHITE.alpha(0.04))
        .on_press(|s: &mut Popover| s.expanded = !s.expanded)
        .child(
            text(title)
                .key("more-text")
                .font(TextStyle::Label.font())
                .color(Palette::ink3()),
        )
}
