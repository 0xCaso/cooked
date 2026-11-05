import AppKit
import Foundation
import Testing
@testable import Cooked

@Suite struct MenuBarTests {
    @Test func waveColumnsFollowTheLastNineMinutesAgainstTheThreshold() {
        let values: [Double] = [999, 0, 39, 40, 89, 90, 200, 10, 50, 100]
        #expect(MenuBarWave.heights(values, threshold: 100) == [0, 0, 1, 1, 2, 2, 0, 1, 2])
    }

    @Test func waveWithFewMinutesStartsFlat() {
        #expect(MenuBarWave.heights([100], threshold: 100) == [0, 0, 0, 0, 0, 0, 0, 0, 2])
        #expect(MenuBarWave.heights([50, 50], threshold: 0) == Array(repeating: 0, count: 9))
    }

    @Test func popoverHangsUnderTheItemAndStaysOnScreen() {
        let visible = CGRect(x: 0, y: 0, width: 1512, height: 945)
        let size = CGSize(width: 452, height: 700)
        let middle = IslandPlacement.underItem(CGRect(x: 700, y: 945, width: 60, height: 37), size: size, visibleFrame: visible)
        #expect(middle.midX == 730)
        #expect(middle.maxY == 945 - IslandPlacement.gapBelowItem + IslandPlacement.overhang)
        // next to the clock: pushed left so the popover (the frame minus its margin) ends at the screen edge
        let edge = IslandPlacement.underItem(CGRect(x: 1400, y: 945, width: 60, height: 37), size: size, visibleFrame: visible)
        #expect(edge.maxX == 1512 + IslandPlacement.overhang)
    }

    @Test @MainActor func displayModeIsKeptAndStartsAsTheIsland() {
        let defaults = UserDefaults(suiteName: "MenuBarTests-\(UUID().uuidString)")!
        #expect(IslandModel(defaults: defaults).displayMode == .island)
        IslandModel(defaults: defaults).displayMode = .menuBar
        #expect(IslandModel(defaults: defaults).displayMode == .menuBar)
    }

    /// The menu bar item's mouse-down closes the popover as a click elsewhere; its click right after must not reopen it.
    @Test @MainActor func aClickElsewhereCountsForHalfASecond() {
        let model = IslandModel(defaults: UserDefaults(suiteName: "MenuBarTests-\(UUID().uuidString)")!)
        #expect(!model.closedByThisClick())
        model.open()
        model.dismiss(.clickOutside)
        #expect(model.closedByThisClick())
        #expect(!model.closedByThisClick(now: Date().addingTimeInterval(0.6)))
        model.open()
        model.close()
        model.open()
        model.dismiss(.escape)
        #expect(!model.closedByThisClick(now: Date().addingTimeInterval(0.6)))
    }

    /// A menu item with a `MenuAction` target runs its closure (it once raised "unrecognized selector").
    @Test @MainActor func menuItemsRunTheirAction() {
        var ran = false
        _ = NSApplication.shared // menu actions go through NSApp
        let action = MenuAction { ran = true }
        let menu = NSMenu()
        let item = menu.addItem(withTitle: "Quit", action: #selector(MenuAction.fire(_:)), keyEquivalent: "")
        item.target = action
        menu.performActionForItem(at: 0)
        #expect(ran)
    }
}
