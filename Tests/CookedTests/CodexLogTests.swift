import Foundation
import Testing
@testable import Cooked

struct CodexLogTests {
    let iso: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()

    /// A Codex home with the sample rollouts in `sessions/`, and the sample index.
    func makeHome(_ samples: [String]) throws -> String {
        let home = FileManager.default.temporaryDirectory.appendingPathComponent("codex-\(UUID().uuidString)").path
        let day = home + "/sessions/2026/11/01"
        try FileManager.default.createDirectory(atPath: day, withIntermediateDirectories: true)
        for sample in samples {
            let id = UUID().uuidString.lowercased()
            try FileManager.default.copyItem(atPath: Fixture.url(sample).path, toPath: "\(day)/rollout-2026-11-01T10-00-00-\(id).jsonl")
        }
        return home
    }

    func counts(_ sample: String) throws -> [CodexLog.Count] { try Fixture.lines(sample).compactMap(CodexLog.parse) }

    @Test func sampleCallsCountInputMinusCachedPlusOutput() throws {
        let c = try counts("codex-rollout.jsonl")
        let first: Double = 16898 - 8960 + 294, second: Double = 17946 - 16128 + 209
        #expect(c.map(\.tokens) == [first, second, second])
    }

    @Test func sampleLimitsAreReadByWindow() throws {
        let c = try #require(counts("codex-rollout.jsonl").first)
        // a free plan: one 30-day window, neither 5 h nor week
        #expect(c.limits?.fiveHour == nil && c.limits?.week == nil)
        let plus = #"{"timestamp":"2026-10-31T01:03:19.553Z","type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{"primary":{"used_percent":12.0,"window_minutes":300,"resets_at":1793420421},"secondary":{"used_percent":40.0,"window_minutes":10080,"resets_at":1793805472}}}}"#
        let l = try #require(CodexLog.parse(Data(plus.utf8))?.limits)
        #expect(l.fiveHour == .init(usedPercent: 12, resetsAt: Date(timeIntervalSince1970: 1793420421)))
        #expect(l.week == .init(usedPercent: 40, resetsAt: Date(timeIntervalSince1970: 1793805472)))
    }

    @Test func importedRolloutAddsNoTokens() throws {
        #expect(try counts("codex-imported-rollout.jsonl").allSatisfy { $0.tokens == 0 && $0.limits == nil })
    }

    @Test func readsMinutesOnceEachAndSkipsTheRepeatedEvent() throws {
        let home = try makeHome(["codex-rollout.jsonl", "codex-imported-rollout.jsonl"])
        let log = CodexLog(home: home)
        let now = try #require(iso.date(from: "2026-08-20T18:52:00.000Z"))
        let m = log.read(now: now, minutes: 120 * 24 * 60).minutes // the samples are from August and November
        let minute = { (s: String) in Int(self.iso.date(from: s)!.timeIntervalSince1970) / 60 }
        // 18:51:04 and 18:51:09 are two calls; 18:51:15 repeats the 18:51:09 totals
        let both: Double = (16898 - 8960 + 294) + (17946 - 16128 + 209)
        #expect(m == [minute("2026-08-20T18:51:04.062Z"): both])
        #expect(log.read(now: now, minutes: 120 * 24 * 60).minutes == m) // a second reading adds nothing
    }

