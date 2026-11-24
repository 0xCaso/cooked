//! `ReviewFixTests.swift`: findings of the final review, each pinned by a test. Every case drives AppKit objects
//! (`IslandModel`, `IslandPanel`, `AppDelegate`), which `cooked-model` does not have: they wait for the tasks that
//! port those types.

#[test]
#[ignore = "needs Task 7: IslandModel (chartEpoch, open/close, toggleDemo)"]
fn chart_epoch_survives_open_close_and_demo() {
    // Swift: let epoch = model.chartEpoch; model.open(); model.close(); model.toggleDemo(); model.toggleDemo();
    // #expect(model.chartEpoch == epoch)
    unimplemented!("port with IslandModel in Task 7");
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

#[test]
#[ignore = "needs Task 17: reopen handling (applicationShouldHandleReopen)"]
fn reopening_shows_the_popover() {
    // Swift: #expect(!model.isOpen); applicationShouldHandleReopen(...); #expect(model.isOpen)
    unimplemented!("port with reopen in Task 17");
}
