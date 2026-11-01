import Foundation
import Testing
@testable import Cooked

/// One data path: the session files, the bridge files and the minute cache; T3 Code's logs only for a Claude percent
/// with no bridge file yet.
struct OneDataPathTests {
    let now = Date()

    func tempDir(_ name: String) throws -> String {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("\(name)-\(UUID().uuidString)").path
        try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        return dir
    }

    /// The JSON Claude Code gives its status line, with the limits.
    func statusLine(five: Double, fiveReset: Date, week: Double, weekReset: Date) -> String {
        #"{"session_id":"s","transcript_path":"/h/.claude-work/projects/p/s.jsonl","rate_limits":{"five_hour":{"used_percentage":\#(five),"resets_at":\#(Int(fiveReset.timeIntervalSince1970))},"seven_day":{"used_percentage":\#(week),"resets_at":\#(Int(weekReset.timeIntervalSince1970))}}}"#
    }

    @Test func readsTheBridgeFiles() throws {
        let dir = try tempDir("bridge")
        let reset = now.addingTimeInterval(3600), weekReset = now.addingTimeInterval(86400)
        try statusLine(five: 42, fiveReset: reset, week: 12, weekReset: weekReset).write(toFile: dir + "/claudeAgent_work.json", atomically: true, encoding: .utf8)
        // an API key user: no rate_limits, no percent
        try #"{"session_id":"s"}"#.write(toFile: dir + "/claudeAgent.json", atomically: true, encoding: .utf8)
        let s = StatusLineBridge.samples(dir: dir)
        #expect(Array(s.keys) == ["claudeAgent_work"])
        let work = try #require(s["claudeAgent_work"])
        #expect(work.five == 0.42 && work.week == 0.12)
        #expect(work.fiveReset == Double(Int(reset.timeIntervalSince1970)))
        #expect(abs(work.at - now.timeIntervalSince1970) < 5) // the file's change time
    }

    @Test func theBridgeWinsOverTheT3CodeLogs() throws {
        let reset = now.addingTimeInterval(3600)
        let logs = try LimitLogTests.logDir([
            LimitLogTests.claudeLine(at: now.addingTimeInterval(-60), account: "claudeAgent_work", five: 0.30, fiveReset: reset),
            LimitLogTests.claudeLine(at: now.addingTimeInterval(-60), account: "claudeAgent", five: 0.70, fiveReset: reset),
            LimitLogTests.codexLine(at: now.addingTimeInterval(-60), used: 55, reset: reset),
        ])
        let bridge = try tempDir("bridge")
        try statusLine(five: 42, fiveReset: reset, week: 12, weekReset: now.addingTimeInterval(86400))
            .write(toFile: bridge + "/claudeAgent_work.json", atomically: true, encoding: .utf8)
        var s = source(logs)
        s.bridgeDir = bridge
        let accounts = try s.read(now: now).get().snapshot.accounts
        #expect(accounts.map(\.id) == ["claudeAgent_work", "claudeAgent"])
        #expect(accounts.map(\.fiveHourPercent) == [42, 70]) // work from the bridge, personal from the logs until it has one
        // Codex from T3 Code's logs is not read: its percent comes from the rollouts
    }

    @Test func codexPercentComesFromTheRollouts() throws {
        let home = try tempDir("codex")
        let day = home + "/sessions/2026/11/01"
        try FileManager.default.createDirectory(atPath: day, withIntermediateDirectories: true)
        let iso = ISO8601DateFormatter()
        iso.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        let at = iso.string(from: now.addingTimeInterval(-30))
        let reset = Int(now.timeIntervalSince1970) + 3600
        let line = #"{"timestamp":"\#(at)","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000,"cached_input_tokens":0,"output_tokens":10,"total_tokens":1010},"last_token_usage":{"input_tokens":1000,"cached_input_tokens":0,"output_tokens":10,"total_tokens":1010}},"rate_limits":{"primary":{"used_percent":12.0,"window_minutes":300,"resets_at":\#(reset)},"secondary":{"used_percent":40.0,"window_minutes":10080,"resets_at":\#(reset + 86400)}}}}"#
        try (line + "\n").write(toFile: day + "/rollout-2026-11-01T10-00-00-\(UUID().uuidString.lowercased()).jsonl", atomically: true, encoding: .utf8)
        var s = source(try LimitLogTests.logDir([]))
        s.codex = CodexLog(home: home, ledger: s.ledger)
        let snapshot = try s.read(now: now).get().snapshot
        #expect(snapshot.accounts.map(\.id) == ["codex"])
        #expect(snapshot.accounts[0].fiveHourPercent == 12 && snapshot.accounts[0].weekPercent == 40)
        #expect(snapshot.series.suffix(2).reduce(0, +) == 1010)
        // after a restart the rollout continues from its mark: the limits come from its last lines
        let restarted = CodexLog(home: home, ledger: s.ledger)
        #expect(restarted.read(now: now).limits?.fiveHour?.usedPercent == 12)
    }

    @Test func theAlertLinesComeFromTheCache() {
        let ledger = MinuteLedger()
        let minuteNow = Int(now.timeIntervalSince1970) / 60
        // 40 busy quarter hours for work, the busiest at 15 × 2000; personal adds 100 to each
        for q in 0..<40 {
            for m in 0..<15 {
                ledger.add("claudeAgent_work", minute: (minuteNow / 15 - 40 + q) * 15 + m, tokens: Double(q == 39 ? 2000 : 1000))
                ledger.add("claudeAgent", minute: (minuteNow / 15 - 40 + q) * 15 + m, tokens: 100)
            }
        }
        let b = ledger.buckets15()
        #expect(b.byAccount["claudeAgent_work"]?.count == 40)
        #expect(b.total.max() == Double(15 * 2100))
        let log = LimitLog(dir: nil)
        let sample = LimitLog.Sample(at: now.timeIntervalSince1970, five: 0.5, fiveReset: now.timeIntervalSince1970 + 3600,
                                     week: 0.1, weekReset: nil, status: "allowed")
        let status = log.read(now: now, samples: ["claudeAgent_work": sample, "claudeAgent": sample], buckets: ledger.buckets15)
        #expect(status.accounts["claudeAgent_work"]?.spikeThreshold15m == 30_000)
        #expect(status.accounts["claudeAgent"]?.spikeThreshold15m == 1_500)
        #expect(status.totalThreshold15m == 31_500)
    }
}
