import Testing
@testable import UsageIsland

struct DemoDataTests {
    @Test func shortFormat() {
        #expect(TokenFormat.short(950) == "950")
        #expect(TokenFormat.short(9000) == "9k")
        #expect(TokenFormat.short(40000) == "40k")
        #expect(TokenFormat.short(184_000) == "184k")
        #expect(TokenFormat.short(1_200_000) == "1.2M")
    }

    @Test func calmSnapshot() {
        let s = DemoData.calm
        #expect(s.clock == "19:28")
        #expect(s.historyStart == "18:28")
        #expect(s.accounts.map(\.short) == ["work", "team", "personal", "side", "codex"])
        #expect(s.accounts.map(\.fiveHourPercent) == [4, 11, 0, 12, 6])
        #expect(s.accounts.map(\.fiveHourReset) == ["00:20", "04:40", "—", "01:55", "03:10"])
        #expect(s.accounts.map(\.isLive) == [true, false, false, false, false])
        #expect(s.activeID == "work")
        #expect(s.rateNow == 9000)
        #expect(s.threshold == 40000)
        #expect(s.series.count == 60)
        #expect(s.urgency == nil)
        #expect(!s.isUrgent)
        #expect(s.week == WeekLine(short: "work", percent: 74, reset: "Thu 00:00"))
    }

    @Test func peakSnapshot() {
        let s = DemoData.peak
        #expect(s.clock == "23:05")
        #expect(s.historyStart == "22:05")
        #expect(s.accounts.map(\.fiveHourPercent) == [82, 12, 31, 12, 9])
        #expect(s.accounts.map(\.fiveHourReset) == ["00:20", "04:40", "02:40", "01:55", "03:10"])
        #expect(s.accounts.map(\.isLive) == [true, false, true, false, false])
        #expect(s.rateNow == 184_000)
        #expect(s.week.percent == 79)
        #expect(s.urgency == Urgency(
            percent: 82, fullAt: "23:14", detail: "claude-work · in 9 min",
            source: "184k/min from “Refactor usage page”", switchLabel: "Switch to side · 12%"))
        #expect(s.isUrgent)
    }

    @Test func seriesIsDeterministic() {
        #expect(DemoData.series(count: 60, peak: false) == DemoData.series(count: 60, peak: false))
        let peak = DemoData.series(count: 60, peak: true)
        #expect(peak.suffix(8).allSatisfy { $0 > 40000 })
        let calm = DemoData.series(count: 60, peak: false)
        #expect(calm[41] == 47000 || (47000...52000).contains(calm[41]))
        #expect(calm.enumerated().filter { $0.offset != 41 }.allSatisfy { $0.element < 40000 })
    }

    @Test func calmRatesCycle() {
        #expect(DemoData.calmRates == [9000, 11000, 8000, 10000])
    }
}
