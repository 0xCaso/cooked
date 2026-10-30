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

    @Test func stripLightsTheShareLeftAndPutsTheLineAtTimeLeft() {
        // team on 30/10 at 13:08: 26% left, 27% of the window to go, 15 columns
        let l = PaceStrip.layout(left: 26, timeLeft: 0.27, cols: 15)
        #expect(abs(l.fill - 3.9) < 0.001)
        #expect(l.line == 4)
        #expect(PaceStrip.layout(left: 140, timeLeft: nil, cols: 15).fill == 15)
        #expect(PaceStrip.layout(left: 140, timeLeft: nil, cols: 15).line == nil)
        #expect(PaceStrip.columns(width: 46) == 15)
        #expect(PopoverView.left(74) == 26)
        #expect(PopoverView.left(140) == 0)
    }

    /// Usage times: the dark dots before the line are the overspend, red only on a hot strip.
    @Test func overspendIsTheDarkDotsBeforeTheLine() {
        // 10% left (1.5 columns), half the window to go: line at column 7
        let l = PaceStrip.layout(left: 10, timeLeft: 0.5, cols: 15)
        #expect(l.line == 7)
        #expect(PaceStrip.overspend(column: 3, cover: 0, line: l.line, hot: true))
        #expect(!PaceStrip.overspend(column: 3, cover: 0, line: l.line, hot: false))
        #expect(!PaceStrip.overspend(column: 0, cover: 1, line: l.line, hot: true)) // lit
        #expect(!PaceStrip.overspend(column: 9, cover: 0, line: l.line, hot: true)) // past the line
    }

    /// Dot motion task 3: each row's front sways on its spring, within ±0.7 column, and settles.
    @Test func frontsSwayWithinBoundsAndKeepMoving() {
        let fronts = PaceFronts()
        var biggest = 0.0, moving = 0
        var rows = Set<Int>()
        for frame in 0...(60 * 60) {
            let sway = fronts.sway(at: Double(frame) / 60, still: false)
            #expect(sway.count == 3)
            biggest = max(biggest, sway.map(abs).max()!)
            if sway.contains(where: { abs($0) > 0.05 }) { moving += 1 }
            for r in sway.indices where abs(sway[r]) > 0.05 { rows.insert(r) }
        }
        print("pill fronts: biggest sway \(biggest) columns, moving in \(moving) of 3601 frames")
        #expect(biggest <= PaceFronts.maxSway)
        #expect(biggest > 0.2)
        #expect(moving > 1000) // slow kicks: the fronts rest a while between them
        #expect(rows.count == 3) // every row has its own front
    }

    @Test func stillFrontsDoNotSway() {
        let fronts = PaceFronts()
        for frame in 0...600 { _ = fronts.sway(at: Double(frame) / 60, still: false) }
        #expect(fronts.sway(at: 10.1, still: true) == [0, 0, 0])
    }

    @Test func frontDotIsPartlyLitAndTheDotsBehindMoveLess() {
        // 3.9 columns lit: columns 0-2 full, column 3 at 90%, column 4 dark
        #expect(PaceStrip.cover(column: 2, fill: 3.9, sway: 0) == 1)
        #expect(abs(PaceStrip.cover(column: 3, fill: 3.9, sway: 0) - 0.9) < 1e-9)
        #expect(PaceStrip.cover(column: 4, fill: 3.9, sway: 0) == 0)
        #expect(abs(PaceStrip.cover(column: 4, fill: 3.9, sway: 0.5) - 0.4) < 1e-9)
        // an empty strip does not sway into its first dot
        #expect(PaceStrip.cover(column: 0, fill: 0, sway: 0.7) == 0)
        let front = PaceStrip.shift(column: 3, fill: 3.9, sway: 0.7)
        let back = PaceStrip.shift(column: 0, fill: 3.9, sway: 0.7)
        #expect(abs(front - PaceStrip.maxShift) < 1e-9)
        #expect(back > 0 && back < front)
        #expect(PaceStrip.shift(column: 3, fill: 3.9, sway: 0) == 0)
        #expect(PaceStrip.shift(column: 6, fill: 3.9, sway: 0.7) == 0) // dark dots stay put
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
