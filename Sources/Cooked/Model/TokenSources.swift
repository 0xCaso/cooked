import Foundation

/// One line of "What burns tokens": a project and the T3 Code thread working in it, over the last 15 minutes.
struct TokenSource: Identifiable, Equatable {
    /// The T3 thread id when known, else the Claude session id.
    let id: String
    /// The T3 Code thread, when the source runs in one: a click opens it.
    var threadID: String? = nil
    let project: String
    /// The thread's title: "Draft the release notes".
    let detail: String
    /// The account's short name: "team".
    var account = ""
    /// New tokens per minute over the last 15 minutes.
    let rate: Double
    /// "1 agent + 3 sub", "3 sub"; nil when nothing ran in the last 5 minutes.
    let agents: String?
    /// Minutes since the last call, when nothing ran in the last 5 minutes: the line goes under "Ended".
    var endedAgo: Int? = nil
}

enum TokenSources {
    /// The full list, when the card is open; it shows `SourcesCard.collapsed` lines until then.
    static let limit = 8
    static let window: Double = 15
    /// Lines under this many tokens per minute are left out: a source of 0/min says nothing.
    static let minimumRate: Double = 1

    /// The top sources: the sessions of the session files, named by their T3 Code thread when they have one.
    static func make(sessions: [TranscriptLog.Session], threads: [String: TokenLog.Thread],
                     accounts: [ConnectedAccount] = [], now: Date? = nil) -> [TokenSource] {
        func short(_ id: String) -> String { LiveSnapshot.short(id: id, accounts: accounts) }
        let minuteNow = now.map { Int($0.timeIntervalSince1970) / 60 }
        func ended(_ s: TranscriptLog.Session) -> Int? {
            guard !s.mainActive, s.activeSubagents == 0, let minuteNow, let last = s.lastMinute else { return nil }
            return max(minuteNow - last, 0)
        }
        let out = sessions.map { s in
            let thread = threads[s.id]
            return TokenSource(id: thread?.id ?? s.id, threadID: thread?.id, project: thread.map(project) ?? project(cwd: s.cwd),
                               detail: thread?.title ?? s.name ?? (s.account == Accounts.codexID ? "Codex session" : "Claude Code session"), account: short(s.account),
                               rate: s.tokens / window, agents: agents(main: s.mainActive, subagents: s.activeSubagents),
                               endedAgo: ended(s))
        }
        // one T3 thread can hold several sessions (a resumed or handed-off conversation): one line
        var merged: [String: TokenSource] = [:]
        for s in out {
            if let m = merged[s.id] {
                merged[s.id] = TokenSource(id: s.id, threadID: m.threadID, project: m.project, detail: m.detail, account: m.account, rate: m.rate + s.rate,
                                           agents: m.agents ?? s.agents,
                                           endedAgo: m.endedAgo.flatMap { a in s.endedAgo.map { min(a, $0) } })
            } else { merged[s.id] = s }
        }
        return merged.values.filter { $0.rate >= minimumRate }
            // live lines by rate, then the ended ones, the newest first
            .sorted { a, b in
                switch (a.endedAgo, b.endedAgo) {
                case (nil, nil): (a.rate, a.id) > (b.rate, b.id)
                case (nil, _): true
                case (_, nil): false
                case let (x?, y?): (x, b.rate, a.id) < (y, a.rate, b.id)
                }
            }
            .prefix(limit).map { $0 }
    }

    static func agents(main: Bool, subagents: Int) -> String? {
        switch (main, subagents) {
        case (false, 0): nil
        case (true, 0): "1 agent"
        case (true, _): "1 agent + \(subagents) sub"
        case (false, _): "\(subagents) sub"
        }
    }

    static func project(_ thread: TokenLog.Thread) -> String { thread.project ?? "t3 scratch" }

    /// The folder's name; T3 Code's scratch threads, one folder each, are all "t3 scratch".
    static func project(cwd: String?) -> String {
        guard let cwd, !cwd.isEmpty else { return "unknown" }
        if cwd.contains("/.t3/scratch") { return "t3 scratch" }
        return URL(fileURLWithPath: cwd).lastPathComponent
    }
}
