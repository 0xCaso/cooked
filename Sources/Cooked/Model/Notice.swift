import Foundation

/// What the popover's notice card says when a number is missing, and its one fix
///. One at a time, in the order of the cases.
enum Notice: Equatable, Sendable {
    /// No connected account: the island says "Set up Cooked".
    case noAccounts
    /// A connected account's newest session file has lines Cooked does not know: its tokens are not counted.
    case formatChanged(account: String, file: String)
    /// Cooked connected the status line, but it changed since: no Claude percent until it connects again.
    case paused
    /// A Claude account with no percent, its status line not connected.
    case noPercent(id: String, account: String)
    /// The status line is connected but Claude sends no percent: an API key, which has no 5-hour limit.
    case apiKey(id: String, account: String)

    var title: String {
        switch self {
        case .noAccounts: "No accounts connected"
        case .formatChanged: "Cooked cannot read Claude Code's new session files"
        case .paused: "Claude percent paused"
        case .noPercent(_, let account): "No 5-hour percent for \(account)"
        case .apiKey(_, let account): "\(account) uses an API key"
        }
    }

    var body: String {
        switch self {
        case .noAccounts: "Cooked reads the session files of your Claude and Codex accounts on this Mac."
        case .formatChanged: "Tokens per minute stop for this account until you update Cooked. The percents still work."
        case .paused: "Your status line changed, so Cooked gets no percent from Claude."
        case .noPercent: "Claude sends it only to the status line. Cooked can read it there."
        case .apiKey: "API keys have no 5-hour limit, so Cooked shows only tokens per minute."
        }
    }

    /// The key that hides it: only the notices you can put off have one.
    var dismissKey: String? {
        switch self {
        case .noPercent(let id, _): "noPercent:\(id)"
        case .apiKey(let id, _): "apiKey:\(id)"
        default: nil
        }
    }

    /// "Copy details" for a changed format: what a bug report needs.
    var details: String? {
        guard case .formatChanged(let account, let file) = self else { return nil }
        return "Cooked cannot read the session format of \(account).\nFile: \(file)"
    }

    /// Every notice that applies, first the one to show. `statusLine`: each Claude account's status line;
    /// `bridged`: the accounts whose status line ran (the bridge wrote their file), with or without a percent.
    static func all(accounts: [ConnectedAccount], formatChanged: [String: String] = [:],
                    statusLine: [String: StatusLineSetup.State] = [:], withPercent: Set<String> = [],
                    bridged: Set<String> = []) -> [Notice] {
        if accounts.isEmpty { return [.noAccounts] }
        var out: [Notice] = accounts.compactMap { a in formatChanged[a.id].map { .formatChanged(account: a.name, file: $0) } }
        if accounts.contains(where: { statusLine[$0.id] == .changed }) { out.append(.paused) }
        let missing = accounts.filter { $0.kind == .claude && !withPercent.contains($0.id) }
        // a status line not connected comes before an API key; connected with no call yet says nothing
        out += missing.filter { (statusLine[$0.id] ?? .notConnected) == .notConnected }
            .map { .noPercent(id: $0.id, account: $0.name) }
        out += missing.filter { statusLine[$0.id] == .connected && bridged.contains($0.id) }
            .map { .apiKey(id: $0.id, account: $0.name) }
        return out
    }

    /// The one to show: the first not put off.
    static func first(_ notices: [Notice], dismissed: Set<String>) -> Notice? {
        notices.first { $0.dismissKey.map { !dismissed.contains($0) } ?? true }
    }
}
