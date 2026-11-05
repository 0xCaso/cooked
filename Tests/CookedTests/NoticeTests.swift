import Foundation
import Testing
@testable import Cooked

/// The popover's notice card.
struct NoticeTests {
    let work = ConnectedAccount(id: "claudeAgent_work", name: "work", path: "/h/.claude-work")
    let personal = ConnectedAccount(id: "claudeAgent", name: "personal", path: "/h/.claude")
    let codex = ConnectedAccount(id: "codex", name: "codex", path: "/h/.codex")

    @Test func oneAtATimeInOrder() {
        let all = Notice.all(accounts: [work, personal, codex], formatChanged: ["codex": "/h/.codex/r.jsonl"],
                             statusLine: ["claudeAgent_work": .changed, "claudeAgent": .notConnected],
                             withPercent: ["codex"])
        #expect(all == [.formatChanged(account: "codex", file: "/h/.codex/r.jsonl"), .paused,
                        .noPercent(id: "claudeAgent", account: "personal")])
        #expect(Notice.all(accounts: []) == [.noAccounts])
    }

    @Test func apiKeyOnlyAfterTheStatusLineRan() {
        let lines: [String: StatusLineSetup.State] = ["claudeAgent_work": .connected]
        // connected, no call yet: nothing to say
        #expect(Notice.all(accounts: [work], statusLine: lines).isEmpty)
        #expect(Notice.all(accounts: [work], statusLine: lines, bridged: ["claudeAgent_work"])
                == [.apiKey(id: "claudeAgent_work", account: "work")])
        #expect(Notice.all(accounts: [work], statusLine: lines, withPercent: ["claudeAgent_work"], bridged: ["claudeAgent_work"]).isEmpty)
        // Codex always has its percent in the rollouts: never asked to connect
        #expect(Notice.all(accounts: [codex]).isEmpty)
    }

    @MainActor @Test func notNowHidesItUntilTheAccountsChange() throws {
        let defaults = try #require(UserDefaults(suiteName: "notice-\(UUID().uuidString)"))
        let model = IslandModel(defaults: defaults)
        let missing = Notice.noPercent(id: "claudeAgent", account: "personal")
        model.show([missing, .apiKey(id: "claudeAgent_work", account: "work")])
        model.dismiss(missing)
        #expect(model.notice == .apiKey(id: "claudeAgent_work", account: "work"))
        #expect(IslandModel(defaults: defaults).dismissed == ["noPercent:claudeAgent"]) // kept across launches
        model.accountsChanged()
        #expect(model.notice == missing)
        model.dismiss(.paused) // a notice with a fix cannot be put off
        #expect(model.dismissed.isEmpty)
    }

    @Test func aConnectedAccountWithoutAPercentKeepsItsRow() {
        let status = StatusFile(updatedAt: "", accounts: [:])
        let minuteNow = Int(Date().timeIntervalSince1970) / 60
        let s = LiveSnapshot.make(status: status, minutes: ["claudeAgent": [minuteNow: 5000]], now: Date(), accounts: [personal])
        #expect(s?.accounts.map(\.id) == ["claudeAgent"])
        #expect(s?.active?.hasPercent == false)
        #expect(s?.active?.fiveHourLeftIfKnown == nil)
        #expect(s?.isUrgent == false)
        #expect((s?.rateNow ?? 0) > 0)
    }

    @Test func onlyTheNewestFileIsChecked() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("format-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let old = dir.appendingPathComponent("old.jsonl"), new = dir.appendingPathComponent("new.jsonl")
        try #"{"type":"user","sessionId":"s"}"#.write(to: old, atomically: true, encoding: .utf8)
        try #"{"kind":"turn","id":"s"}"#.write(to: new, atomically: true, encoding: .utf8)
        var cache: [String: SessionFormat] = [:]
        let urls = [(url: old, modified: Date(timeIntervalSince1970: 1)), (url: new, modified: Date(timeIntervalSince1970: 2))]
        #expect(SessionFormat.newestChanged(urls, cache: &cache) == new.path)
        #expect(cache == [new.path: .formatChanged])
        #expect(SessionFormat.newestChanged(Array(urls.prefix(1)), cache: &cache) == nil)
    }
}
