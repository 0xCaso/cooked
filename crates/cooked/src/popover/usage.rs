//! Usage per account: a pill per window, counted as what is left. The 5-hour pill opens with the account name, then the
//! % and "full HH:MM" (red: it runs out before the reset) or "resets HH:MM"; the week pill its % and
//! reset day. Under each, a 44 x 2 tile bar. Accounts not used in 5 hours are left out.

use cooked_model::tiles::bar_grid;
use cooked_model::usage_snapshot::{Account, UsageSnapshot};
use rune::{El, col, el, row, text};

use super::{CARD_RADIUS, WELL_RADIUS};
use crate::theme::{Palette, TextStyle, card, well};
use crate::tiles::{TileGrid, tile_slot};

/// A bar's tiles.
pub const BAR: (usize, usize) = (44, 2);
/// The week column, points.
const WEEK_WIDTH: f32 = 150.0;
const COLUMN_GAP: f32 = 8.0;
/// Bar height and tile spacing.
const BAR_HEIGHT: f32 = 8.0;
const BAR_GAP: f32 = 1.2;
const BAR_RADIUS: f32 = 0.85;
/// The even-pace line, as Swift's `PaceStrip`: 1.6 pt wide, 2.5 pt past the bar above and below, ink at 72%.
const LINE_WIDTH: f32 = 1.6;
const LINE_OUT: f32 = 2.5;
const LINE_ALPHA: f32 = 0.72;

/// The 5-hour window runs out before it resets: its % and times are red.
pub fn fills(a: &Account) -> bool {
    a.is_blocked || a.full_at.is_some()
}

/// The share of the window still to go, where the pace line sits; none without a percent.
fn time_left(left: Option<i64>, elapsed: Option<f64>) -> Option<f32> {
    left.and(elapsed).map(|e| (1.0 - e) as f32)
}

/// The bar's tiles and, over them, the pace line in its column (`bar_grid` leaves that column off): a row of one
/// slot per column at the tiles' spacing, so the line sits on the column's centre.
fn pace_strip(mark: u32, time_left: Option<f32>) -> El {
    let bar = tile_slot(mark).key("bar").w_full().h_full();
    let mut strip = el().key("pace").w_full().h(BAR_HEIGHT).child(bar);
    if let Some(t) = time_left {
        let cols = BAR.0;
        let line = ((t.clamp(0.0, 1.0) * (cols - 1) as f32).round() as usize).min(cols - 1);
        let mut track = row()
            .key("pace-track")
            .absolute(0.0, -LINE_OUT)
            .w_full()
            .h(BAR_HEIGHT + 2.0 * LINE_OUT)
            .gap(BAR_GAP);
        for c in 0..cols {
            let mut slot = row().grow(1.0).h_full().justify_center();
            if c == line {
                slot = slot.child(
                    el().key("pace-line")
                        .w(LINE_WIDTH)
                        .h_full()
                        .radius(LINE_WIDTH / 2.0)
                        .fill(Palette::ink().alpha(LINE_ALPHA)),
                );
            }
            track = track.child(slot);
        }
        strip = strip.child(track);
    }
    strip
}

/// The bars of every account row, 5 hours then week, in row order.
pub fn bars(s: &UsageSnapshot) -> Vec<TileGrid> {
    let grid = |left: Option<i64>, elapsed: Option<f64>, hot: bool| {
        let fraction = left.unwrap_or(0) as f32 / 100.0;
        let (cols, rows) = BAR;
        let line = time_left(left, elapsed);
        TileGrid::new(cols, rows, bar_grid(fraction, line, hot, cols, rows))
            .spacing(BAR_GAP, BAR_RADIUS)
    };
    s.rows()
        .into_iter()
        .flat_map(|a| {
            [
                grid(a.five_hour_left_if_known(), a.five_hour_elapsed, fills(a)),
                grid(
                    a.week_percent.map(Account::left),
                    a.week_elapsed,
                    a.week_full_at.is_some(),
                ),
            ]
        })
        .collect()
}

