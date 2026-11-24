//! `IslandModelTests.swift`: the island's state with no AppKit. The panel-size cases measure SwiftUI views and wait
//! for the island view.

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

#[test]
#[ignore = "needs Task 12: IslandLayout.panelSize measures the SwiftUI island and popover"]
fn panel_size_holds_open_peak() {
    // Swift: let size = IslandLayout.panelSize; #expect(size.width == 404 + 2 * 24)
    // #expect(size.height >= 900 && size.height <= max(IslandLayout.maxPanelHeight, 945))
}

// MARK: - IslandHitTestTests

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

// MARK: - PanelSizeTests

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
#[ignore = "needs Task 12: measures the SwiftUI popover with the sources list open and closed"]
fn the_open_list_makes_the_panel_taller() {
    // Swift: height(expanded, accounts) = NSHostingView(IslandRoot(model)).fittingSize.height
    // #expect(height(true, 5) > height(false, 5) + 100); #expect(height(true, 7) > height(true, 5))
}

// MARK: - Review fixes and live state

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
