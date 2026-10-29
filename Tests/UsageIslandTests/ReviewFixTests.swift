import AppKit
import Foundation
import Testing
@testable import UsageIsland

/// Findings of the final review, each pinned by a test.
@MainActor
struct ReviewFixTests {
    /// The charts' time must not restart when the island is clicked or the demo switches.
    @Test func chartEpochSurvivesOpenCloseAndDemo() {
        let model = IslandModel()
        let epoch = model.chartEpoch
        model.open(); model.close(); model.toggleDemo(); model.toggleDemo()
        #expect(model.chartEpoch == epoch)
    }

    /// Any key other than Esc closes the popover (and hands the keyboard back) instead of beeping.
    @Test func typingClosesThePopover() throws {
        let model = IslandModel()
        let panel = IslandPanel(model: model)
        model.open()
        let event = try #require(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0,
                                                  windowNumber: panel.windowNumber, context: nil, characters: "a",
                                                  charactersIgnoringModifiers: "a", isARepeat: false, keyCode: 0))
        panel.keyDown(with: event)
        #expect(!model.isOpen)
    }

    /// Each press starts a fresh drag; a cancelled gesture cannot leave a stale start behind.
    @Test func eachPressStartsAFreshDrag() {
        let panel = IslandPanel(model: IslandModel())
        panel.dragBegan()
        let first = panel.dragStart?.origin
        panel.setFrameOrigin(CGPoint(x: 300, y: 300))
        panel.dragBegan() // no dragEnded in between: the previous gesture was cancelled
        #expect(panel.dragStart?.origin == CGPoint(x: 300, y: 300))
        #expect(first != panel.dragStart?.origin)
    }

    /// Closing the popover by dragging waits for the drag to end before reordering the window.
    @Test func handBackWaitsForTheDragToEnd() {
        let model = IslandModel()
        let panel = IslandPanel(model: model)
        model.open()
        panel.dragBegan()
        panel.dragChanged()
        #expect(!model.isOpen)
        #expect(panel.isDragging)
        panel.dragEnded()
        #expect(!panel.isDragging)
        #expect(panel.dragStart == nil)
    }

    /// The bundle's fonts are registered too, not left to ATSApplicationFontsPath alone.
    @Test func fontsComeFromTheBundleWhenItHasThem() throws {
        let res = FileManager.default.temporaryDirectory.appendingPathComponent("res-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: res.appendingPathComponent("Fonts"), withIntermediateDirectories: true)
        #expect(AppDelegate.fontsDirectory(resourceURL: res) == res.appendingPathComponent("Fonts"))
        #expect(AppDelegate.fontsDirectory(resourceURL: nil).path.hasSuffix("Resources/Fonts"))
        #expect(AppDelegate.fontsDirectory(resourceURL: res.appendingPathComponent("missing")).path.hasSuffix("Resources/Fonts"))
    }
}

/// Bug found with a manual right-click test: while the context menu tracks, the pointer leaves the island,
/// the panel went back to ignoring the mouse, and no menu item could be chosen.
@MainActor
struct ContextMenuTests {
    @Test func panelKeepsTheMouseWhileAMenuTracks() {
        let panel = IslandPanel(model: IslandModel()) // no hit rects: the pointer is never "inside"
        panel.ignoresMouseEvents = false
        NotificationCenter.default.post(name: NSMenu.didBeginTrackingNotification, object: NSMenu())
        panel.updateMouseEvents()
        #expect(!panel.ignoresMouseEvents)
        NotificationCenter.default.post(name: NSMenu.didEndTrackingNotification, object: NSMenu())
        panel.updateMouseEvents()
        #expect(panel.ignoresMouseEvents)
    }
}

@MainActor
struct ReopenTests {
    /// Opening the app from Applications or Spotlight while it already runs shows the popover.
    @Test func reopeningShowsThePopover() {
        let delegate = AppDelegate()
        #expect(!delegate.model.isOpen)
        _ = delegate.applicationShouldHandleReopen(NSApplication.shared, hasVisibleWindows: true)
        #expect(delegate.model.isOpen)
    }
}
