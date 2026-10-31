import Foundation
import Testing
@testable import Cooked

@Suite @MainActor struct ChartRangeTests {
    @Test func eachRangeShowsItsSpanOfMinutes() {
        // a day of minutes: each minute's value is its age, the newest last
        let day = (0..<1440).map { Double(1439 - $0) }
        let history = MinuteHistory(values: day, top: 2000, threshold: 100)
        for range in ChartRange.allCases {
            let w = history.window(range.minutes)
            #expect(w.values.count == range.minutes)
            #expect(w.values.last == 0) // the minute in progress
            #expect(w.values.first == Double(range.minutes - 1)) // the oldest minute of the range
        }
        #expect(ChartRange.allCases.map(\.rawValue) == ["10m", "30m", "1h", "4h", "1d"])
    }

    @Test func aRangeLongerThanTheDataStartsEmpty() {
        let hour = MinuteHistory(values: Array(repeating: 5000, count: 60), top: 8000, threshold: 1000)
        let w = hour.window(240)
        #expect(w.values.prefix(180).allSatisfy { $0 == 0 })
        #expect(w.values.suffix(60).allSatisfy { $0 == 5000 })
    }

    @Test func theScaleFitsTheRangeAlone() {
        var values = Array(repeating: 1000.0, count: 1440)
        values[100] = 90_000 // a spike 22 hours ago
        let history = MinuteHistory(values: values, top: 90_000, threshold: 1000)
        #expect(history.window(1440).top == 90_000)
        #expect(history.window(10).top == 1600) // 1.6 × the threshold: the old spike does not flatten the last 10 minutes
    }

    @Test func tenMinutesBinsEachMinuteOverItsColumns() {
        var minutes = Array(repeating: 0.0, count: 10)
        minutes[5] = 64000
        let t = TideScale.targets(minutes, cols: 84, rows: 24, top: 64000)
        // 8.4 columns per minute: minute 5 covers columns 42-50, partly 50
        #expect(abs(t[43] - 24) < 1e-9 && abs(t[49] - 24) < 1e-9)
        #expect(t[41] == 0.4 && t[51] == 0.4)
    }

    @Test func theRangeIsKeptAcrossLaunches() {
        let defaults = UserDefaults(suiteName: "ChartRangeTests")!
        defaults.removePersistentDomain(forName: "ChartRangeTests")
        let first = IslandModel(defaults: defaults)
        #expect(first.chartRange == .tenMinutes)
        first.chartRange = .fourHours
        #expect(IslandModel(defaults: defaults).chartRange == .fourHours)
        defaults.removePersistentDomain(forName: "ChartRangeTests")
    }
}
