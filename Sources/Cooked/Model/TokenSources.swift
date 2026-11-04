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
    /// Where a click goes when the source has no T3 Code thread.
    var session: OpenTarget? = nil

    /// The T3 Code thread first, else the session itself.
    var target: OpenTarget? { threadID.map { .t3Code(thread: $0) } ?? session }
}

/// Where a click on a row goes (release spec, section 1, Click on a row). A struct, not an enum with payloads: that
/// enum, inside `TokenSource`, crashed the debug build on release (Swift 6, `outlined destroy of TokenSource`).
struct OpenTarget: Equatable {
    enum Place: Equatable { case t3Code, codex, claude }
    let place: Place
    /// The T3 Code thread, or the session id.
    let id: String
    var cwd: String? = nil
    /// A Claude account's folder; nil for `~/.claude`, the only account the Claude app can open.
    var configDir: String? = nil

    static func t3Code(thread: String) -> OpenTarget { .init(place: .t3Code, id: thread) }
    static func codex(session: String, cwd: String?) -> OpenTarget { .init(place: .codex, id: session, cwd: cwd) }
    static func claude(session: String, cwd: String?, configDir: String?) -> OpenTarget {
        .init(place: .claude, id: session, cwd: cwd, configDir: configDir)
    }

    /// The session behind a transcript: Codex, `~/.claude`, or another Claude account with its folder.
    static func session(_ s: TranscriptLog.Session, accounts: [ConnectedAccount]) -> OpenTarget {
        if s.account == Accounts.codexID { return .codex(session: s.id, cwd: s.cwd) }
        let dir = s.account == Accounts.id(folder: ".claude") ? nil : accounts.first { $0.id == s.account }?.path
        return .claude(session: s.id, cwd: s.cwd, configDir: dir)
    }

    /// The command that resumes the session in a Terminal.
    var command: String {
        switch place {
        case .t3Code: "open " + Self.quote("t3code://threads/" + id)
        case .codex: "codex resume " + Self.quote(id)
        case .claude: (configDir.map { "CLAUDE_CONFIG_DIR=" + Self.quote($0) + " " } ?? "") + "claude --resume " + Self.quote(id)
        }
    }

    /// The `.command` file: into the project folder, then the command.
    var script: String {
        "#!/bin/sh\n" + (cwd.map { "cd " + Self.quote($0) + " || exit 1\n" } ?? "") + command + "\n"
    }

    /// One shell word, in single quotes.
    static func quote(_ s: String) -> String { "'" + s.replacingOccurrences(of: "'", with: "'\\''") + "'" }
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
                               endedAgo: ended(s), session: .session(s, accounts: accounts))
        }
        // one T3 thread can hold several sessions (a resumed or handed-off conversation): one line
        var merged: [String: TokenSource] = [:]
        for s in out {
            if let m = merged[s.id] {
                merged[s.id] = TokenSource(id: s.id, threadID: m.threadID, project: m.project, detail: m.detail, account: m.account, rate: m.rate + s.rate,
                                           agents: m.agents ?? s.agents,
                                           endedAgo: m.endedAgo.flatMap { a in s.endedAgo.map { min(a, $0) } }, session: m.session)
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
