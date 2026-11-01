import Foundation

/// One Claude Code or Codex account Cooked reads: a config folder, with the short name the user gave it.
/// Ids follow T3 Code's provider instances, so the T3 Code extras still match: `~/.claude` is `claudeAgent`,
/// `~/.claude-x-y` is `claudeAgent_x_y`, `~/.codex` is `codex`.
struct ConnectedAccount: Codable, Equatable, Identifiable, Sendable {
    let id: String
    var name: String
    /// The config folder: `~/.claude-work`.
    let path: String

    var kind: Accounts.Kind { id == Accounts.codexID ? .codex : .claude }
}

enum Accounts {
    enum Kind: Equatable, Sendable { case claude, codex }

    static let codexID = "codex"

    /// A folder found on this Mac, connected or not.
    struct Found: Equatable {
        let id: String
        let kind: Kind
        let path: String
        /// The newest change in its sessions; nil when it has none.
        let lastUsed: Date?
    }

    /// Every `~/.claude*` folder with a `projects/` folder of transcripts, and `~/.codex` with `sessions/`:
    /// `~/.claude` first, the other Claude folders by name, Codex last.
    static func scan(home: String = NSHomeDirectory(), files: FileManager = .default) -> [Found] {
        let names = ((try? files.contentsOfDirectory(atPath: home)) ?? []).sorted()
        var out: [Found] = []
        for name in names where name == ".claude" || name.hasPrefix(".claude-") {
            let projects = home + "/" + name + "/projects"
            guard isDirectory(projects, files) else { continue }
            out.append(Found(id: id(folder: name), kind: .claude, path: home + "/" + name, lastUsed: newest(in: projects, files)))
        }
        let sessions = home + "/.codex/sessions"
        if isDirectory(sessions, files) {
            out.append(Found(id: codexID, kind: .codex, path: home + "/.codex", lastUsed: newestCodex(sessions, files)))
        }
        return out
    }

    /// `.claude` → `claudeAgent`, `.claude-team` → `claudeAgent_team`.
    static func id(folder: String) -> String {
        "claudeAgent" + folder.dropFirst(".claude".count).replacingOccurrences(of: "-", with: "_")
    }

    /// The name an account gets until the user changes it: `.claude` → "claude", `.claude-team` → "team".
    static func defaultName(_ found: Found) -> String {
        if found.kind == .codex { return "codex" }
        let folder = URL(fileURLWithPath: found.path).lastPathComponent
        let rest = folder.dropFirst(".claude".count).drop { $0 == "-" }
        return rest.isEmpty ? "claude" : rest.replacingOccurrences(of: "-", with: " ")
    }

    private static func isDirectory(_ path: String, _ files: FileManager) -> Bool {
        var dir: ObjCBool = false
        return files.fileExists(atPath: path, isDirectory: &dir) && dir.boolValue
    }

    private static func modified(_ path: String, _ files: FileManager) -> Date? {
        (try? files.attributesOfItem(atPath: path))?[.modificationDate] as? Date
    }

    /// A project folder changes when a session file is added in it.
    private static func newest(in dir: String, _ files: FileManager) -> Date? {
        ((try? files.contentsOfDirectory(atPath: dir)) ?? []).compactMap { modified(dir + "/" + $0, files) }.max()
    }

    /// `sessions/<year>/<month>/<day>/`: the last day folder.
    private static func newestCodex(_ dir: String, _ files: FileManager) -> Date? {
        var path = dir
        for _ in 0..<3 {
            guard let last = ((try? files.contentsOfDirectory(atPath: path)) ?? []).filter({ !$0.hasPrefix(".") }).max() else { break }
            path += "/" + last
        }
        return path == dir ? nil : newest(in: path, files) ?? modified(path, files)
    }
}

/// The connected accounts, in display order, in UserDefaults as JSON. The setup window (a later task) edits them;
/// until then, the first launch connects every folder `Accounts.scan` finds, with its default name.
enum AccountStore {
    static let key = "accounts"

    static func load(from defaults: UserDefaults = .standard) -> [ConnectedAccount]? {
        defaults.data(forKey: key).flatMap { try? JSONDecoder().decode([ConnectedAccount].self, from: $0) }
    }

    static func save(_ accounts: [ConnectedAccount], to defaults: UserDefaults = .standard) {
        if let data = try? JSONEncoder().encode(accounts) { defaults.set(data, forKey: key) }
    }

    /// The saved accounts, or, the first time, every found folder.
    static func current(defaults: UserDefaults = .standard, home: String = NSHomeDirectory()) -> [ConnectedAccount] {
        if let saved = load(from: defaults) { return saved }
        let found = Accounts.scan(home: home).map { ConnectedAccount(id: $0.id, name: Accounts.defaultName($0), path: $0.path) }
        if !found.isEmpty { save(found, to: defaults) } // nothing found yet: scan again next launch
        return found
    }
}
