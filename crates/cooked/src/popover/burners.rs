//! What burns tokens: the top sources of the last 5 minutes, project and rate over thread and account. A click opens
//! the source where it runs (`Request::OpenSession`). The first three show; "N more" opens the rest. In urgency the
//! rate of the account that fills is red.
//!
//! The line under the pointer lifts out of the card, as in the Swift app: a raised chrome card 1 pt up (still with
//! Reduce Motion), the hairlines beside it gone. It says where a click opens it ("Open in T3 Code ↗") in place of the
//! account, and its agent count by the rate; an ended line, its rate and "↗" in place of "9 min ago".

use cooked_model::token_format;
use cooked_model::token_sources::TokenSource;
use cooked_model::usage_snapshot::UsageSnapshot;
use rune::{Anim, Color, Cursor, El, col, el, row, text};

use super::{CARD_RADIUS, Popover, Request};
use crate::theme::{Motion, Palette, TextStyle, card};

/// Lines shown until "N more" opens the rest.
pub const COLLAPSED: usize = 3;
/// The card's side padding and a line's: the text sits 14 pt in, and a lifted line's card 6 pt.
const LIST_PADDING: f32 = 6.0;
const LINE_PADDING: f32 = 8.0;

/// The source under the pointer, and where a click opens it (`mac::open::label`); `None`: nowhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lift {
    pub id: String,
    pub open: Option<&'static str>,
}

/// What a line says on its right.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Right {
    /// Lifted and live: the agent count by the rate.
    pub agents: Option<String>,
    /// Live: the account, or lifted, where a click opens it. Ended: "9 min ago", or lifted, "↗" after the rate.
    pub note: String,
    /// The note says where a click opens: full ink.
    pub opens: bool,
    /// Ended and lifted: the rate shows before the note.
    pub rate: bool,
}

/// `source`'s right side, lifted when `hovered` is this source.
pub fn right(source: &TokenSource, hovered: Option<&Lift>) -> Right {
    let lifted = hovered.filter(|l| l.id == source.id);
    match (source.ended_ago, lifted) {
        // at rest, or lifted with nowhere to open it
        (Some(ago), None | Some(Lift { open: None, .. })) => Right {
            agents: None,
            note: format!("{ago} min ago"),
            opens: false,
            rate: false,
        },
        (Some(_), Some(_)) => Right {
            agents: None,
            note: "↗".to_owned(),
            opens: true,
            rate: true,
        },
        (None, lifted) => {
            let open = lifted.and_then(|l| l.open);
            Right {
                agents: lifted.and(source.agents.clone()),
                note: open.map_or_else(|| source.account.clone(), str::to_owned),
                opens: open.is_some(),
                rate: false,
            }
        }
    }
}

/// How far a lifted line rises, points: none with Reduce Motion.
pub fn lift_offset(reduce_motion: bool) -> f32 {
    if reduce_motion { 0.0 } else { -1.0 }
}

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

pub fn card_for(
    snapshot: &UsageSnapshot,
    expanded: bool,
    hovered: Option<&Lift>,
    reduce_motion: bool,
) -> El {
    let sources = shown(&snapshot.sources, expanded);
    let hot = hot(snapshot);
    let lifted = |i: usize| {
        sources
            .get(i)
            .is_some_and(|s| hovered.is_some_and(|l| l.id == s.id))
    };
    let mut list = col()
        .key("burners")
        .w_full()
        .pt(10.0)
        .px(LIST_PADDING)
        .pb(8.0);
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
                    .px(LINE_PADDING - 2.0),
            );
        } else if i > 0 && source.ended_ago.is_none() {
            list = list.child(hairline(i, lifted(i) || lifted(i - 1)));
        }
        list = list.child(line(
            source,
            hot == Some(source.id.as_str()),
            hovered,
            reduce_motion,
        ));
    }
    let hidden = snapshot.sources.len().saturating_sub(COLLAPSED);
    if hidden > 0 {
        let title = if expanded {
            "Show less".to_owned()
        } else {
            format!("{hidden} more")
        };
        let last = sources.len();
        list = list
            .child(hairline(last, last > 0 && lifted(last - 1)))
            .child(more(title));
    }
    card(list, CARD_RADIUS)
}

