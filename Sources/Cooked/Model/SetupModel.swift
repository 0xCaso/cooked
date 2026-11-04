import Foundation
import Observation
import ServiceManagement

/// Open at login: `SMAppService.mainApp` in the app, a fake in tests.
protocol LoginItem: AnyObject {
    var isEnabled: Bool { get }
    func set(_ on: Bool) throws
}

/// The app's own login item. Outside a bundle (`swift run`) registering fails: Setup goes on without it.
final class MainAppLoginItem: LoginItem {
    var isEnabled: Bool { SMAppService.mainApp.status == .enabled }

    func set(_ on: Bool) throws {
        if on { try SMAppService.mainApp.register() } else if isEnabled { try SMAppService.mainApp.unregister() }
    }
}

/// Setup and Settings (release spec, section 3): which accounts Cooked reads and their names, the Claude percent through
/// the status line bridge, and open at login. Nothing outside Cooked's own folder is written before Allow or Connect.
@MainActor
@Observable
final class SetupModel {
    /// One found folder, as the Accounts step shows it.
    struct Row: Identifiable, Equatable {
        let found: Accounts.Found
        var ticked: Bool
        var name: String
        var id: String { found.id }
    }

    /// One real `settings.json` and the ticked Claude accounts that read it (several, through symlinks).
    struct SettingsFile: Equatable {
        struct Reader: Equatable {
            let name: String
            /// `~/.claude-work/settings.json`, as the account sees it.
            let path: String
            let isLink: Bool
        }
        let real: String
        let readers: [Reader]
    }

    enum Step: Int, CaseIterable, Comparable {
        case accounts, percent, login
        static func < (a: Step, b: Step) -> Bool { a.rawValue < b.rawValue }

        var title: String {
            switch self {
            case .accounts: "Accounts"
            case .percent: "Claude percent"
            case .login: "Open at login"
            }
        }
    }

    static let doneKey = "setupDone"

    let home: String
    let defaults: UserDefaults
    let statusLine: StatusLineSetup
    let login: LoginItem

    var rows: [Row]
    var step: Step = .accounts
    /// The Open at login toggle.
    var openAtLogin = true
    private(set) var percent: StatusLineSetup.State = .notConnected
    /// The last error from writing a settings file or the login item, in one line.
    private(set) var problem: String?
    /// Called after the accounts change, so the island reads the new list.
    var onAccountsChange: () -> Void = {}

    init(home: String = NSHomeDirectory(), defaults: UserDefaults = .standard,
         statusLine: StatusLineSetup = StatusLineSetup(), login: LoginItem = MainAppLoginItem()) {
        self.home = home
        self.defaults = defaults
        self.statusLine = statusLine
        self.login = login
        rows = Self.rows(found: Accounts.scan(home: home), saved: AccountStore.load(from: defaults))
        openAtLogin = login.isEnabled || !Self.isDone(defaults)
        refresh()
    }

    static func isDone(_ defaults: UserDefaults = .standard) -> Bool { defaults.bool(forKey: doneKey) }

    /// Saved accounts keep their tick and name; with nothing saved yet, every folder is ticked with its default name.
    /// A saved account whose folder is gone drops out.
    static func rows(found: [Accounts.Found], saved: [ConnectedAccount]?) -> [Row] {
        found.map { f in
            guard let saved else { return Row(found: f, ticked: true, name: Accounts.defaultName(f)) }
            let match = saved.first { $0.id == f.id }
            return Row(found: f, ticked: match != nil, name: match?.name ?? Accounts.defaultName(f))
        }
    }

    /// The ticked rows, in the list's order; an empty name falls back to the default one.
    var accounts: [ConnectedAccount] {
        rows.filter(\.ticked).map { row in
            let name = row.name.trimmingCharacters(in: .whitespaces)
            return ConnectedAccount(id: row.id, name: name.isEmpty ? Accounts.defaultName(row.found) : name, path: row.found.path)
        }
    }

    /// The steps this Mac needs: no Claude account ticked, no status line step.
    var steps: [Step] {
        accounts.contains { $0.kind == .claude } ? Step.allCases : [.accounts, .login]
    }

    var settingsFiles: [SettingsFile] {
        var order: [String] = []
        var readers: [String: [SettingsFile.Reader]] = [:]
        for account in accounts where account.kind == .claude {
            let path = account.path + "/settings.json"
            let real = StatusLineSetup.real(path)
            if readers[real] == nil { order.append(real) }
            let isLink = (try? FileManager.default.destinationOfSymbolicLink(atPath: path)) != nil
            readers[real, default: []].append(.init(name: account.name, path: tilde(path), isLink: isLink))
        }
        return order.map { SettingsFile(real: $0, readers: readers[$0] ?? []) }
    }

