//! The mockup's demo snapshots.
use cooked_model::demo_data::{self, Feed};
use cooked_model::token_format;
use cooked_model::usage_snapshot::Urgency;

#[test]
fn short_format() {
    assert_eq!(token_format::short(950.0), "950");
    assert_eq!(token_format::short(9000.0), "9k");
    assert_eq!(token_format::short(40000.0), "40k");
    assert_eq!(token_format::short(184_000.0), "184k");
    assert_eq!(token_format::short(1_200_000.0), "1.2M");
}

#[test]
fn calm_snapshot() {
    let s = demo_data::calm();
    assert_eq!(s.clock, "19:28");
    let shorts = |v: Vec<&cooked_model::usage_snapshot::Account>| -> Vec<String> {
        v.iter().map(|a| a.short.clone()).collect()
    };
    assert_eq!(
        shorts(s.accounts.iter().collect()),
        ["work", "team", "personal", "side", "codex"]
    );
    assert_eq!(
        s.accounts
            .iter()
            .map(|a| a.five_hour_percent)
            .collect::<Vec<_>>(),
        [4, 11, 0, 12, 6]
    );
    assert_eq!(
        s.accounts
            .iter()
            .map(|a| a.five_hour_reset.as_str())
            .collect::<Vec<_>>(),
        ["00:20", "04:40", "—", "01:55", "03:10"]
    );
    assert_eq!(
        s.accounts.iter().map(|a| a.is_live).collect::<Vec<_>>(),
        [true, false, false, false, false]
    );
    assert_eq!(s.active_id, "work");
    assert_eq!(s.rate_now, 9000);
    assert_eq!(s.threshold, 40000);
    assert_eq!(s.series.len(), 60);
    assert_eq!(s.urgency, None);
    assert!(!s.is_urgent());
    assert_eq!(
        s.accounts
            .iter()
            .map(|a| a.week_percent)
            .collect::<Vec<_>>(),
        [Some(74), Some(22), None, Some(41), Some(9)]
    );
    // personal is not used today: hidden
    assert_eq!(shorts(s.rows()), ["work", "team", "side", "codex"]);
    assert_eq!(shorts(s.unused()), ["personal"]);
}

#[test]
fn peak_snapshot() {
    let s = demo_data::peak();
    assert_eq!(s.clock, "23:05");
    assert_eq!(
        s.accounts
            .iter()
            .map(|a| a.five_hour_percent)
            .collect::<Vec<_>>(),
        [82, 12, 31, 12, 9]
    );
    assert_eq!(
        s.accounts
            .iter()
            .map(|a| a.five_hour_reset.as_str())
            .collect::<Vec<_>>(),
        ["00:20", "04:40", "02:40", "01:55", "03:10"]
    );
    assert_eq!(
        s.accounts.iter().map(|a| a.is_live).collect::<Vec<_>>(),
        [true, false, true, false, false]
    );
    assert_eq!(s.rate_now, 184_000);
    assert_eq!(
        s.accounts
            .iter()
            .map(|a| a.week_percent)
            .collect::<Vec<_>>(),
        [Some(79), Some(23), Some(64), Some(41), Some(10)]
    );
    assert!(s.unused().is_empty());
    assert_eq!(
        s.urgency,
        Some(Urgency::new(
            82,
            "23:14",
            "claude-work in 9 min",
            "184k/min from “Refactor usage page”"
        ))
    );
    assert!(s.is_urgent());
}

#[test]
fn series_is_deterministic() {
    assert_eq!(demo_data::series(60, false), demo_data::series(60, false));
    let peak = demo_data::series(60, true);
    assert!(peak.iter().rev().take(8).all(|v| *v > 40000.0));
    let calm = demo_data::series(60, false);
    assert!(calm[41] == 47000.0 || (47000.0..=52000.0).contains(&calm[41]));
    assert!(
        calm.iter()
            .enumerate()
            .filter(|(i, _)| *i != 41)
            .all(|(_, v)| *v < 40000.0)
    );
}

#[test]
fn calm_rates_cycle() {
    assert_eq!(demo_data::CALM_RATES, [9000, 11000, 8000, 10000]);
}

/// The feeds' raw values are the `--feed` arguments.
#[test]
fn feeds_round_trip_their_raw_values() {
    for f in Feed::ALL {
        assert_eq!(Feed::from_raw(f.raw()), Some(f));
    }
    assert_eq!(Feed::DemoPeak.raw(), "demoPeak");
    assert_eq!(Feed::Live.demo(), None);
    assert_eq!(
        Feed::DemoCalm.demo().map(|s| s.clock),
        Some("19:28".to_owned())
    );
}
