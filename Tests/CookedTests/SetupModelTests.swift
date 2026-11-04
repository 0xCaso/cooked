import Foundation
import Testing
@testable import Cooked

/// A fresh user (temporary home) gets through Setup, and skipping each step leaves their files as they were.
@MainActor
struct SetupModelTests {
    final class FakeLogin: LoginItem {
        var isEnabled = false
        func set(_ on: Bool) throws { isEnabled = on }
    }

    let home: String
    let defaults: UserDefaults
    let login = FakeLogin()
    let settingsText = "{\n  \"statusLine\": {\"type\": \"command\", \"command\": \"my-line\"}\n}\n"

    init() throws {
        home = FileManager.default.temporaryDirectory.appendingPathComponent("setup-\(UUID().uuidString)").path
        let files = FileManager.default
        for dir in [".claude/projects/p", ".claude-work/projects/p", ".codex/sessions/2026/11/04", ".claude-empty"] {
            try files.createDirectory(atPath: home + "/" + dir, withIntermediateDirectories: true)
        }
        try settingsText.write(toFile: home + "/.claude/settings.json", atomically: true, encoding: .utf8)
        try files.createSymbolicLink(atPath: home + "/.claude-work/settings.json", withDestinationPath: home + "/.claude/settings.json")
        defaults = UserDefaults(suiteName: "setup-\(UUID().uuidString)")!
    }

    func model() -> SetupModel {
        SetupModel(home: home, defaults: defaults, statusLine: StatusLineSetup(support: home + "/support"), login: login)
    }

    var settings: String { (try? String(contentsOfFile: home + "/.claude/settings.json", encoding: .utf8)) ?? "" }

    @Test func aFreshUserGetsThroughSetup() {
        let m = model()
        #expect(m.rows.map(\.id) == ["claudeAgent", "claudeAgent_work", "codex"])
        #expect(m.rows.allSatisfy { $0.ticked })
        #expect(m.steps == [.accounts, .percent, .login])
        #expect(m.openAtLogin)

        m.rows[0].name = "personal"
        m.rows[2].ticked = false
        #expect(m.settingsFiles.count == 1)
        #expect(m.settingsFiles[0].readers.map(\.name) == ["personal", "work"])
        #expect(m.settingsFiles[0].readers.map(\.isLink) == [false, true])
        #expect(m.removeSentence.hasPrefix("Your old status line comes back in both accounts, Cooked stops opening at login"))

        m.connect()
        #expect(m.percent == .connected)
        #expect(m.wrappedCommand() == "my-line")
        m.finish()
        #expect(AccountStore.load(from: defaults)?.map(\.name) == ["personal", "work"])
        #expect(login.isEnabled)
        #expect(SetupModel.isDone(defaults))

        // Remove undoes everything and keeps the backup
        m.removeAll()
        #expect(settings == settingsText)
        #expect(m.percent == .notConnected)
        #expect(!login.isEnabled && !m.openAtLogin)
        #expect(!FileManager.default.fileExists(atPath: home + "/support/statusline-bridge.sh"))
        #expect(FileManager.default.fileExists(atPath: home + "/.claude/settings.json.cooked-backup"))
    }

    @Test func skippingEveryStepChangesNoFile() {
        let m = model()
        m.openAtLogin = false
        m.finish()
        #expect(settings == settingsText)
        #expect(!FileManager.default.fileExists(atPath: home + "/support"))
        #expect(!login.isEnabled)
        #expect(AccountStore.load(from: defaults)?.count == 3)
        #expect(SetupModel.isDone(defaults))
    }

    @Test func noClaudeAccountNoPercentStep() {
        let m = model()
        m.rows[0].ticked = false
        m.rows[1].ticked = false
        #expect(m.steps == [.accounts, .login])
        #expect(m.next(after: .accounts) == .login)
        #expect(m.back(from: .login) == .accounts)
    }

    @Test func savedAccountsKeepTheirTickAndName() {
        let found = Accounts.scan(home: home)
        let rows = SetupModel.rows(found: found, saved: [ConnectedAccount(id: "claudeAgent_work", name: "work", path: home + "/.claude-work")])
        #expect(rows.map(\.ticked) == [false, true, false])
        #expect(rows.map(\.name) == ["claude", "work", "codex"])
    }

    @Test func anEmptyNameFallsBackToTheDefault() {
        let m = model()
        m.rows[1].name = "  "
        #expect(m.accounts[1].name == "work")
    }

    @Test func aChangedLineShowsAsChanged() throws {
        let m = model()
        m.connect()
        try #"{"statusLine":{"type":"command","command":"npx claude-hud"}}"#
            .write(toFile: home + "/.claude/settings.json", atomically: true, encoding: .utf8)
        m.refresh()
        #expect(m.percent == .changed)
        #expect(m.currentCommand() == "npx claude-hud")
        m.connect()
        #expect(m.percent == .connected)
    }

    @Test func lastUseReadsShort() {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "Europe/Rome")!
        let now = ISO8601DateFormatter().date(from: "2026-11-04T17:00:00Z")!
        #expect(LastUse.text(now.addingTimeInterval(-3600), now: now, calendar: calendar) == "today 17:00")
        #expect(LastUse.text(now.addingTimeInterval(-86400), now: now, calendar: calendar) == "yesterday")
        #expect(LastUse.text(now.addingTimeInterval(-4 * 86400), now: now, calendar: calendar) == "Sat 31 Oct")
        #expect(LastUse.text(nil, now: now, calendar: calendar) == "never")
    }
}
