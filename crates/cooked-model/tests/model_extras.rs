//! Not Swift cases: checks for ported code whose Swift tests live in later tasks' files.

use cooked_model::clock::{FixedOffset, days_from_civil, iso_date};
use cooked_model::status_file::{StatusFile, WatcherTime};
use cooked_model::{status_line_bridge, token_format};

#[test]
fn token_format_short() {
    assert_eq!(token_format::short(950.0), "950");
    assert_eq!(token_format::short(9000.0), "9k");
    assert_eq!(token_format::short(1_200_000.0), "1.2M");
}

#[test]
fn iso_dates_need_a_fraction() {
    assert!(
        iso_date("2026-10-08T16:46:11.683Z").is_some_and(|t| (t - 1_791_477_971.683).abs() < 1e-6)
    );
    assert_eq!(
        iso_date("2026-10-08T18:46:11.683+02:00"),
        iso_date("2026-10-08T16:46:11.683Z")
    );
    assert_eq!(iso_date("2026-10-08T16:46:11Z"), None);
}

#[test]
fn watcher_times_are_local_and_roll_into_next_year() {
    let tz = FixedOffset(3600);
    // 30/12/2026 12:00 local
    let now = (days_from_civil(2026, 12, 30) * 86_400 + 11 * 3600) as f64;
    assert_eq!(
        WatcherTime::parse("14:05", now, &tz),
        Some(now + 2.0 * 3600.0 + 300.0)
    );
    let jan = (days_from_civil(2027, 1, 2) * 86_400 + 9 * 3600) as f64;
    assert_eq!(WatcherTime::parse("02/01 10:00", now, &tz), Some(jan));
    assert_eq!(WatcherTime::parse("nope", now, &tz), None);
}

#[test]
fn status_file_keeps_account_ids() {
    let s = StatusFile::decode(
        br#"{"updated_at":"10:00","accounts":{"claudeAgent_work":{"name":"work","five_hour_pct":40,"projection":{"window":"5h","eta":"11:00"}}}}"#,
    )
    .unwrap();
    let e = &s.accounts["claudeAgent_work"];
    assert_eq!(e.five_hour_pct, Some(40));
    assert_eq!(e.projection.as_ref().map(|p| p.eta.as_str()), Some("11:00"));
    assert_eq!(s.total_threshold_15m, None);
}

#[test]
fn bridge_sample_reads_rate_limits() {
    let json = br#"{"rate_limits":{"five_hour":{"used_percentage":100,"resets_at":1700000000},"seven_day":{"used_percentage":12.5}}}"#;
    let s = status_line_bridge::sample(json, 5.0).unwrap();
    assert_eq!(
        (
            s.five,
            s.five_reset,
            s.week,
            s.week_reset,
            s.status.as_str()
        ),
        (1.0, Some(1.7e9), 0.125, None, "rejected")
    );
    assert!(status_line_bridge::sample(br#"{"rate_limits":{}}"#, 5.0).is_none());
}
