//! Even pace and the pills' strips.
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
    assert_eq!(
        live_snapshot::elapsed(now + 9.0 * 3600.0, 5.0 * 3600.0, now),
        0.0
    );
}

#[test]
fn weekday_is_short_english() {
    // 02/11/2026 09:00 in Rome (UTC+1)
    let monday = (days_from_civil(2026, 11, 2) * 86_400 + 8 * 3600) as f64;
    assert_eq!(live_snapshot::weekday(monday, &FixedOffset(3600)), "Mon");
}

#[test]
#[ignore = "parity gap, pending a design choice: the pills' bar fronts do not sway (Swift PaceFronts)"]
fn fronts_sway_within_bounds_and_keep_moving() {
    // Three fronts, seed 11, over 3600 frames at 60 fps: no sway past the max sway, the biggest over 0.2, more than
    // 1000 frames swaying over 0.05.
    unimplemented!("the pills' bar fronts do not sway");
}

#[test]
#[ignore = "parity gap, pending a design choice: the pills' bar fronts do not sway (Swift PaceFronts)"]
fn still_fronts_do_not_sway() {
    // Seed 11, 600 frames moving, then still at 10.1 s: every sway is 0.
    unimplemented!("the pills' bar fronts do not sway");
}

#[test]
#[ignore = "parity gap, pending a design choice: whole tiles, no partly lit front or shifting dots (Swift PaceStrip.cover/shift)"]
fn front_dot_is_partly_lit_and_the_dots_behind_move_less() {
    // 3.9 columns lit: columns 0-2 full, column 3 at 90%, column 4 dark, and at 40% with a sway of 0.5; a fill of 0
    // lights nothing. With a sway of 0.7 the front dot shifts the most and the dots behind it less; with no sway, or
    // dark, a dot stays put.
    unimplemented!("tiles are whole: no partly lit front or shifting dots");
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
