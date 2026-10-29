import Foundation

/// The mockup's demo data (src/lib.js): `calm` is a real status.json of 28/10 19:28, `peak` (23:05) is invented.
enum DemoData {
    static let threshold = 40000
    static let calmRates = [9000, 11000, 8000, 10000]

    /// Tokens per minute, the same seeded series as the mockup, so the charts match its screenshots.
    static func series(count n: Int, peak: Bool) -> [Double] {
        var seed: Int64 = 7
        func rnd() -> Double {
            seed = (seed * 16807) % 2_147_483_647
            return Double(seed) / 2_147_483_647
        }
        return (0..<n).map { i in
            let k = Double(i) / Double(n - 1)
            var v = 6000 + rnd() * 9000 + sin(Double(i) / 3) * 2500
            if peak {
                v = 12000 + rnd() * 14000 + pow(k, 6) * 150_000
                v += i > n - 9 ? 40000 + rnd() * 50000 : 0
            } else if Int((k * 59).rounded()) == 41 {
                v = 47000
            }
            if rnd() < 0.3 { v += rnd() * 5000 }
            return v
        }
    }

    static let calm = UsageSnapshot(
        clock: "19:28", historyStart: "18:28",
        accounts: [
            Account(id: "work", short: "work", fiveHourPercent: 4, fiveHourReset: "00:20", isLive: true),
            Account(id: "per", short: "personal", fiveHourPercent: 0, fiveHourReset: "—", isLive: false),
            Account(id: "side", short: "side", fiveHourPercent: 12, fiveHourReset: "01:55", isLive: false),
            Account(id: "cdx", short: "codex", fiveHourPercent: 6, fiveHourReset: "03:10", isLive: false),
        ],
        activeID: "work", rateNow: 9000, threshold: threshold,
        series: series(count: 60, peak: false),
        week: WeekLine(short: "work", percent: 74, reset: "Thu 00:00"),
        urgency: nil)

    static let peak = UsageSnapshot(
        clock: "23:05", historyStart: "22:05",
        accounts: [
            Account(id: "work", short: "work", fiveHourPercent: 82, fiveHourReset: "00:20", isLive: true),
            Account(id: "per", short: "personal", fiveHourPercent: 31, fiveHourReset: "02:40", isLive: true),
            Account(id: "side", short: "side", fiveHourPercent: 12, fiveHourReset: "01:55", isLive: false),
            Account(id: "cdx", short: "codex", fiveHourPercent: 9, fiveHourReset: "03:10", isLive: false),
        ],
        activeID: "work", rateNow: 184_000, threshold: threshold,
        series: series(count: 60, peak: true),
        week: WeekLine(short: "work", percent: 79, reset: "Thu 00:00"),
        urgency: Urgency(
            percent: 82, fullAt: "23:14", detail: "claude-work · in 9 min",
            source: "184k/min from “Refactor usage page”", switchLabel: "Switch to side · 12%"))
}
