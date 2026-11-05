import CoreGraphics
import SwiftUI
import Testing
@testable import Cooked

@MainActor
struct IslandModelTests {
    @Test func startsCalmAndClosed() {
        let model = IslandModel()
        #expect(model.snapshot == DemoData.calm)
        #expect(!model.isOpen)
        #expect(model.rateNow == 9000)
    }

    @Test func toggleDemoSwitchesCalmAndPeak() {
        let model = IslandModel()
        model.toggleDemo()
        #expect(model.snapshot == DemoData.peak)
        #expect(model.rateNow == 184_000)
        model.toggleDemo()
        #expect(model.snapshot == DemoData.calm)
        #expect(model.rateNow == 9000)
    }

    @Test func openAndClose() {
        let model = IslandModel()
        model.open()
        #expect(model.isOpen)
        model.close()
        #expect(!model.isOpen)
    }

    @Test func calmRateCyclesEveryTick() {
        let model = IslandModel()
        var seen: [Int] = []
        for _ in 0..<5 { model.tickRate(); seen.append(model.rateNow) }
        #expect(seen == [11000, 8000, 10000, 9000, 11000])
    }

    @Test func peakRateStaysOnTick() {
        let model = IslandModel()
        model.toggleDemo()
        model.tickRate()
        #expect(model.rateNow == 184_000)
    }

    @Test func panelSizeHoldsOpenPeak() {
        let size = IslandLayout.panelSize
        #expect(size.width == CGFloat(404 + 2 * 24))
        // the full list of 8 sources open and 5 accounts need about 1240 pt: more than a 14" MacBook's screen, so the
        // panel stops at the screen under the menu bar and the popover's lower part scrolls
        #expect(size.height >= 900 && size.height <= max(IslandLayout.maxPanelHeight, 945), "panel height \(size.height)")
    }
}

struct IslandHitTestTests {
    // panel 452 × 600 with its bottom-left corner at (1000, 800) on screen; rects are top-left based
    let panel = CGRect(x: 1000, y: 800, width: 452, height: 600)
    let island = CGRect(x: 106, y: 24, width: 240, height: 44)

    @Test func pointOnIslandHits() {
        // 30 pt below the panel top, 200 pt in
        #expect(IslandHitTest.hits(CGPoint(x: 1200, y: 1370), rects: [island], panelFrame: panel))
    }

    @Test func transparentMarginPassesThrough() {
        #expect(!IslandHitTest.hits(CGPoint(x: 1010, y: 1370), rects: [island], panelFrame: panel))
        #expect(!IslandHitTest.hits(CGPoint(x: 1200, y: 1000), rects: [island], panelFrame: panel))
    }

    @Test func outsideThePanelPassesThrough() {
        #expect(!IslandHitTest.hits(CGPoint(x: 10, y: 10), rects: [island], panelFrame: panel))
    }
}

/// The panel is measured with the full list of sources open and every connected account, so nothing is cut.
@MainActor
struct PanelSizeTests {
    @Test func tallestHasTheFullListAndEveryAccount() {
        let s = IslandLayout.tallest(accounts: 7)
        #expect(s.sources.count == TokenSources.limit)
        #expect(s.rows.count == 7)
        #expect(IslandLayout.tallest(accounts: 2).rows.count == DemoData.peak.rows.count)
    }

    @Test func theOpenListMakesThePanelTaller() {
        func height(_ expanded: Bool, accounts: Int) -> CGFloat {
            let model = IslandModel(snapshot: IslandLayout.tallest(accounts: accounts))
            model.isOpen = true
            model.sourcesExpanded = expanded
            return NSHostingView(rootView: AnyView(IslandRoot(model: model, forceStill: true).environment(\.solidSurfaces, true))).fittingSize.height
        }
        #expect(height(true, accounts: 5) > height(false, accounts: 5) + 100)
        #expect(height(true, accounts: 7) > height(true, accounts: 5))
    }
}