/// The 5-hour pill's times: "full 23:14" (red), "resets 04:40", or nothing without a percent. Blocked: the reset
/// in red.
pub fn five_hour_times(a: &Account) -> Option<(String, bool)> {
    if a.is_blocked {
        return Some((format!("resets {}", a.five_hour_reset), true));
    }
    if let Some(full) = &a.full_at {
        return Some((format!("full {full}"), true));
    }
    a.has_percent
        .then(|| (format!("resets {}", a.five_hour_reset), false))
}

/// The week pill's times: "full HH:MM" (red) or "resets Mon".
pub fn week_times(a: &Account) -> Option<(String, bool)> {
    if let Some(full) = &a.week_full_at {
        return Some((format!("full {full}"), true));
    }
    a.week_reset
        .as_ref()
        .map(|r| (format!("resets {r}"), false))
}

pub fn card_for(s: &UsageSnapshot, first_mark: u32) -> El {
    let label = |key: &'static str, t: &'static str| {
        text(t)
            .key(key)
            .font(TextStyle::Label.font())
            .color(Palette::ink3())
    };
    let mut body = col().key("usage").w_full().pt(14.0).px(14.0).pb(6.0);
    let rows = s.rows();
    if !rows.is_empty() {
        body = body.child(
            row()
                .key("usage-columns")
                .w_full()
                .gap(COLUMN_GAP)
                .px(4.0)
                .pb(6.0)
                .child(label("five-hours", "5 hours").grow(1.0))
                .child(label("week", "week").w(WEEK_WIDTH)),
        );
    }
    for (i, a) in rows.into_iter().enumerate() {
        let mark = first_mark + 2 * i as u32;
        body = body.child(
            row()
                .key(format!("account-{}", a.id))
                .w_full()
                .gap(COLUMN_GAP)
                .mb(6.0)
                .child(five_hour_pill(a, mark).grow(1.0).shrink().min_w(0.0))
                .child(week_pill(a, mark + 1).w(WEEK_WIDTH)),
        );
    }
    card(body, CARD_RADIUS)
}

/// A pill: its top line, then the bar the pill's full width.
fn pill(key: &'static str, top: El, mark: u32, time_left: Option<f32>) -> El {
    well(
        col()
            .key(key)
            .pt(6.0)
            .px(10.0)
            .pb(7.0)
            .child(top.mb(5.0))
            .child(pace_strip(mark, time_left)),
        WELL_RADIUS,
    )
}

fn times(key: &'static str, t: Option<(String, bool)>) -> Option<El> {
    t.map(|(t, red)| {
        text(t)
            .key(key)
            .font(TextStyle::LabelNumber.font())
            .color(if red { Palette::red() } else { Palette::ink3() })
    })
}

fn percent(key: &'static str, left: Option<i64>, red: bool) -> El {
    text(left.map_or("—".to_owned(), |l| format!("{l}%")))
        .key(key)
        .font(TextStyle::Number.font())
        .color(if red { Palette::red() } else { Palette::ink2() })
}

fn five_hour_pill(a: &Account, mark: u32) -> El {
    // the account consuming now at 500 and full ink, the others at 72% (DESIGN.md rule 3)
    let name = text(a.short.clone())
        .key("name")
        .font(if a.is_live {
            TextStyle::Emphasis.font()
        } else {
            TextStyle::Body.font()
        })
        .color(if a.is_live {
            Palette::ink()
        } else {
            Palette::ink2()
        })
        .truncate()
        .shrink();
    let mut top = row()
        .key("top")
        .w_full()
        .gap(8.0)
        .items_end()
        .child(name)
        .child(percent("percent", a.five_hour_left_if_known(), fills(a)))
        .child(el().grow(1.0));
    if let Some(t) = times("times", five_hour_times(a)) {
        top = top.child(t);
    }
    let line = time_left(a.five_hour_left_if_known(), a.five_hour_elapsed);
    pill("five-hour", top, mark, line)
}

fn week_pill(a: &Account, mark: u32) -> El {
    let mut top = row()
        .key("top")
        .w_full()
        .gap(8.0)
        .items_end()
        .child(percent(
            "percent",
            a.week_percent.map(Account::left),
            a.week_full_at.is_some(),
        ))
        .child(el().grow(1.0));
    if let Some(t) = times("times", week_times(a)) {
        top = top.child(t);
    }
    let line = time_left(a.week_percent.map(Account::left), a.week_elapsed);
    pill("week", top, mark, line)
}
