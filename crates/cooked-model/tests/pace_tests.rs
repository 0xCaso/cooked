//! `PaceTests.swift`: even pace and the pills' strips.
mod common;

use common::token_source;
use cooked_model::clock::{FixedOffset, days_from_civil};
use cooked_model::demo_data;
use cooked_model::live_snapshot;

#[test]
fn elapsed_is_the_share_of_the_window_gone() {
    let now = 1_000_000.0;
    assert_eq!(live_snapshot::elapsed(now + 3600.0, 5.0 * 3600.0, now), 0.8);
    assert_eq!(live_snapshot::elapsed(now - 60.0, 5.0 * 3600.0, now), 1.0);
    assert_eq!(live_snapshot::elapsed(now + 9.0 * 3600.0, 5.0 * 3600.0, now), 0.0);
}

#[test]
fn weekday_is_short_english() {
    // 02/11/2026 09:00 in Rome (UTC+1)
    let monday = (days_from_civil(2026, 11, 2) * 86_400 + 8 * 3600) as f64;
    assert_eq!(live_snapshot::weekday(monday, &FixedOffset(3600)), "Mon");
}

#[test]
#[ignore = "needs Task 13: the pills' pace strip (Views/AccountPills.swift PaceStrip, PopoverView.left)"]
fn strip_lights_the_share_left_and_puts_the_line_at_time_left() {
    // Swift: team on 30/10 at 13:08: 26% left, 27% of the window to go, 15 columns
    // let l = PaceStrip.layout(left: 26, timeLeft: 0.27, cols: 15); #expect(abs(l.fill - 3.9) < 0.001); #expect(l.line == 4)
    // #expect(PaceStrip.layout(left: 140, timeLeft: nil, cols: 15).fill == 15); ….line == nil
    // #expect(PaceStrip.columns(width: 46) == 15); #expect(PopoverView.left(74) == 26); #expect(PopoverView.left(140) == 0)
    unimplemented!("port with the popover pills in Task 13");
}

#[test]
#[ignore = "needs Task 13: the pills' pace strip (Views/AccountPills.swift PaceStrip)"]
fn overspend_is_the_dark_dots_before_the_line() {
    // Swift: 10% left (1.5 columns), half the window to go: line at column 7
    // let l = PaceStrip.layout(left: 10, timeLeft: 0.5, cols: 15); #expect(l.line == 7)
    // #expect(PaceStrip.overspend(column: 3, cover: 0, line: l.line, hot: true))
    // #expect(!PaceStrip.overspend(column: 3, cover: 0, line: l.line, hot: false))
    // #expect(!PaceStrip.overspend(column: 0, cover: 1, line: l.line, hot: true)) // lit
    // #expect(!PaceStrip.overspend(column: 9, cover: 0, line: l.line, hot: true)) // past the line
    unimplemented!("port with the popover pills in Task 13");
}

#[test]
#[ignore = "needs Task 13: the pills' moving fronts (Views/AccountPills.swift PaceFronts)"]
fn fronts_sway_within_bounds_and_keep_moving() {
    // Swift: let fronts = PaceFronts(seed: 11); for frame in 0...3600 { sway = fronts.sway(at: frame / 60, still: false) }
    // #expect(sway.count == 3); #expect(biggest <= PaceFronts.maxSway); #expect(biggest > 0.2)
    // #expect(moving > 1000) (frames with a sway over 0.05); #expect(rows.count == 3)
    unimplemented!("port with the popover pills in Task 13");
}

#[test]
#[ignore = "needs Task 13: the pills' moving fronts (Views/AccountPills.swift PaceFronts)"]
fn still_fronts_do_not_sway() {
    // Swift: let fronts = PaceFronts(seed: 11); for frame in 0...600 { _ = fronts.sway(at: frame / 60, still: false) }
    // #expect(fronts.sway(at: 10.1, still: true) == [0, 0, 0])
    unimplemented!("port with the popover pills in Task 13");
}

#[test]
#[ignore = "needs Task 13: the pills' pace strip (Views/AccountPills.swift PaceStrip)"]
fn front_dot_is_partly_lit_and_the_dots_behind_move_less() {
    // Swift: 3.9 columns lit: columns 0-2 full, column 3 at 90%, column 4 dark
    // #expect(PaceStrip.cover(column: 2, fill: 3.9, sway: 0) == 1); cover(3, 3.9, 0) ≈ 0.9; cover(4, 3.9, 0) == 0
    // cover(4, 3.9, 0.5) ≈ 0.4; cover(0, fill: 0, sway: 0.7) == 0
    // front = shift(3, 3.9, 0.7) ≈ PaceStrip.maxShift; back = shift(0, 3.9, 0.7): 0 < back < front
    // shift(3, 3.9, 0) == 0; shift(6, 3.9, 0.7) == 0 // dark dots stay put
    unimplemented!("port with the popover pills in Task 13");
}

#[test]
fn peak_opens_the_top_thread_of_the_account_that_fills() {
    let mut s = demo_data::peak();
    s.sources = vec![
        token_source("a", Some("ta"), "p", "d", "personal", 9.0, None),
        token_source("b", Some("tb"), "p", "d", "work", 5.0, None),
    ];
    assert_eq!(s.peak_thread(), Some("tb"));
    s.sources.pop();
    assert_eq!(s.peak_thread(), Some("ta"));
    s.sources.clear();
    assert_eq!(s.peak_thread(), None);
}
