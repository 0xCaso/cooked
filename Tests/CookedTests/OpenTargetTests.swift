import Foundation
import Testing
@testable import Cooked

/// A click on a row goes where the session runs (release spec, section 1, Click on a row).
struct OpenTargetTests {
    let accounts = [ConnectedAccount(id: "claudeAgent", name: "personal", path: "/Users/x/.claude"),
                    ConnectedAccount(id: "claudeAgent_work", name: "work", path: "/Users/x/.claude-work"),
                    ConnectedAccount(id: "codex", name: "codex", path: "/Users/x/.codex")]

    func session(_ account: String, _ id: String) -> TranscriptLog.Session {
        .init(account: account, id: id, cwd: "/Users/x/My Repo", tokens: 600, mainActive: true, activeSubagents: 0).steady
    }

    @Test func eachKindOfRowHasItsPlace() {
        let threads = ["t3-session": TokenLog.Thread(id: "thread-1", title: "A thread", project: "p")]
        let sources = TokenSources.make(sessions: [session("claudeAgent", "t3-session"), session("claudeAgent", "home"),
                                                   session("claudeAgent_work", "work"), session("codex", "cx")],
                                        threads: threads, accounts: accounts)
        let byID = Dictionary(uniqueKeysWithValues: sources.map { ($0.id, $0.target) })
        #expect(byID["thread-1"] == .t3Code(thread: "thread-1"))
        #expect(byID["home"] == .claude(session: "home", cwd: "/Users/x/My Repo", configDir: nil))
        #expect(byID["work"] == .claude(session: "work", cwd: "/Users/x/My Repo", configDir: "/Users/x/.claude-work"))
        #expect(byID["cx"] == .codex(session: "cx", cwd: "/Users/x/My Repo"))
    }

    @Test func theTerminalFallbackResumesInTheProjectFolder() {
        let other = OpenTarget.claude(session: "work", cwd: "/Users/x/it's", configDir: "/Users/x/.claude-work")
        #expect(other.script == "#!/bin/sh\ncd '/Users/x/it'\\''s' || exit 1\nCLAUDE_CONFIG_DIR='/Users/x/.claude-work' claude --resume 'work'\n")
        #expect(OpenTarget.claude(session: "home", cwd: nil, configDir: nil).command == "claude --resume 'home'")
        #expect(OpenTarget.codex(session: "cx", cwd: nil).command == "codex resume 'cx'")
    }
}
