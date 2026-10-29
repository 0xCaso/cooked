import Foundation

/// One line of "What burns tokens": a project and the T3 Code thread working in it, over the last 15 minutes.
struct TokenSource: Identifiable, Equatable {
    /// The T3 thread id when known, else the Claude session id.
    let id: String
    /// The T3 Code thread, when the source runs in one: a click opens it.
    var threadID: String? = nil
    let project: String
    /// "Draft the release notes · work": the thread's title and the account.
    let detail: String
    /// New tokens per minute over the last 15 minutes.
    let rate: Double
    /// "1 agent + 3 sub", "3 sub"; nil when nothing ran in the last 5 minutes.
    let agents: String?
}

enum TokenSources {
    static let limit = 3
    static let window: Double = 15
    /// Lines under this many tokens per minute are left out: a source of 0/min says nothing.
    static let minimumRate: Double = 1

    /// The top sources: Claude sessions from their transcripts, and the database's threads for the other accounts.
    static func make(sessions: [TranscriptLog.Session], threads: [String: TokenLog.Thread],
                     recent: [TokenLog.RecentThread], transcriptAccounts: Set<String>) -> [TokenSource] {
        var out = sessions.map { s in
            let thread = threads[s.id]
            return TokenSource(id: thread?.id ?? s.id, threadID: thread?.id, project: thread.map(project) ?? project(cwd: s.cwd),
                               detail: detail(thread?.title ?? "Claude Code session", account: s.account),
                               rate: s.tokens / window, agents: agents(main: s.mainActive, subagents: s.activeSubagents))
        }
        out += recent.filter { !transcriptAccounts.contains($0.account) }.map { r in
            // finished turns only: the database has no running count, so one agent per thread
            TokenSource(id: r.thread.id, threadID: r.thread.id, project: project(r.thread), detail: detail(r.thread.title, account: r.account),
                        rate: r.tokens / window, agents: nil)
        }
        // one T3 thread can hold several sessions (a resumed or handed-off conversation): one line
        var merged: [String: TokenSource] = [:]
        for s in out {
            if let m = merged[s.id] {
                merged[s.id] = TokenSource(id: s.id, threadID: m.threadID, project: m.project, detail: m.detail, rate: m.rate + s.rate,
                                           agents: m.agents ?? s.agents)
            } else { merged[s.id] = s }
        }
        return merged.values.filter { $0.rate >= minimumRate }
            .sorted { ($0.rate, $0.id) > ($1.rate, $1.id) }
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

    static func detail(_ title: String, account: String) -> String {
        "\(title) · \(LiveSnapshot.short(id: account, name: account))"
    }

    static func project(_ thread: TokenLog.Thread) -> String { thread.project ?? "t3 scratch" }

    /// The folder's name; T3 Code's scratch threads, one folder each, are all "t3 scratch".
    static func project(cwd: String?) -> String {
        guard let cwd, !cwd.isEmpty else { return "unknown" }
        if cwd.contains("/.t3/scratch") { return "t3 scratch" }
        return URL(fileURLWithPath: cwd).lastPathComponent
    }
}