    /// `/Users/x/.claude` → `~/.claude`.
    func tilde(_ path: String) -> String {
        path.hasPrefix(home + "/") ? "~" + path.dropFirst(home.count) : path
    }

    /// Changed wins (the percent is paused somewhere), then connected, then not connected.
    func refresh() {
        let states = settingsFiles.map { statusLine.state(settings: $0.real) }
        percent = states.contains(.changed) ? .changed : states.contains(.connected) ? .connected : .notConnected
    }

    // MARK: - Actions

    func saveAccounts() {
        AccountStore.save(accounts, to: defaults)
        onAccountsChange()
        refresh()
    }

    /// Allow and Connect (again): every real settings file the ticked Claude accounts read.
    func connect() {
        run { for file in settingsFiles { try statusLine.connect(settings: file.real) } }
    }

    func disconnect() {
        run { for file in settingsFiles { try statusLine.disconnect(settings: file.real) } }
    }

    func setOpenAtLogin(_ on: Bool) {
        openAtLogin = on
        run { try login.set(on) }
    }

    /// Setup's last step: Done.
    func finish() {
        saveAccounts()
        setOpenAtLogin(openAtLogin)
        defaults.set(true, forKey: Self.doneKey)
    }

    /// The step after `step`, or nil after the last.
    func next(after step: Step) -> Step? { steps.first { $0 > step } }
    func back(from step: Step) -> Step? { steps.last { $0 < step } }

    /// Undoes all Cooked changed: the old status lines come back, Cooked stops opening at login, and the bridge script
    /// goes. The `settings.json.cooked-backup` copies stay.
    func removeAll() {
        run {
            for file in settingsFiles { try statusLine.disconnect(settings: file.real) }
            // Accounts unticked since connecting: their saved lines too.
            for name in (try? FileManager.default.contentsOfDirectory(atPath: statusLine.savedDir)) ?? [] where name.hasSuffix(".json") {
                if let data = FileManager.default.contents(atPath: statusLine.savedDir + "/" + name),
                   let saved = try? JSONDecoder().decode(StatusLineSetup.Saved.self, from: data) {
                    try statusLine.disconnect(settings: saved.settings)
                }
            }
            try login.set(false)
            openAtLogin = false
            if FileManager.default.fileExists(atPath: statusLine.script) { try FileManager.default.removeItem(atPath: statusLine.script) }
        }
    }

    /// The Remove confirmation, with this Mac's numbers.
    var removeSentence: String {
        let n = settingsFiles.reduce(0) { $0 + $1.readers.count }
        let where_ = n == 1 ? "" : n == 2 ? " in both accounts" : " in all \(n) accounts"
        return "Your old status line comes back\(where_), Cooked stops opening at login, and the bridge script is deleted. The backup of settings.json stays."
    }

    // MARK: - Details

    /// The newest bridge sample of a ticked account: its name, percent and time.
    func lastTerminalPercent() -> (name: String, percent: Int, at: Date)? {
        let samples = StatusLineBridge.samples(dir: statusLine.bridgeDir)
        let best = accounts.compactMap { a in samples[a.id].map { (a.name, $0) } }.max { $0.1.at < $1.1.at }
        return best.map { ($0.0, Int(($0.1.five * 100).rounded()), Date(timeIntervalSince1970: $0.1.at)) }
    }

    /// The command the user's own status line runs under the bridge, if any.
    func wrappedCommand() -> String? {
        for file in settingsFiles {
            let path = statusLine.savedDir + "/\(StatusLineSetup.key(file.real)).command"
            if let text = try? String(contentsOfFile: path, encoding: .utf8), !text.isEmpty { return text }
        }
        return nil
    }

    /// The command that replaced the bridge, for the changed state.
    func currentCommand() -> String? {
        for file in settingsFiles where statusLine.state(settings: file.real) == .changed {
            guard let text = try? String(contentsOfFile: file.real, encoding: .utf8),
                  let member = JSONObjectText(text)?.member("statusLine") else { return nil }
            let value = try? JSONSerialization.jsonObject(with: Data(member.value.utf8)) as? [String: Any]
            return value?["command"] as? String
        }
        return nil
    }

    private func run(_ body: () throws -> Void) {
        do { try body(); problem = nil } catch { problem = Self.message(error) }
        refresh()
    }

    static func message(_ error: Error) -> String {
        if case StatusLineSetup.Failure.notAnObject = error { return "settings.json is not a JSON object, so Cooked did not change it." }
        return error.localizedDescription
    }
}
