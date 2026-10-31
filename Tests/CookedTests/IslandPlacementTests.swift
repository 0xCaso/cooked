import CoreGraphics
import Foundation
import Testing
@testable import Cooked

struct IslandPlacementTests {
    // A 1512×982 screen with a 33 pt menu bar.
    let screen = CGRect(x: 0, y: 0, width: 1512, height: 982)
    let visible = CGRect(x: 0, y: 0, width: 1512, height: 949)
    let size = CGSize(width: 280, height: 84)

    @Test func centeredHorizontally() {
        let frame = IslandPlacement.initialFrame(size: size, visibleFrame: visible)
        #expect(frame.midX == visible.midX)
    }

    @Test func sitsJustBelowTheMenuBar() {
        let frame = IslandPlacement.initialFrame(size: size, visibleFrame: visible)
        // the island (inside the transparent margin) starts 8 pt below the menu bar; the margin may overlap the bar
        #expect(IslandPlacement.gapBelowMenuBar == 8)
        #expect(frame.maxY - IslandPlacement.overhang == visible.maxY - IslandPlacement.gapBelowMenuBar)
        #expect(frame.maxY < screen.maxY)
    }

    @Test func followsAScreenThatIsNotAtTheOrigin() {
        let second = CGRect(x: 1512, y: 200, width: 1920, height: 1055)
        let frame = IslandPlacement.initialFrame(size: size, visibleFrame: second)
        #expect(frame.midX == second.midX)
        #expect(frame.maxY - IslandPlacement.overhang == second.maxY - IslandPlacement.gapBelowMenuBar)
    }

    @Test func keepsTheRequestedSize() {
        let frame = IslandPlacement.initialFrame(size: size, visibleFrame: visible)
        #expect(frame.size == size)
    }

    // The surfaces (the panel minus its transparent margin) stay inside the visible frame.
    func surfaces(_ f: CGRect) -> CGRect { f.insetBy(dx: IslandPlacement.overhang, dy: IslandPlacement.overhang) }

    @Test func clampKeepsWholePanelVisible() {
        let panel = CGSize(width: 452, height: 613)
        for origin in [CGPoint(x: 700, y: -300), CGPoint(x: 1400, y: 200), CGPoint(x: -200, y: 200), CGPoint(x: 600, y: 600)] {
            let f = IslandPlacement.clamped(CGRect(origin: origin, size: panel), to: visible)
            #expect(visible.contains(surfaces(f)), "\(origin) → \(f)")
            #expect(f.size == panel)
        }
        let inside = CGRect(x: 300, y: 200, width: 452, height: 613)
        #expect(IslandPlacement.clamped(inside, to: visible) == inside)
    }

    let main = ScreenInfo(id: 1, visibleFrame: CGRect(x: 0, y: 0, width: 1512, height: 949))
    let second = ScreenInfo(id: 2, visibleFrame: CGRect(x: 1512, y: 0, width: 1920, height: 1050))
    let panel = CGSize(width: 452, height: 613)

    @Test func restoreUsesSavedScreen() {
        let saved = SavedPosition(displayID: 2, topCenter: CGPoint(x: 2400, y: 1050))
        let f = IslandPlacement.restoredFrame(size: panel, saved: saved, screens: [main, second], main: main)
        #expect(f.midX == 2400)
        #expect(f.maxY == 1050)
        #expect(second.visibleFrame.contains(surfaces(f)))
    }

    @Test func restoreFallsBackToMainScreen() {
        let saved = SavedPosition(displayID: 9, topCenter: CGPoint(x: 2400, y: 1050))
        let f = IslandPlacement.restoredFrame(size: panel, saved: saved, screens: [main], main: main)
        #expect(f == IslandPlacement.initialFrame(size: panel, visibleFrame: main.visibleFrame))
        #expect(IslandPlacement.restoredFrame(size: panel, saved: nil, screens: [main], main: main) == f)
    }

    @Test func restoreClampsWhenScreenShrank() {
        let small = ScreenInfo(id: 2, visibleFrame: CGRect(x: 1512, y: 0, width: 1024, height: 700))
        let saved = SavedPosition(displayID: 2, topCenter: CGPoint(x: 3300, y: 1050))
        let f = IslandPlacement.restoredFrame(size: panel, saved: saved, screens: [main, small], main: main)
        #expect(small.visibleFrame.contains(surfaces(f)))
    }

    @Test func screenUnderThePointer() {
        #expect(IslandPlacement.screen(containing: CGPoint(x: 2000, y: 500), in: [main, second])?.id == 2)
        #expect(IslandPlacement.screen(containing: CGPoint(x: 100, y: 100), in: [main, second])?.id == 1)
        #expect(IslandPlacement.screen(containing: CGPoint(x: -100, y: 100), in: [main, second]) == nil)
    }

    @Test func savedPositionRoundTrips() throws {
        let p = SavedPosition(displayID: 3, topCenter: CGPoint(x: 10.5, y: 20))
        let data = try JSONEncoder().encode(p)
        #expect(try JSONDecoder().decode(SavedPosition.self, from: data) == p)
    }
}

struct PositionStoreTests {
    @Test func savesAndLoads() throws {
        let defaults = try #require(UserDefaults(suiteName: "cooked-tests-\(UUID().uuidString)"))
        #expect(PositionStore.load(from: defaults) == nil)
        let p = SavedPosition(displayID: 2, topCenter: CGPoint(x: 1720, y: 1426))
        PositionStore.save(p, to: defaults)
        #expect(PositionStore.load(from: defaults) == p)
    }
}
