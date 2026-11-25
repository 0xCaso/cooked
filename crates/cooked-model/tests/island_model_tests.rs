//! The island's state with no AppKit. The panel-size cases measure views: they live in
//! `crates/cooked`.

use cooked_model::defaults::MemoryDefaults;
use cooked_model::demo_data;
use cooked_model::island_model::{IslandHitTest, IslandState, Point, Rect};

fn model() -> IslandState<MemoryDefaults> {
    IslandState::new(demo_data::calm(), None, MemoryDefaults::default(), 0.0)
}

#[test]
fn starts_calm_and_closed() {
    let model = model();
    assert_eq!(model.snapshot, demo_data::calm());
    assert!(!model.is_open);
    assert_eq!(model.rate_now, 9000);
}

#[test]
fn toggle_demo_switches_calm_and_peak() {
    let mut model = model();
    model.toggle_demo();
    assert_eq!(model.snapshot, demo_data::peak());
    assert_eq!(model.rate_now, 184_000);
    model.toggle_demo();
    assert_eq!(model.snapshot, demo_data::calm());
    assert_eq!(model.rate_now, 9000);
}

#[test]
fn open_and_close() {
    let mut model = model();
    model.open();
    assert!(model.is_open);
    model.close();
    assert!(!model.is_open);
}

#[test]
fn calm_rate_cycles_every_tick() {
    let mut model = model();
    let mut seen = Vec::new();
    for _ in 0..5 {
        model.tick_rate();
        seen.push(model.rate_now);
    }
    assert_eq!(seen, [11000, 8000, 10000, 9000, 11000]);
}

#[test]
fn peak_rate_stays_on_tick() {
    let mut model = model();
    model.toggle_demo();
    model.tick_rate();
    assert_eq!(model.rate_now, 184_000);
}

// panel 452 × 600 with its bottom-left corner at (1000, 800) on screen; rects are top-left based
const PANEL: Rect = Rect {
    x: 1000.0,
    y: 800.0,
    width: 452.0,
    height: 600.0,
};
const ISLAND: Rect = Rect {
    x: 106.0,
    y: 24.0,
    width: 240.0,
    height: 44.0,
};

#[test]
fn point_on_island_hits() {
    // 30 pt below the panel top, 200 pt in
    assert!(IslandHitTest::hits(
        Point {
            x: 1200.0,
            y: 1370.0
        },
        &[ISLAND],
        PANEL
    ));
}

#[test]
fn transparent_margin_passes_through() {
    assert!(!IslandHitTest::hits(
        Point {
            x: 1010.0,
            y: 1370.0
        },
        &[ISLAND],
        PANEL
    ));
    assert!(!IslandHitTest::hits(
        Point {
            x: 1200.0,
            y: 1000.0
        },
        &[ISLAND],
        PANEL
    ));
}

#[test]
fn outside_the_panel_passes_through() {
    assert!(!IslandHitTest::hits(
        Point { x: 10.0, y: 10.0 },
        &[ISLAND],
        PANEL
    ));
}

#[test]
fn tallest_has_the_full_list_and_every_account() {
    use cooked_model::token_sources::LIMIT;
    let s = IslandState::<MemoryDefaults>::tallest(7);
    assert_eq!(s.sources.len(), LIMIT);
    assert_eq!(s.rows().len(), 7);
    assert_eq!(
        IslandState::<MemoryDefaults>::tallest(2).rows().len(),
        demo_data::peak().rows().len()
    );
}

#[test]
fn closing_by_a_click_elsewhere_swallows_that_same_click() {
    use cooked_model::island_model::DismissReason;
    let mut model = model();
    model.open();
    model.dismiss_popover(DismissReason::ClickOutside, 100.0);
    assert!(!model.is_open);
    assert!(model.closed_by_this_click(100.3));
    assert!(!model.closed_by_this_click(100.6));
    model.open();
    model.dismiss_popover(DismissReason::Escape, 200.0);
    assert!(!model.closed_by_this_click(200.1));
}

/// The demo peak's new minutes carry on at the level the series ends on: before, the feed drew them from the calm
/// start of the series (minutes 5...54), so the chart's right side dropped to a fifth after launch.
#[test]
fn demo_minutes_after_launch_keep_the_series_level() {
    for snapshot in [demo_data::peak(), demo_data::calm()] {
        // the peak's burst: its last 8 minutes
        let last: Vec<f64> = snapshot.series[snapshot.series.len() - 8..].to_vec();
        let lowest = last.iter().copied().fold(f64::INFINITY, f64::min);
        let before = last.iter().sum::<f64>() / last.len() as f64;
        let mut clock = IslandState::<MemoryDefaults>::demo_clock(&snapshot);
        // 12 demo minutes of 4 s: the last 10 closed minutes are all new
        let mut history = clock.history(0.0, false);
        for k in 1..=48 {
            history = clock.history(k as f64, false);
        }
        let n = history.values.len();
        let fresh = &history.values[n - 11..n - 1];
        let after = fresh.iter().sum::<f64>() / fresh.len() as f64;
        assert!(
            after > 0.6 * before && after < 1.4 * before,
            "{}: new minutes average {after:.0}, the series ended at {before:.0}",
            snapshot.clock
        );
        // no single new minute falls under the burst either (a dip in the chart's right side): ±15% of its minutes
        for v in fresh {
            assert!(
                *v >= 0.85 * lowest - 1e-6,
                "{}: a new minute of {v:.0}, the burst's lowest {lowest:.0}",
                snapshot.clock
            );
        }
    }
}
