import Foundation
import Testing
@testable import Cooked

struct HistogramClockTests {
    @Test func currentMinuteGrows() {
        let clock = HistogramClock(values: Array(repeating: 10000, count: 60), threshold: 40000, minuteLength: 4) { 20000 }
        _ = clock.history(at: 0, still: false)
        _ = clock.history(at: 4, still: false) // minute closes: the last minute now fills towards 20000
        #expect(abs(clock.history(at: 6, still: false).values[59] - 10000) < 1e-6)
        #expect(abs(clock.history(at: 7, still: false).values[59] - 15000) < 1e-6)
    }

    @Test func closingMinuteMovesTheHistoryLeft() {
        let values = (0..<60).map { Double(1000 * ($0 + 1)) }
        let clock = HistogramClock(values: values, threshold: 40000, minuteLength: 4) { 0 }
        #expect(clock.history(at: 0, still: false).values[10] == 11000)
        let after = clock.history(at: 4, still: false)
        #expect(after.values[10] == 12000)
        #expect(after.values[58] == 60000) // the closed minute keeps its full total
        #expect(after.values.count == 60)
    }

    @Test func stillShowsFullMinutes() {
        let clock = HistogramClock(values: Array(repeating: 1000, count: 60), threshold: 40000, minuteLength: 4) { 30000 }
        _ = clock.history(at: 0, still: true)
        #expect(clock.history(at: 4.1, still: true).values[59] == 30000)
    }

    @Test func topNeverBelowTheThresholdMargin() {
        let clock = HistogramClock(values: [1000], threshold: 40000, minuteLength: 60) { 0 }
        #expect(clock.history(at: 0, still: true).top == 64000)
        let high = HistogramClock(values: [640_000], threshold: 40000, minuteLength: 60) { 0 }
        #expect(high.history(at: 0, still: true).top == 640_000)
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
}
