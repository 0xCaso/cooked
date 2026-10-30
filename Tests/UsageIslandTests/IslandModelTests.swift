import CoreGraphics
import Testing
@testable import UsageIsland

@MainActor
struct IslandModelTests {
    @Test func startsCalmAndClosed() {
        let model = IslandModel()
        #expect(model.snapshot == DemoData.calm)
        #expect(!model.isOpen)
        #expect(model.rateNow == 9000)
    }

    @Test func peakCanBeMinimizedUntilItEnds() {
        let model = IslandModel(snapshot: DemoData.peak)
        #expect(model.showsPeakCard)
        model.peakMinimized = true
        #expect(!model.showsPeakCard)
        // the peak ends: the next peak shows the card again
        model.toggleDemo()
        #expect(!model.peakMinimized)
        model.toggleDemo()
        #expect(model.showsPeakCard)
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
        // island 108 + gap 8 + popover with 3 sources, 5 rows of pills (38 pt pitch, as in the mockup) + margins 48
        #expect((CGFloat(820)...CGFloat(890)).contains(size.height), "panel height \(size.height)")
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
