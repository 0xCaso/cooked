//! `ChartRangeTests.swift`: how far back the popover's chart looks.

use cooked_model::defaults::MemoryDefaults;
use cooked_model::demo_data;
use cooked_model::histogram_clock::{ChartRange, MinuteHistory};
use cooked_model::island_model::IslandState;
use cooked_model::tide::TideScale;

#[test]
fn each_range_shows_its_span_of_minutes() {
    // a day of minutes: each minute's value is its age, the newest last
    let day: Vec<f64> = (0..1440).map(|i| (1439 - i) as f64).collect();
    let history = MinuteHistory {
        values: day,
        top: 2000.0,
        threshold: 100.0,
    };
    for range in ChartRange::ALL {
        let w = history.window(range.minutes());
        assert_eq!(w.values.len(), range.minutes());
        assert_eq!(w.values.last(), Some(&0.0)); // the minute in progress
        assert_eq!(w.values.first(), Some(&((range.minutes() - 1) as f64))); // the oldest minute of the range
    }
    let raws: Vec<&str> = ChartRange::ALL.iter().map(|r| r.raw()).collect();
    assert_eq!(raws, ["10m", "30m", "1h", "4h", "1d"]);
}

#[test]
fn a_range_longer_than_the_data_starts_empty() {
    let hour = MinuteHistory {
        values: vec![5000.0; 60],
        top: 8000.0,
        threshold: 1000.0,
    };
    let w = hour.window(240);
    assert!(w.values[..180].iter().all(|v| *v == 0.0));
    assert!(w.values[180..].iter().all(|v| *v == 5000.0));
}

#[test]
fn the_scale_fits_the_range_alone() {
    let mut values = vec![1000.0; 1440];
    values[100] = 90_000.0; // a spike 22 hours ago
    let history = MinuteHistory {
        values,
        top: 90_000.0,
        threshold: 1000.0,
    };
    assert_eq!(history.window(1440).top, 90_000.0);
    assert_eq!(history.window(10).top, 1600.0); // 1.6 × the threshold: the old spike does not flatten the last 10 minutes
}

#[test]
fn ten_minutes_bins_each_minute_over_its_columns() {
    let mut minutes = vec![0.0; 10];
    minutes[5] = 64000.0;
    let t = TideScale::targets(&minutes, 84, 24, 64000.0);
    // 8.4 columns per minute: minute 5 covers columns 42-50, partly 50
    assert!((t[43] - 24.0).abs() < 1e-9 && (t[49] - 24.0).abs() < 1e-9);
    assert!(t[41] == 0.4 && t[51] == 0.4);
}

#[test]
fn the_range_is_kept_across_launches() {
    let mut first = IslandState::new(demo_data::calm(), None, MemoryDefaults::default(), 0.0);
    assert_eq!(first.chart_range(), ChartRange::TenMinutes);
    first.set_chart_range(ChartRange::FourHours);
    let defaults = first.into_defaults();
    assert_eq!(
        IslandState::new(demo_data::calm(), None, defaults, 0.0).chart_range(),
        ChartRange::FourHours
    );
}