/// A 0.5 pt line between two sources, under the text's width; gone beside a lifted line.
fn hairline(i: usize, hidden: bool) -> El {
    let alpha = if hidden { 0.0 } else { Palette::LINE_OPACITY };
    el().key(format!("line-{i}"))
        .w_full()
        .px(LINE_PADDING - 2.0)
        .child(el().w_full().h(0.5).fill(Color::WHITE.alpha(alpha)))
}

/// One source: project and tokens/min, then the thread and its account; an ended one, one quiet line. Lifted: a
/// raised card.
fn line(source: &TokenSource, hot: bool, hovered: Option<&Lift>, reduce_motion: bool) -> El {
    let target = source.target();
    let side = right(source, hovered);
    let lifted = hovered.is_some_and(|l| l.id == source.id);
    let (id, hover_target) = (source.id.clone(), target.clone());
    // The pointer is tracked on a box that stays put: a hit area that rose with the line would leave the pointer on
    // its bottom point, drop the line and catch the pointer again.
    let hit_box = row()
        .key(format!("source-{}", source.id))
        .w_full()
        .hit()
        .label(source.project.clone())
        .cursor(Cursor::Pointer)
        .on_hover_change(move |s: &mut Popover, inside, _| {
            s.hover(&id, hover_target.as_ref(), inside)
        })
        .on_press(move |s: &mut Popover| s.request(Request::OpenSession(target.clone())));
    let mut line = row()
        .key("line")
        .w_full()
        .px(LINE_PADDING)
        .py(7.0)
        .gap(8.0)
        .radius(10.0)
        .offset(
            0.0,
            if lifted {
                lift_offset(reduce_motion)
            } else {
                0.0
            },
        );
    if let Some(curve) = Motion::animation(Motion::CONTROL, reduce_motion) {
        line = line.offset_motion(curve);
    }
    if lifted {
        line = card(line, 10.0);
    }
    let line = match source.ended_ago {
        Some(_) => {
            // at rest "9 min ago" at 55%; lifted, the rate then "  ↗" at full ink, as Swift's "\(rate)  ↗"
            let mut end = row().key("end").center_y().frame_motion(Anim::INSTANT);
            if side.rate {
                end = end.child(rate(source.rate, false));
            }
            let (note, ink) = if side.rate {
                (format!("  {}", side.note), Palette::ink())
            } else {
                (side.note, Palette::ink3())
            };
            line.center_y()
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
                    end.child(
                        text(note)
                            .key("ago")
                            .frame_motion(Anim::INSTANT)
                            .font(TextStyle::Body.font())
                            .color(ink),
                    ),
                )
        }
        None => {
            let mut top = row()
                .key("rate-row")
                .gap(8.0)
                .items_end()
                .frame_motion(Anim::INSTANT);
            if let Some(agents) = side.agents {
                top = top.child(
                    text(agents)
                        .key("agents")
                        .frame_motion(Anim::INSTANT)
                        .font(TextStyle::Body.font())
                        .color(Palette::ink3()),
                );
            }
            line.child(
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
                // Lifted, the column gains the agents and "Open in …": it lands in place and they fade in, as
                // SwiftUI's text swap; travelling in from the right, they would cross the card's edge.
                col()
                    .key("right")
                    .items_end()
                    .frame_motion(Anim::INSTANT)
                    .child(top.child(rate(source.rate, hot)))
                    .child(
                        text(side.note)
                            .key("account")
                            .frame_motion(Anim::INSTANT)
                            .font(TextStyle::Body.font())
                            .color(if side.opens {
                                Palette::ink()
                            } else {
                                Palette::ink2()
                            }),
                    ),
            )
        }
    };
    hit_box.child(line)
}

/// "113k/min": the number at full ink, red for the source that fills; its unit at 55%.
fn rate(rate: f64, hot: bool) -> El {
    row()
        .key("rate-pair")
        .frame_motion(Anim::INSTANT)
        .child(
            text(token_format::short(rate))
                .key("rate")
                .font(TextStyle::Number.font())
                .color(if hot { Palette::red() } else { Palette::ink() }),
        )
        .child(
            text("/min")
                .key("rate-unit")
                .font(TextStyle::Number.font())
                .color(Palette::ink3()),
        )
}

/// "2 more" under the first lines, "Show less" once open: quieter than a source, brighter on hover.
fn more(title: String) -> El {
    el().key("more")
        .w_full()
        .px(LINE_PADDING)
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
