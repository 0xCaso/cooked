import Foundation
import Testing
@testable import UsageIsland

@Suite @MainActor struct PaceTests {
    @Test func elapsedIsTheShareOfTheWindowGone() {
        let now = Date(timeIntervalSince1970: 1_000_000)
        #expect(LiveSnapshot.elapsed(until: now.addingTimeInterval(3600), window: 5 * 3600, now: now) == 0.8)
        #expect(LiveSnapshot.elapsed(until: now.addingTimeInterval(-60), window: 5 * 3600, now: now) == 1)
        #expect(LiveSnapshot.elapsed(until: now.addingTimeInterval(9 * 3600), window: 5 * 3600, now: now) == 0)
    }

    @Test func weekdayIsShortEnglish() {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "Europe/Rome")!
        let monday = calendar.date(from: DateComponents(year: 2026, month: 11, day: 2, hour: 9))!
        #expect(LiveSnapshot.weekday(monday, calendar) == "Mon")
    }

    @Test func stripLightsTheUsedShareAndPutsTheLineAtEvenPace() {
        // team on 30/10: 26% used, 17% of the window gone, 15 columns
        let l = PaceStrip.layout(percent: 26, elapsed: 0.17, cols: 15)
        #expect(l.lit == 4)
        #expect(l.line == 2)
        #expect(abs(l.ahead - 2.55) < 0.001)
        #expect(PaceStrip.layout(percent: 140, elapsed: nil, cols: 15).lit == 15)
        #expect(PaceStrip.layout(percent: 140, elapsed: nil, cols: 15).line == nil)
        #expect(PaceStrip.columns(width: 46) == 15)
    }

    @Test func peakOpensTheTopThreadOfTheAccountThatFills() {
        var s = DemoData.peak
        s.sources = [TokenSource(id: "a", threadID: "ta", project: "p", detail: "d", account: "personal", rate: 9, agents: nil),
                     TokenSource(id: "b", threadID: "tb", project: "p", detail: "d", account: "work", rate: 5, agents: nil)]
        #expect(IslandView.peakThread(s) == "tb")
        s.sources.removeLast()
        #expect(IslandView.peakThread(s) == "ta")
        s.sources = []
        #expect(IslandView.peakThread(s) == nil)
    }
}
