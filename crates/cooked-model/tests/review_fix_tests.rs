//! `ReviewFixTests.swift`: findings of the final review, each pinned by a test. The cases that drove AppKit objects
//! (`IslandPanel`, `AppDelegate`) live in `crates/cooked` or no longer apply; `IslandModel` is pure state since Task 7.

use cooked_model::defaults::MemoryDefaults;
use cooked_model::demo_data;
use cooked_model::island_model::IslandState;

#[test]
fn chart_epoch_survives_open_close_and_demo() {
    let mut model = IslandState::new(demo_data::calm(), None, MemoryDefaults::default(), 1234.5);
    let epoch = model.chart_epoch();
    model.open();
    model.close();
    model.toggle_demo();
    model.toggle_demo();
    assert_eq!(model.chart_epoch(), epoch);
}

// ReviewFixTests.typingClosesThePopover: `esc_and_any_key_close_it` in `crates/cooked/src/popover/tests.rs` (the
// popover's own overlay takes the keys, `popover::on_key`).
// ReviewFixTests.eachPressStartsAFreshDrag: `each_press_starts_a_fresh_drag` in `crates/cooked/src/island.rs`.
// ReviewFixTests.handBackWaitsForTheDragToEnd: the wait does not apply (Swift handed the keyboard back by reordering
// the panel being dragged; the popover is now its own overlay); the drag closing the popover and the release ending
// it: `a_drag_closes_the_popover_and_the_release_ends_it` in `crates/cooked/src/island.rs`.
// ReviewFixTests.fontsComeFromTheBundleWhenItHasThem: does not apply, the Plex faces are embedded with
// `include_bytes!` (`font_files_are_the_four_static_faces` in `crates/cooked/src/theme.rs`).
// ContextMenuTests.panelKeepsTheMouseWhileAMenuTracks: does not apply, Cooked no longer toggles the panel's
// `ignoresMouseEvents`: Rune's `OverlayInput::Regions` owns it, and the menu tracks in AppKit's own menu window
// (`tx.popup_menu`, `popUpMenuPositioningItem`).
// ReopenTests.reopeningShowsThePopover: `reopening_shows_the_popover` in `crates/cooked/src/island.rs` (Task 17).
