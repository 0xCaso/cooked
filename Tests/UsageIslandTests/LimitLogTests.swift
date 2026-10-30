import Foundation
import Testing
@testable import UsageIsland

/// The limits read from T3 Code's provider logs, in place of the watcher script.
struct LimitLogTests {
    nonisolated(unsafe) static let iso: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()

    /// A Claude `rate_limit_event` line as T3 Code logs it (30/10 11:59).
    static func claudeLine(at: Date, account: String?, five: Double, fiveReset: Date, week: Double = 0.38,
                           weekReset: Date = Date(timeIntervalSince1970: 1_793_714_400), status: String = "allowed") -> String {
        let session = account.map { "provider-session:provider-instance:\($0):thread:t1:x" } ?? "provider-session:x"
        return """
            [\(iso.string(from: at))] NTIVE: {"provider":"claudeAgent","providerSessionId":"\(session)","event":{"direction":"incoming","stage":"decoded","payload":{"type":"rate_limit_event","rate_limit_info":{"status":"\(status)","unifiedWindows":{"five_hour":{"utilization":\(five),"resetsAt":\(Int(fiveReset.timeIntervalSince1970)),"status":"allowed"},"seven_day":{"utilization":\(week),"resetsAt":\(Int(weekReset.timeIntervalSince1970)),"status":"allowed"}}}}}}
            """
    }

    /// A Codex `account/rateLimits/updated` line.
    static func codexLine(at: Date, used: Double, reset: Date, hit: Bool = false) -> String {
        """
        [\(iso.string(from: at))] NTIVE: {"provider":"codex","providerSessionId":"provider-session:provider-instance:codex:thread:t2:y","event":{"payload":{"method":"account/rateLimits/updated","params":{"rateLimits":{"primary":{"usedPercent":\(used),"windowDurationMins":300,"resetsAt":\(Int(reset.timeIntervalSince1970))},"secondary":{"usedPercent":12,"windowDurationMins":10080,"resetsAt":\(Int(reset.timeIntervalSince1970) + 86400)},"rateLimitReachedType":\(hit ? #""rate_limit_reached""# : "null")}}}}}
        """
    }

