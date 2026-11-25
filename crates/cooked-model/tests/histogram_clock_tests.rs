//! The demo clock's minutes and the account bar.

use cooked_model::account_bar::AccountBar;
use cooked_model::histogram_clock::HistogramClock;

#[test]
fn current_minute_grows() {
    let mut clock = HistogramClock::new(vec![10000.0; 60], 40000.0, 4.0, Box::new(|| 20000.0));
    clock.history(0.0, false);
    clock.history(4.0, false); // minute closes: the last minute now fills towards 20000
    assert!((clock.history(6.0, false).values[59] - 10000.0).abs() < 1e-6);
    assert!((clock.history(7.0, false).values[59] - 15000.0).abs() < 1e-6);
}

#[test]
fn closing_minute_moves_the_history_left() {
    let values: Vec<f64> = (0..60).map(|i| 1000.0 * (i as f64 + 1.0)).collect();
    let mut clock = HistogramClock::new(values, 40000.0, 4.0, Box::new(|| 0.0));
    assert_eq!(clock.history(0.0, false).values[10], 11000.0);
    let after = clock.history(4.0, false);
    assert_eq!(after.values[10], 12000.0);
    assert_eq!(after.values[58], 60000.0); // the closed minute keeps its full total
    assert_eq!(after.values.len(), 60);
}

#[test]
fn still_shows_full_minutes() {
    let mut clock = HistogramClock::new(vec![1000.0; 60], 40000.0, 4.0, Box::new(|| 30000.0));
    clock.history(0.0, true);
    assert_eq!(clock.history(4.1, true).values[59], 30000.0);
}

#[test]
fn top_never_below_the_threshold_margin() {
    let mut clock = HistogramClock::new(vec![1000.0], 40000.0, 60.0, Box::new(|| 0.0));
    assert_eq!(clock.history(0.0, true).top, 64000.0);
    let mut high = HistogramClock::new(vec![640_000.0], 40000.0, 60.0, Box::new(|| 0.0));
    assert_eq!(high.history(0.0, true).top, 640_000.0);
}

#[test]
fn account_bar_lit_count() {
    let lit = |p: i64| {
        AccountBar::grid(p, false)
            .level
            .iter()
            .filter(|l| **l == 1.0)
            .count()
    };
    assert_eq!(lit(0), 0);
    assert_eq!(lit(4), 1);
    assert_eq!(lit(82), 16);
    assert_eq!(lit(100), 20);
    assert_eq!(lit(140), 20);
    assert!(AccountBar::grid(50, true).hot.iter().all(|h| *h));
}
