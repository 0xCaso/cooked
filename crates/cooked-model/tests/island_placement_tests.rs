//! `IslandPlacementTests.swift` and `PositionStoreTests`, plus Review Focus 4 (`missing_display_falls_back_to_main_top_center`).

use cooked_model::defaults::MemoryDefaults;
use cooked_model::island_placement::{
    GAP_BELOW_MENU_BAR, OVERHANG, Point, Rect, ScreenInfo, Size, clamped, initial_frame,
    restored_frame, screen_containing,
};
use cooked_model::position_store::{self, IslandPosition};

// A 1512×982 screen with a 33 pt menu bar.
const SCREEN: Rect = Rect::new(0.0, 0.0, 1512.0, 982.0);
const VISIBLE: Rect = Rect::new(0.0, 0.0, 1512.0, 949.0);
const SIZE: Size = Size::new(280.0, 84.0);

const MAIN: ScreenInfo = ScreenInfo {
    id: 1,
    visible_frame: Rect::new(0.0, 0.0, 1512.0, 949.0),
};
const SECOND: ScreenInfo = ScreenInfo {
    id: 2,
    visible_frame: Rect::new(1512.0, 0.0, 1920.0, 1050.0),
};
const PANEL: Size = Size::new(452.0, 613.0);

/// The surfaces (the panel minus its transparent margin) stay inside the visible frame.
fn surfaces(f: Rect) -> Rect {
    f.inset_by(OVERHANG, OVERHANG)
}

fn saved(display_id: u32, x: f64, y: f64) -> IslandPosition {
    IslandPosition {
        display_id,
        top_center: [x, y],
    }
}

#[test]
fn centered_horizontally() {
    let frame = initial_frame(SIZE, VISIBLE);
    assert_eq!(frame.mid_x(), VISIBLE.mid_x());
}

#[test]
fn sits_just_below_the_menu_bar() {
    let frame = initial_frame(SIZE, VISIBLE);
    // the island (inside the transparent margin) starts 8 pt below the menu bar; the margin may overlap the bar
    assert_eq!(GAP_BELOW_MENU_BAR, 8.0);
    assert_eq!(
        frame.max_y() - OVERHANG,
        VISIBLE.max_y() - GAP_BELOW_MENU_BAR
    );
    assert!(frame.max_y() < SCREEN.max_y());
}

#[test]
fn follows_a_screen_that_is_not_at_the_origin() {
    let second = Rect::new(1512.0, 200.0, 1920.0, 1055.0);
    let frame = initial_frame(SIZE, second);
    assert_eq!(frame.mid_x(), second.mid_x());
    assert_eq!(
        frame.max_y() - OVERHANG,
        second.max_y() - GAP_BELOW_MENU_BAR
    );
}

#[test]
fn keeps_the_requested_size() {
    let frame = initial_frame(SIZE, VISIBLE);
    assert_eq!(frame.size(), SIZE);
}

#[test]
fn clamp_keeps_whole_panel_visible() {
    for origin in [
        Point::new(700.0, -300.0),
        Point::new(1400.0, 200.0),
        Point::new(-200.0, 200.0),
        Point::new(600.0, 600.0),
    ] {
        let f = clamped(
            Rect::new(origin.x, origin.y, PANEL.width, PANEL.height),
            VISIBLE,
        );
        assert!(VISIBLE.contains_rect(surfaces(f)), "{origin:?} → {f:?}");
        assert_eq!(f.size(), PANEL);
    }
    let inside = Rect::new(300.0, 200.0, 452.0, 613.0);
    assert_eq!(clamped(inside, VISIBLE), inside);
}

#[test]
fn restore_uses_saved_screen() {
    let f = restored_frame(
        PANEL,
        Some(&saved(2, 2400.0, 1050.0)),
        &[MAIN, SECOND],
        &MAIN,
    );
    assert_eq!(f.mid_x(), 2400.0);
    assert_eq!(f.max_y(), 1050.0);
    assert!(SECOND.visible_frame.contains_rect(surfaces(f)));
}

#[test]
fn restore_falls_back_to_main_screen() {
    let f = restored_frame(PANEL, Some(&saved(9, 2400.0, 1050.0)), &[MAIN], &MAIN);
    assert_eq!(f, initial_frame(PANEL, MAIN.visible_frame));
    assert_eq!(restored_frame(PANEL, None, &[MAIN], &MAIN), f);
}

#[test]
fn restore_clamps_when_screen_shrank() {
    let small = ScreenInfo {
        id: 2,
        visible_frame: Rect::new(1512.0, 0.0, 1024.0, 700.0),
    };
    let f = restored_frame(
        PANEL,
        Some(&saved(2, 3300.0, 1050.0)),
        &[MAIN, small],
        &MAIN,
    );
    assert!(small.visible_frame.contains_rect(surfaces(f)));
}

#[test]
fn screen_under_the_pointer() {
    let screens = [MAIN, SECOND];
    assert_eq!(
        screen_containing(Point::new(2000.0, 500.0), &screens).map(|s| s.id),
        Some(2)
    );
    assert_eq!(
        screen_containing(Point::new(100.0, 100.0), &screens).map(|s| s.id),
        Some(1)
    );
    assert_eq!(screen_containing(Point::new(-100.0, 100.0), &screens), None);
}

#[test]
fn saved_position_round_trips() {
    let p = saved(3, 10.5, 20.0);
    let data = position_store::encode(&p);
    assert_eq!(position_store::decode(&data), Some(p));
}

/// Review Focus 4: the display the island sat on was unplugged; the main display is not first in the list.
#[test]
fn missing_display_falls_back_to_main_top_center() {
    let saved = saved(2, 2400.0, 1050.0);
    let other = ScreenInfo {
        id: 3,
        visible_frame: Rect::new(-1920.0, 0.0, 1920.0, 1050.0),
    };
    let f = restored_frame(SIZE, Some(&saved), &[other, MAIN], &MAIN);
    assert_eq!(f, initial_frame(SIZE, MAIN.visible_frame));
    assert_eq!(f.mid_x(), MAIN.visible_frame.mid_x());
    assert_eq!(
        f.max_y() - OVERHANG,
        MAIN.visible_frame.max_y() - GAP_BELOW_MENU_BAR
    );
}

#[test]
fn reads_the_json_swift_writes() {
    // JSONEncoder writes CGPoint as [x, y] and whole doubles without a fraction.
    let swift = br#"{"topCenter":[1720,1426],"displayID":2}"#;
    assert_eq!(
        position_store::decode(swift),
        Some(saved(2, 1720.0, 1426.0))
    );
    let json: serde_json::Value =
        serde_json::from_slice(&position_store::encode(&saved(2, 10.5, 20.0))).unwrap_or_default();
    assert_eq!(
        json,
        serde_json::json!({"displayID": 2, "topCenter": [10.5, 20.0]})
    );
}

#[test]
fn bad_json_loads_as_none() {
    assert_eq!(position_store::decode(&[1, 2]), None);
}

mod position_store_tests {
    use super::*;

    #[test]
    fn saves_and_loads() {
        let mut defaults = MemoryDefaults::default();
        assert_eq!(position_store::load(&defaults), None);
        let p = saved(2, 1720.0, 1426.0);
        position_store::save(&p, &mut defaults);
        assert_eq!(position_store::load(&defaults), Some(p));
        assert!(defaults.data.contains_key(position_store::KEY));
    }
}
