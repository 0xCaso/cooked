import Foundation

/// The mockup's demo data (src/lib.js): `calm` is a real status.json of 28/10 19:28, `peak` (23:05) is invented.
/// The week % of every account but work is invented.
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
        clock: "19:28",
        accounts: [
            Account(id: "work", short: "work", fiveHourPercent: 4, fiveHourReset: "00:20", isLive: true, weekPercent: 74,
                    fiveHourElapsed: 0.38, weekElapsed: 0.71, weekReset: "Mon"),
            Account(id: "team", short: "team", fiveHourPercent: 11, fiveHourReset: "04:40", isLive: false, weekPercent: 22,
                    fiveHourElapsed: 0.04, weekElapsed: 0.3, weekReset: "Wed"),
            Account(id: "per", short: "personal", fiveHourPercent: 0, fiveHourReset: "—", isLive: false, isUnused: true),
            Account(id: "side", short: "side", fiveHourPercent: 12, fiveHourReset: "01:55", isLive: false, weekPercent: 41,
                    fiveHourElapsed: 0.11, weekElapsed: 0.5, weekReset: "Tue"),
            Account(id: "cdx", short: "codex", fiveHourPercent: 6, fiveHourReset: "03:10", isLive: false, weekPercent: 9,
                    fiveHourElapsed: 0.2, weekElapsed: 0.14, weekReset: "Sun"),
        ],
        activeID: "work", rateNow: 9000, threshold: threshold,
        series: series(count: 60, peak: false),
        urgency: nil)

    static let peak = UsageSnapshot(
        clock: "23:05",
        accounts: [
            Account(id: "work", short: "work", fiveHourPercent: 82, fiveHourReset: "00:20", isLive: true, fullAt: "23:14", weekPercent: 79,
                    fiveHourElapsed: 0.55, weekElapsed: 0.71, weekReset: "Mon"),
            Account(id: "team", short: "team", fiveHourPercent: 12, fiveHourReset: "04:40", isLive: false, weekPercent: 23,
                    fiveHourElapsed: 0.07, weekElapsed: 0.3, weekReset: "Wed"),
            Account(id: "per", short: "personal", fiveHourPercent: 31, fiveHourReset: "02:40", isLive: true, fullAt: "01:50",
                    weekPercent: 64, fiveHourElapsed: 0.15, weekElapsed: 0.42, weekReset: "Thu"),
            Account(id: "side", short: "side", fiveHourPercent: 12, fiveHourReset: "01:55", isLive: false, weekPercent: 41,
                    fiveHourElapsed: 0.11, weekElapsed: 0.5, weekReset: "Tue"),
            Account(id: "cdx", short: "codex", fiveHourPercent: 9, fiveHourReset: "03:10", isLive: false, weekPercent: 10,
                    fiveHourElapsed: 0.2, weekElapsed: 0.14, weekReset: "Sun"),
        ],
        activeID: "work", rateNow: 184_000, threshold: threshold,
        series: series(count: 60, peak: true),
        urgency: Urgency(
            percent: 82, fullAt: "23:14", detail: "claude-work in 9 min",
            source: "184k/min from “Refactor usage page”"),
        sources: [
            TokenSource(id: "t1", threadID: "t1", project: "cooked", detail: "Refactor usage page", account: "work", rate: 113_000,
                        agents: "1 agent + 3 sub"),
            TokenSource(id: "t2", project: "garden-notes", detail: "Draft the release notes", account: "work", rate: 18_000, agents: "1 agent"),
            TokenSource(id: "t3", project: "t3 scratch", detail: "Onboarding copy pass", account: "personal", rate: 4000, agents: nil),
        ])
}
