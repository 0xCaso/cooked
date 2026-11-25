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
