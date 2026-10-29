import Foundation
import Testing
@testable import UsageIsland

struct HistogramClockTests {
    // max = 1.6 × 40000 = 64000 → 18 rows
    func rows(_ v: Double) -> Double { v / 64000 * 18 }

    @Test func currentMinuteGrows() {
        let clock = HistogramClock(values: Array(repeating: 10000, count: 60), threshold: 40000, minuteLength: 4) { 20000 }
        _ = clock.heights(at: 0, still: false)
        _ = clock.heights(at: 4, still: false) // minute closes: the last column now fills towards 20000
        let half = clock.heights(at: 6, still: false)[59]
        let threeQuarters = clock.heights(at: 7, still: false)[59]
        #expect(abs(half - rows(10000)) < rows(10000) * 0.02)
        #expect(abs(threeQuarters - rows(15000)) < rows(15000) * 0.02)
    }

    @Test func shiftEasesOnSpring() {
        let values = (0..<60).map { Double(1000 * ($0 + 1)) }
        let clock = HistogramClock(values: values, threshold: 40000, minuteLength: 4) { 0 }
        _ = clock.heights(at: 0, still: false)
        let atShift = clock.heights(at: 4, still: false)[10]
        #expect(abs(atShift - rows(11000)) < 1e-9)
        let settled = clock.heights(at: 4.5, still: false)[10]
        #expect(abs(settled - rows(12000)) < rows(12000) * 0.02)
    }

    @Test func stillShowsFinalValues() {
        let values = (0..<60).map { Double(1000 * ($0 + 1)) }
        let clock = HistogramClock(values: values, threshold: 40000, minuteLength: 4) { 30000 }
        _ = clock.heights(at: 0, still: true)
        let h = clock.heights(at: 4.1, still: true)
        #expect(h[10] == rows(12000))
        #expect(h[59] == rows(30000))
    }

    @Test func thresholdRow() {
        let clock = HistogramClock(values: [1000], threshold: 40000, minuteLength: 60) { 0 }
        #expect(clock.thresholdRow == 18 - Int((40000.0 / 64000 * 18).rounded()))
    }

    @Test func histogramClampsExtremes() {
        var values = Array(repeating: 0.0, count: 60)
        values[59] = 0
        values[30] = 640_000 // 10× the 1.6 × threshold scale, but max follows the data
        let clock = HistogramClock(values: values, threshold: 40000, minuteLength: 60) { 0 }
        let g = clock.grid(at: 0, still: true, urgent: false)
        #expect(g.cols == 60 && g.rows == 18)
        #expect(g.level[0 * 18 + 17] > 0)
        #expect(g.level[0 * 18 + 16] == 0)
        #expect(g.level[30 * 18 + 0] > 0)
        #expect(clock.heights(at: 0, still: true).allSatisfy { $0 >= 1 && $0 <= 18 })
    }

    @Test func redOnlyAboveThresholdWhenUrgent() {
        let clock = HistogramClock(values: Array(repeating: 64000, count: 60), threshold: 40000, minuteLength: 60) { 0 }
        let urgent = clock.grid(at: 0, still: true, urgent: true)
        #expect(urgent.hot[0 * 18 + 0])
        #expect(!urgent.hot[0 * 18 + 17])
        #expect(!clock.grid(at: 0, still: true, urgent: false).hot.contains(true))
    }

    @Test func accountBarLitCount() {
        func lit(_ p: Int) -> Int { AccountBar.grid(percent: p, hot: false).level.filter { $0 == 1 }.count }
        #expect(lit(0) == 0)
        #expect(lit(4) == 1)
        #expect(lit(82) == 16)
        #expect(lit(100) == 20)
        #expect(lit(140) == 20)
        #expect(AccountBar.grid(percent: 50, hot: true).hot.allSatisfy { $0 })
    }

    @Test func springProgressShape() {
        #expect(springProgress(0) == 0)
        #expect(abs(springProgress(1) - (1 - 7 * exp(-6.0))) < 1e-12)
    }
}
