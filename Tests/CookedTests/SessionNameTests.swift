import Foundation
import Testing
@testable import Cooked

struct SessionNameTests {
    func titles(_ lines: [String]) -> TranscriptLog.Titles {
        var t = TranscriptLog.Titles()
        for line in lines { TranscriptLog.title(Data(line.utf8), into: &t) }
        return t
    }

    @Test func sampleNamesFromTheRealFormat() {
        let head = TranscriptLog.headTitles(of: Fixture.url("claude-session.jsonl"))
        #expect(head == .init(custom: "Renamed demo session", ai: "Demo session title", firstPrompt: "Fix the flaky login test",
                             cwd: "/Users/me/Projects/demo"))
        #expect(head.name(after: .init()) == "Renamed demo session")
    }

    @Test func customThenAiThenFirstPrompt() {
        let prompt = #"{"type":"user","message":{"role":"user","content":"Fix the build\nand more"}}"#
        let ai = #"{"type":"ai-title","aiTitle":"Build fix","sessionId":"s"}"#
        let custom = #"{"type":"custom-title","customTitle":"My build","sessionId":"s"}"#
        #expect(titles([prompt]).name(after: .init()) == "Fix the build")
        #expect(titles([prompt, ai]).name(after: .init()) == "Build fix")
        #expect(titles([prompt, ai, custom]).name(after: .init()) == "My build")
        // newer lines win: a rename in the tail beats the head's title
        #expect(titles([prompt, custom]).name(after: .init(custom: "Renamed later")) == "Renamed later")
        #expect(titles([prompt, custom]).name(after: .init(ai: "Newer AI title")) == "My build")
    }

    @Test func firstPromptSkipsCommandsContextAndToolOutput() {
        let t = titles([
            #"{"type":"user","isMeta":true,"message":{"content":"Caveat: injected"}}"#,
            #"{"type":"user","message":{"content":"<command-name>/clear</command-name>"}}"#,
            #"{"type":"user","message":{"content":[{"type":"tool_result","content":"output"}]}}"#,
            #"{"type":"user","message":{"content":[{"type":"text","text":"  \n  Plan the release  "}]}}"#,
            #"{"type":"user","message":{"content":"Second prompt"}}"#,
        ])
        #expect(t.firstPrompt == "Plan the release")
    }

    @Test func longNamesAreCut() {
        let name = TranscriptLog.clean(String(repeating: "a", count: 120))
        #expect(name?.count == 80 && name?.hasSuffix("…") == true)
    }

    @Test func liveSessionsCarryTheirName() throws {
        let now = Date()
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        let at = f.string(from: now.addingTimeInterval(-60))
        func call(_ id: String) -> String {
            #"{"type":"assistant","timestamp":"\#(at)","cwd":"/r/demo","message":{"id":"\#(id)","usage":{"input_tokens":10,"output_tokens":5}}}"#
        }
        let home = FileManager.default.temporaryDirectory.appendingPathComponent("home-\(UUID().uuidString)").path
        let project = home + "/.claude/projects/-r-demo"
        try FileManager.default.createDirectory(atPath: project + "/sub/subagents", withIntermediateDirectories: true)
        // a session with its own call, and one where only a subagent works: its name comes from the main file
        try ([#"{"type":"user","message":{"content":"Write the docs"}}"#, #"{"type":"ai-title","aiTitle":"Docs pass","sessionId":"main"}"#, call("m1")]
            .joined(separator: "\n") + "\n").write(toFile: project + "/main.jsonl", atomically: true, encoding: .utf8)
        try (#"{"type":"user","message":{"content":"Refactor the parser"}}"# + "\n").write(toFile: project + "/sub.jsonl", atomically: true, encoding: .utf8)
        try (call("a1") + "\n").write(toFile: project + "/sub/subagents/agent-1.jsonl", atomically: true, encoding: .utf8)
        let log = TranscriptLog(accounts: [.init(id: "claudeAgent", name: "claude", path: home + "/.claude")])
        let names = Dictionary(uniqueKeysWithValues: log.read(now: now).sessions.map { ($0.id, $0.name) })
        #expect(names == ["main": "Docs pass", "sub": "Refactor the parser"])
        // a rename while running shows at the next reading
        let handle = try FileHandle(forWritingTo: URL(fileURLWithPath: project + "/main.jsonl"))
        try handle.seekToEnd()
        try handle.write(contentsOf: Data((#"{"type":"custom-title","customTitle":"Docs v2","sessionId":"main"}"# + "\n").utf8))
        try handle.close()
        #expect(log.read(now: now).sessions.first { $0.id == "main" }?.name == "Docs v2")
    }

    @Test func sourceRowsShowTheSessionName() {
        let session = TranscriptLog.Session(account: "claudeAgent", id: "s", cwd: "/r/demo", tokens: 15_000,
                                            mainActive: true, activeSubagents: 0, name: "Docs pass")
        let out = TokenSources.make(sessions: [session], threads: [:])
        #expect(out.first?.detail == "Docs pass")
        #expect(out.first?.project == "demo")
    }
}