    static func logDir(_ lines: [String], thread: String = "t1") throws -> String {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("limits-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try Data((lines.map { $0 + "\n" }.joined()).utf8).write(to: dir.appendingPathComponent("events.\(thread).log"))
        return dir.path
    }

    let calendar: Calendar = {
        var c = Calendar(identifier: .gregorian)
        c.timeZone = TimeZone(identifier: "Europe/Rome")!
        return c
    }()
    var now: Date { calendar.date(from: DateComponents(year: 2026, month: 10, day: 30, hour: 14, minute: 0))! }

    func sample(_ minutesAgo: Double, _ five: Double, reset: Double = 3600) -> LimitLog.Sample {
        let t = now.timeIntervalSince1970
        return .init(at: t - minutesAgo * 60, five: five, fiveReset: t + reset, week: 0.5, weekReset: t + 86400, status: "allowed")
    }

    @Test func parsesClaudeAndCodexEvents() throws {
        let reset = now.addingTimeInterval(3600)
        let (account, s) = try #require(LimitLog.parse(Data(Self.claudeLine(at: now, account: "claudeAgent_team", five: 0.69, fiveReset: reset).utf8)))
        #expect(account == "claudeAgent_team")
        #expect(s == .init(at: now.timeIntervalSince1970, five: 0.69, fiveReset: reset.timeIntervalSince1970, week: 0.38,
                           weekReset: 1_793_714_400, status: "allowed"))
        let (codex, c) = try #require(LimitLog.parse(Data(Self.codexLine(at: now, used: 42, reset: reset, hit: true).utf8)))
        #expect(codex == "codex")
        #expect(c.five == 0.42)
        #expect(c.week == 0.12)
        #expect(c.status == "rejected")
        // a tool result that only quotes the event name is not an event
        let quoted = #"[2026-10-30T12:36:46.727Z] NTIVE: {"event":{"payload":{"type":"user","content":"\"rate_limit_event\""}}}"#
        #expect(LimitLog.parse(Data(quoted.utf8)) == nil)
        #expect(LimitLog.instance(in: "provider-session:x") == nil)
    }

    @Test func readsOnlyNewCompleteLines() throws {
        let reset = now.addingTimeInterval(3600)
        let dir = try Self.logDir([Self.claudeLine(at: now.addingTimeInterval(-120), account: "claudeAgent_work", five: 0.30, fiveReset: reset)])
        let log = LimitLog(dir: dir)
        let path = dir + "/events.t1.log"
        try FileManager.default.setAttributes([.modificationDate: now], ofItemAtPath: path)
        #expect(log.read(now: now, database: nil, calendar: calendar).accounts["claudeAgent_work"]?.fiveHourPct == 30)
        // a new event and half a line: the event counts, the half line waits
        let handle = try #require(FileHandle(forWritingAtPath: path))
        try handle.seekToEnd()
        let next = Self.claudeLine(at: now.addingTimeInterval(-60), account: "claudeAgent_work", five: 0.35, fiveReset: reset)
        handle.write(Data((next + "\n" + next.prefix(40)).utf8))
        try handle.close()
        try FileManager.default.setAttributes([.modificationDate: now], ofItemAtPath: path)
        let work = try #require(log.read(now: now, database: nil, calendar: calendar).accounts["claudeAgent_work"])
        #expect(work.fiveHourPct == 35)
        #expect(work.fiveHourResetsAt == "15:00")
        #expect(work.lastEventAt == "13:59")
        #expect(work.live == true)
        #expect(work.spikeThreshold15m == 600_000)
    }

    @Test func oldFilesAreSkipped() throws {
        let dir = try Self.logDir([Self.claudeLine(at: now.addingTimeInterval(-60), account: "claudeAgent", five: 0.3,
                                                   fiveReset: now.addingTimeInterval(3600))])
        try FileManager.default.setAttributes([.modificationDate: now.addingTimeInterval(-7 * 3600)], ofItemAtPath: dir + "/events.t1.log")
        #expect(LimitLog(dir: dir).read(now: now, database: nil, calendar: calendar).accounts.isEmpty)
    }

    /// Only the current 5 h window and the last hour.
    @Test func pruneKeepsTheCurrentWindow() {
        let old = sample(30, 0.9, reset: -60)
        let s = LimitLog.prune([sample(5, 0.5), old, sample(90, 0.1), sample(5, 0.5), sample(1, 0.6)])
        #expect(s == [sample(5, 0.5), sample(1, 0.6)])
    }

    /// 8 minutes of samples are no "30 min" pace: on 30/10 the script saw work full at 13:57 from them.
    @Test func paceNeedsMostOfItsWindow() {
        let eight = [sample(9, 0.30), sample(5, 0.33), sample(1, 0.36)]
        #expect(LimitLog.slope(eight, window: 1800, minimum: 2) == nil)
        #expect(LimitLog.slope(eight, window: 300, minimum: 3) == nil) // only 2 samples in the last 5 min
        let long = [sample(25, 0.20), sample(1, 0.36)]
        let pace = try? #require(LimitLog.slope(long, window: 1800, minimum: 2))
        #expect(abs((pace ?? 0) - 0.16 / (24 * 60)) < 1e-9)
    }

    @Test func projectionBeforeTheResetOnly() {
        // 0.16 in 24 min: 100% in 105 min, after the reset in 60 min
        let late = LimitLog.entry(id: "claudeAgent_work", samples: [sample(25, 0.20), sample(1, 0.36)], threshold: nil, now: now, calendar: calendar)
        #expect(late.projection == nil)
        let soon = LimitLog.entry(id: "claudeAgent_work", samples: [sample(25, 0.20), sample(1, 0.80)], threshold: nil, now: now, calendar: calendar)
        // 0.6 in 24 min: the last 0.2 in 8 min, at 13:59 + 8
        #expect(soon.projection == .init(window: "30 min", eta: "14:07"))
        let low = LimitLog.entry(id: "claudeAgent_work", samples: [sample(25, 0.01), sample(1, 0.19)], threshold: nil, now: now, calendar: calendar)
        #expect(low.projection == nil) // under 20%
        let stale = LimitLog.entry(id: "claudeAgent_work", samples: [sample(1, 0.5, reset: -30)], threshold: nil, now: now, calendar: calendar)
        #expect(stale.note != nil)
        let idle = LimitLog.entry(id: "claudeAgent_work", samples: [sample(25, 0.2), sample(11, 0.8)], threshold: nil, now: now, calendar: calendar)
        #expect(idle.live == false)
        #expect(idle.projection == nil)
    }

    /// team had 10 buckets on 30/10 and sat at the fixed 600k: it takes the other Claude accounts' p99.
    @Test func thresholdWithLittleHistory() {
        let work = (1...100).map(Double.init) // p99 = 100
        let t = LimitLog.thresholds(buckets: ["claudeAgent_work": work, "claudeAgent_team": Array(repeating: 5, count: 10),
                                              "codex": [1, 2]])
        #expect(t["claudeAgent_work"] == 100)
        #expect(t["claudeAgent_team"] == 100)
        #expect(t["claudeAgent"] == 100)
        #expect(t["codex"] == 350_000)
    }

    /// The popover's chart sums every account: its line is the p99 of the summed buckets.
    @Test func totalThresholdOnTheSum() {
        #expect(LimitLog.totalThreshold(buckets: (1...200).map { Double($0 * 10) }, fallback: 7) == 1990)
        #expect(LimitLog.totalThreshold(buckets: [1, 2, 3], fallback: 7) == 7)
    }

    @Test func clockOnOtherDays() {
        let tomorrow = calendar.date(from: DateComponents(year: 2026, month: 10, day: 31, hour: 9, minute: 5))!
        #expect(LimitLog.clock(tomorrow.timeIntervalSince1970, now, calendar) == "31/10 09:05")
        #expect(LimitLog.clock(now.timeIntervalSince1970, now, calendar) == "14:00")
    }
}
