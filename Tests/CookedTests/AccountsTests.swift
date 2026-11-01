import Foundation
import Testing
@testable import Cooked

struct AccountsTests {
    /// A temporary home with `~/.claude`, `~/.claude-work-team`, `~/.codex`, and folders that are not accounts.
    func makeHome() throws -> String {
        let home = FileManager.default.temporaryDirectory.appendingPathComponent("home-\(UUID().uuidString)").path
        for dir in [".claude/projects/-Users-me-demo", ".claude-work-team/projects/p", ".codex/sessions/2026/11/01",
                    ".claude-empty", ".claude-plugins/cache", ".config"] {
            try FileManager.default.createDirectory(atPath: home + "/" + dir, withIntermediateDirectories: true)
        }
        FileManager.default.createFile(atPath: home + "/.codex/sessions/2026/11/01/rollout-a.jsonl", contents: Data())
        return home
    }

    @Test func scanFindsClaudeAndCodexFolders() throws {
        let home = try makeHome()
        let found = Accounts.scan(home: home)
        #expect(found.map(\.id) == ["claudeAgent", "claudeAgent_work_team", "codex"])
        #expect(found.map(\.kind) == [.claude, .claude, .codex])
        #expect(found.map(\.path) == [home + "/.claude", home + "/.claude-work-team", home + "/.codex"])
        #expect(found.allSatisfy { $0.lastUsed != nil })
        #expect(found.map(Accounts.defaultName) == ["claude", "work team", "codex"])
    }

    @Test func idsMatchT3CodeProviderInstances() {
        #expect(Accounts.id(folder: ".claude") == "claudeAgent")
        #expect(Accounts.id(folder: ".claude-team") == "claudeAgent_team")
    }

    @Test func firstLaunchConnectsEveryFolderThenKeepsTheUsersNames() throws {
        let home = try makeHome()
        let defaults = try #require(UserDefaults(suiteName: "accounts-\(UUID().uuidString)"))
        var accounts = AccountStore.current(defaults: defaults, home: home)
        #expect(accounts.map(\.name) == ["claude", "work team", "codex"])
        accounts[0].name = "personal"
        AccountStore.save([accounts[1], accounts[0]], to: defaults) // reordered, Codex not ticked
        let again = AccountStore.current(defaults: defaults, home: home)
        #expect(again.map(\.name) == ["work team", "personal"])
    }

    @Test func nothingFoundIsNotSaved() throws {
        let defaults = try #require(UserDefaults(suiteName: "accounts-\(UUID().uuidString)"))
        #expect(AccountStore.current(defaults: defaults, home: "/nonexistent").isEmpty)
        #expect(AccountStore.load(from: defaults) == nil)
    }

    @Test func transcriptsReadOnlyConnectedClaudeFolders() {
        let log = TranscriptLog(accounts: [.init(id: "claudeAgent", name: "a", path: "/h/.claude"),
                                           .init(id: "codex", name: "c", path: "/h/.codex")])
        #expect(log.dirs == ["claudeAgent": "/h/.claude"])
    }

    @Test func islandShowsOnlyConnectedAccountsInTheirOrderWithTheirNames() throws {
        let status = try StatusFile.decode(Data(statusFixture.utf8))
        let accounts = [ConnectedAccount(id: "claudeAgent", name: "home", path: "/h/.claude"),
                        ConnectedAccount(id: "claudeAgent_work", name: "job", path: "/h/.claude-work")]
        let s = try #require(LiveSnapshot.make(status: status, minutes: .init(), now: Date(), accounts: accounts))
        #expect(s.accounts.map(\.short) == ["home", "job"])
    }
}
