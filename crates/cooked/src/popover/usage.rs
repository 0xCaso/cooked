//! Usage per account (mockup B `.usage`, Swift `accountsCard`): a pill per window, counted as what is left. The
//! 5-hour pill opens with the account name, then the % and "full HH:MM" (red: it runs out before the reset) or
//! "resets HH:MM"; the week pill its % and reset day. Under each, a 44 x 2 tile bar. Accounts not used in 5 hours
//! fold into one line (DESIGN.md).

use cooked_model::tiles::bar_grid;
use cooked_model::usage_snapshot::{Account, UsageSnapshot};
use rune::{El, col, el, row, text};

use super::{CARD_RADIUS, WELL_RADIUS};
use crate::theme::{Palette, TextStyle, card, well};
use crate::tiles::{TileGrid, tile_slot};

/// A bar's tiles.
pub const BAR: (usize, usize) = (44, 2);
/// The week column, points (mockup B `.u-row`: `minmax(0, 1fr) 150px`).
const WEEK_WIDTH: f32 = 150.0;
const COLUMN_GAP: f32 = 8.0;
/// Bar height and tile spacing (mockup B `.pop.tiles .bar`, `gap: 1.2`).
const BAR_HEIGHT: f32 = 8.0;
const BAR_GAP: f32 = 1.2;
const BAR_RADIUS: f32 = 0.85;

/// The 5-hour window runs out before it resets: its % and times are red.
pub fn fills(a: &Account) -> bool {
    a.is_blocked || a.full_at.is_some()
}

/// The bars of every account row, 5 hours then week, in row order.
pub fn bars(s: &UsageSnapshot) -> Vec<TileGrid> {
    let grid = |left: Option<i64>, elapsed: Option<f64>, hot: bool| {
        let fraction = left.unwrap_or(0) as f32 / 100.0;
        let time_left = left.and(elapsed).map(|e| (1.0 - e) as f32);
        let (cols, rows) = BAR;
        TileGrid::new(cols, rows, bar_grid(fraction, time_left, hot, cols, rows))
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
/// in red (Swift `PillTimes(red:)`).
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

/// "personal, side": the accounts folded into one line.
pub fn unused_line(s: &UsageSnapshot) -> Option<String> {
    let names: Vec<&str> = s.unused().iter().map(|a| a.short.as_str()).collect();
    (!names.is_empty()).then(|| names.join(", "))
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
    if let Some(names) = unused_line(s) {
        body = body.child(
            row()
                .key("unused")
                .w_full()
                .items_end()
                .gap(8.0)
                .px(4.0)
                .pb(6.0)
                .child(label("unused-label", "not used in 5 h"))
                .child(el().grow(1.0))
                .child(
                    text(names)
                        .key("unused-names")
                        .font(TextStyle::Body.font())
                        .color(Palette::ink2())
                        .truncate()
                        .shrink(),
                ),
        );
    }
    card(body, CARD_RADIUS)
}

/// A pill: its top line, then the bar the pill's full width (mockup B `.u5`, `.uw`).
fn pill(key: &'static str, top: El, mark: u32) -> El {
    well(
        col()
            .key(key)
            .pt(6.0)
            .px(10.0)
            .pb(7.0)
            .child(top.mb(5.0))
            .child(tile_slot(mark).key("bar").w_full().h(BAR_HEIGHT)),
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
    pill("five-hour", top, mark)
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
    pill("week", top, mark)
}
