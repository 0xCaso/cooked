//! `ReviewFixTests.swift`: findings of the final review, each pinned by a test. Every case drives AppKit objects
//! (`IslandModel`, `IslandPanel`, `AppDelegate`), which `cooked-model` does not have: they wait for the tasks that
//! port those types; `IslandModel` is pure state since Task 7.

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

#[test]
#[ignore = "needs Task 12: the island panel's key handling (IslandPanel.keyDown)"]
fn typing_closes_the_popover() {
    // Swift: model.open(); panel.keyDown(with: "a" key event); #expect(!model.isOpen)
    unimplemented!("port with the island view in Task 12");
}

#[test]
#[ignore = "needs Task 12: the island drag (IslandPanel.dragBegan/dragStart)"]
fn each_press_starts_a_fresh_drag() {
    // Swift: panel.dragBegan(); first = dragStart.origin; setFrameOrigin(300, 300); panel.dragBegan();
    // #expect(dragStart.origin == (300, 300)); #expect(first != dragStart.origin)
    unimplemented!("port with the island view in Task 12");
}

#[test]
#[ignore = "needs Task 12: the island drag (IslandPanel.dragChanged/dragEnded)"]
fn hand_back_waits_for_the_drag_to_end() {
    // Swift: model.open(); dragBegan(); dragChanged(); #expect(!model.isOpen); #expect(isDragging);
    // dragEnded(); #expect(!isDragging); #expect(dragStart == nil)
    unimplemented!("port with the island view in Task 12");
}

#[test]
#[ignore = "needs Task 12: bundled fonts (AppDelegate.fontsDirectory)"]
fn fonts_come_from_the_bundle_when_it_has_them() {
    // Swift: fontsDirectory(resourceURL: res with Fonts/) == res/Fonts; nil or missing → path ends "Resources/Fonts"
    unimplemented!("port with the fonts in Task 12");
}

#[test]
#[ignore = "needs Task 12: the right-click menu (IslandPanel.updateMouseEvents)"]
fn panel_keeps_the_mouse_while_a_menu_tracks() {
    // Swift: menu didBeginTracking → updateMouseEvents → #expect(!ignoresMouseEvents); didEndTracking →
    // #expect(ignoresMouseEvents)
    unimplemented!("port with the island view in Task 12");
}

// ReopenTests.reopeningShowsThePopover: `reopening_shows_the_popover` in `crates/cooked/src/island.rs` (Task 17).