    @Test func sessionsAreNamedFromTheIndexElseTheFirstPrompt() throws {
        let home = try makeHome([])
        let day = home + "/sessions/2026/11/01"
        let id = "01a0c32f-514f-7102-a63f-ddcb17038712" // in the sample index
        let other = "01a0ffff-0000-7000-8000-000000000000"
        let at = iso.string(from: Date().addingTimeInterval(-60))
        func rollout(_ prompt: String) -> String {
            [#"{"timestamp":"\#(at)","type":"session_meta","payload":{"id":"x","cwd":"/r/demo"}}"#,
             #"{"timestamp":"\#(at)","type":"event_msg","payload":{"type":"user_message","message":"\#(prompt)"}}"#,
             #"{"timestamp":"\#(at)","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":1},"last_token_usage":{"input_tokens":100,"cached_input_tokens":40,"output_tokens":20}}}}"#]
                .joined(separator: "\n") + "\n"
        }
        try rollout("Fix CI").write(toFile: "\(day)/rollout-2026-11-01T10-00-00-\(id).jsonl", atomically: true, encoding: .utf8)
        try rollout("Plan the week").write(toFile: "\(day)/rollout-2026-11-01T11-00-00-\(other).jsonl", atomically: true, encoding: .utf8)
        try FileManager.default.copyItem(atPath: Fixture.url("codex-session-index.jsonl").path, toPath: home + "/session_index.jsonl")
        let sessions = CodexLog(home: home).read(now: Date()).sessions
        #expect(Set(sessions.map(\.id)) == [id, other])
        #expect(sessions.first { $0.id == id }?.name == "Demo Codex thread")
        #expect(sessions.first { $0.id == other }?.name == "Plan the week")
        #expect(sessions.allSatisfy { $0.account == "codex" && $0.cwd == "/r/demo" && $0.tokens == 80 && $0.mainActive })
    }

    @Test func newestLimitsWin() throws {
        let home = try makeHome([])
        let day = home + "/sessions/2026/11/01"
        func line(_ at: String, _ pct: Double) -> String {
            #"{"timestamp":"\#(at)","type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{"primary":{"used_percent":\#(pct),"window_minutes":300,"resets_at":1}}}}"# + "\n"
        }
        let now = Date()
        try line(iso.string(from: now.addingTimeInterval(-120)), 30).write(toFile: "\(day)/rollout-a-\(UUID().uuidString).jsonl", atomically: true, encoding: .utf8)
        try line(iso.string(from: now.addingTimeInterval(-60)), 35).write(toFile: "\(day)/rollout-b-\(UUID().uuidString).jsonl", atomically: true, encoding: .utf8)
        #expect(CodexLog(home: home).read(now: now).limits?.fiveHour?.usedPercent == 35)
    }
}

extension CodexLogTests {
    func tokenCount(at: Date, limits: String) -> String {
        #"{"timestamp":"\#(iso.string(from: at))","type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":\#(limits)}}"#
    }

    /// 2/11: the newest file was a rollout from 20/8, rewritten by a codex:// link test, with a free plan's 30-day
    /// window only; today's rollout had `rate_limits: null`. The limits come from the newest event with 5 h or week.
    @Test func afterARestartTheNewestEventWinsNotTheNewestFile() throws {
        let now = Date()
        let home = try makeHome([])
        let day = home + "/sessions/2026/11/01"
        let plus = #"{"primary":{"used_percent":12.0,"window_minutes":300,"resets_at":\#(Int(now.timeIntervalSince1970) - 600)},"secondary":{"used_percent":40.0,"window_minutes":10080,"resets_at":\#(Int(now.timeIntervalSince1970) + 86400)}}"#
        let free = #"{"primary":{"used_percent":48.0,"window_minutes":43200,"resets_at":1789813849},"secondary":null}"#
        let files: [(String, String, TimeInterval)] = [
            ("plus", tokenCount(at: now.addingTimeInterval(-2 * 86400), limits: plus), -3600),
            ("free", tokenCount(at: now.addingTimeInterval(-70 * 86400), limits: free), -60),  // rewritten a minute ago
            ("null", tokenCount(at: now.addingTimeInterval(-120), limits: "null"), -120),
        ]
        for (name, line, modified) in files {
            let path = "\(day)/rollout-2026-11-01T10-00-00-\(name)-\(UUID().uuidString.lowercased()).jsonl"
            try (line + "\n").write(toFile: path, atomically: true, encoding: .utf8)
            try FileManager.default.setAttributes([.modificationDate: now.addingTimeInterval(modified)], ofItemAtPath: path)
        }
        let ledger = MinuteLedger()
        #expect(CodexLog(home: home, ledger: ledger).read(now: now).limits?.week?.usedPercent == 40)
        // a relaunch: the files continue from their marks, past those lines
        let limits = try #require(CodexLog(home: home, ledger: ledger).read(now: now).limits)
        #expect(limits.fiveHour?.usedPercent == 12 && limits.week?.usedPercent == 40)
        // its 5 h window has reset: the week percent stays
        let sample = try #require(limits.sample)
        let entry = try #require(LimitLog(dir: nil).read(now: now, samples: ["codex": sample]).accounts["codex"])
        #expect(entry.sevenDayPct == 40 && entry.note != nil)
    }
}
