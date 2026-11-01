import Foundation
import Testing
@testable import Cooked

/// Real Claude Code and Codex lines, anonymized by `scripts/fixtures.py`. When a CLI update changes the format,
/// run that script again: these tests show what changed.
enum Fixture {
    static func url(_ name: String) -> URL {
        URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("Fixtures/\(name)")
    }

    static func lines(_ name: String) throws -> [Data] {
        try Data(contentsOf: url(name)).split(separator: UInt8(ascii: "\n")).map { Data($0) }
    }

    /// The `token_count` payloads of a Codex rollout.
    static func tokenCounts(_ name: String) throws -> [[String: Any]] {
        try lines(name).compactMap { line in
            let json = try JSONSerialization.jsonObject(with: line) as? [String: Any]
            let payload = json?["payload"] as? [String: Any]
            return payload?["type"] as? String == "token_count" ? payload : nil
        }
    }
}

struct SessionFormatTests {
    @Test func claudeSampleIsKnownAndCountsOneMessageOnce() throws {
        #expect(SessionFormat.check(file: Fixture.url("claude-session.jsonl")) == .claude)
        let calls = try Fixture.lines("claude-session.jsonl").compactMap(TranscriptLog.parse)
        #expect(calls.count == 2) // one message written in two content blocks
        #expect(Set(calls.map(\.id)).count == 1)
        #expect(calls[0].tokens == 2 + 33682 + 1240) // input + cache write + output, not cache read
        #expect(calls[0].cwd == "/Users/me/Projects/demo")
    }

    @Test func claudeSampleHasTheNameLines() throws {
        let kinds = try Fixture.lines("claude-session.jsonl").compactMap {
            (try JSONSerialization.jsonObject(with: $0) as? [String: Any])?["type"] as? String
        }
        #expect(Set(kinds).isSuperset(of: ["custom-title", "ai-title", "user", "assistant"]))
    }

    @Test func codexSamplesAreKnown() {
        #expect(SessionFormat.check(file: Fixture.url("codex-rollout.jsonl")) == .codex)
        #expect(SessionFormat.check(file: Fixture.url("codex-imported-rollout.jsonl")) == .codex)
    }

    @Test func codexCliRolloutHasLimitsAndARepeatedEvent() throws {
        let counts = try Fixture.tokenCounts("codex-rollout.jsonl")
        #expect(counts.count == 3)
        #expect(counts.allSatisfy { $0["rate_limits"] is [String: Any] })
        // the last two events carry the same totals: the Codex reader (task 4) must count them once
        let totals = counts.map { (($0["info"] as? [String: Any])?["total_token_usage"] as? [String: Any])?["total_tokens"] as? Int }
        #expect(totals[1] != nil && totals[1] == totals[2])
    }

    @Test func codexImportedRolloutAddsNoTokens() throws {
        let counts = try Fixture.tokenCounts("codex-imported-rollout.jsonl")
        #expect(!counts.isEmpty)
        for count in counts {
            #expect(count["rate_limits"] == nil || count["rate_limits"] is NSNull)
            let last = (count["info"] as? [String: Any])?["last_token_usage"] as? [String: Any]
            #expect(last?["input_tokens"] as? Int == 0 && last?["output_tokens"] as? Int == 0)
        }
    }

    @Test func codexSessionIndexHasNames() throws {
        let rows = try Fixture.lines("codex-session-index.jsonl").map { try JSONSerialization.jsonObject(with: $0) as? [String: Any] }
        #expect(rows.count == 2)
        #expect(rows.allSatisfy { $0?["id"] is String && $0?["thread_name"] is String })
    }

    @Test func unknownLinesAreAFormatChangeNotZero() {
        #expect(SessionFormat.check(file: Fixture.url("unknown-format.jsonl")) == .formatChanged)
        #expect(SessionFormat.check([Data("not json".utf8)]) == .formatChanged)
        #expect(SessionFormat.check([Data()]) == .empty)
        #expect(SessionFormat.check(file: Fixture.url("missing.jsonl")) == .empty)
    }
}
