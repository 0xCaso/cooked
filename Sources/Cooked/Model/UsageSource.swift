import Foundation

/// Where the live island reads from: T3 Code's provider logs (the limits), its database and the Claude transcripts.
struct UsageSource: Sendable {
    /// The limits; keeps its read positions and samples across readings.
    var limits = LimitLog()
    var databasePath = TokenLog.defaultPath
    /// Adds up the calls of running turns across readings.
    let running = RunningTurns()
    /// The connected accounts, in display order, with their names.
    var accounts: [ConnectedAccount]
    /// The Claude accounts' minutes; nil reads only the database.
    var transcripts: TranscriptLog?
    /// The Codex account's minutes and sessions; nil reads them from the database.
    var codex: CodexLog?

    init(accounts: [ConnectedAccount] = AccountStore.current()) {
        self.accounts = accounts
        transcripts = TranscriptLog(accounts: accounts)
        codex = CodexLog(accounts: accounts)
    }

    struct Reading: Equatable {
        let snapshot: UsageSnapshot
        /// Wall-clock minute (Unix seconds / 60) of the last value in `snapshot.series`.
        let minute: Int
    }

    enum Problem: Error, Equatable {
        case noAccounts
        var message: String { "No limits in T3 Code's logs yet" }
    }

    func read(now: Date = Date()) -> Result<Reading, Problem> {
        // Without the database the island still has the percentages; only the histogram stays empty.
        let log = TokenLog(path: databasePath)
        let status = limits.read(now: now, database: log)
        var minutes = (try? log.read(now: now, minutes: LiveSnapshot.historyMinutes, running: running)) ?? TokenLog.Minutes()
        // The transcripts have every call of the Claude accounts, subagents included: they replace the database there.
        var calls = transcripts?.read(now: now, minutes: LiveSnapshot.historyMinutes) ?? TranscriptLog.Reading()
        // The rollouts have every Codex call, from the CLI and the Codex app too: they replace the database there.
        if let codex, FileManager.default.fileExists(atPath: codex.home + "/sessions") {
            let rollouts = codex.read(now: now, minutes: LiveSnapshot.historyMinutes)
            calls.minutes[codex.id] = rollouts.minutes
            calls.sessions += rollouts.sessions
        }
        for (id, perMinute) in calls.minutes { minutes.byAccount[id] = perMinute }
        let sources = TokenSources.make(
            sessions: calls.sessions, threads: (try? log.threads(sessions: calls.sessions.map(\.id))) ?? [:],
            recent: minutes.recentThreads, transcriptAccounts: Set(calls.minutes.keys), accounts: accounts)
        guard let snapshot = LiveSnapshot.make(status: status, minutes: minutes, now: now, sources: sources, accounts: accounts) else {
            return .failure(.noAccounts)
        }
        return .success(Reading(snapshot: snapshot, minute: Int(now.timeIntervalSince1970) / 60))
    }
}
