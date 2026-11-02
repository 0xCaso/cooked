import Foundation

/// Where the live island reads from: the session files of the connected accounts (tokens per minute, sessions and the
/// Codex percent), the status line bridge files (the Claude percent) and the minute cache (the alert thresholds).
/// T3 Code adds thread titles when its database exists; until the bridge is connected, its logs give the Claude percent.
struct UsageSource: Sendable {
    /// The percents; keeps their samples across readings.
    var limits = LimitLog(store: LimitLog.defaultStore)
    var bridgeDir = StatusLineBridge.defaultDir
    /// T3 Code's database: thread titles only. No number depends on it.
    var t3Database = TokenLog.defaultPath
    /// The connected accounts, in display order, with their names.
    var accounts: [ConnectedAccount]
    /// The Claude accounts' minutes and sessions.
    var transcripts: TranscriptLog?
    /// The Codex account's minutes, sessions and percent.
    var codex: CodexLog?

    /// The minute cache both readers fill.
    var ledger: MinuteLedger

    init(accounts: [ConnectedAccount] = AccountStore.current(), ledger: MinuteLedger = MinuteLedger(url: MinuteLedger.defaultURL)) {
        self.accounts = accounts
        self.ledger = ledger
        transcripts = TranscriptLog(accounts: accounts, ledger: ledger)
        codex = CodexLog(accounts: accounts, ledger: ledger)
    }

    struct Reading: Equatable {
        let snapshot: UsageSnapshot
        /// Wall-clock minute (Unix seconds / 60) of the last value in `snapshot.series`.
        let minute: Int
        /// 0...1 while the first launch reads 14 days of history; nil when complete.
        var history: Double? = nil
    }

    enum Problem: Error, Equatable {
        case noAccounts
        var message: String { "No 5-hour percent from any account yet" }
    }

    func read(now: Date = Date()) -> Result<Reading, Problem> {
        ledger.begin()
        var calls = transcripts?.read(now: now, minutes: LiveSnapshot.historyMinutes) ?? TranscriptLog.Reading()
        var samples = StatusLineBridge.samples(dir: bridgeDir)
        if let codex, FileManager.default.fileExists(atPath: codex.home + "/sessions") {
            let rollouts = codex.read(now: now, minutes: LiveSnapshot.historyMinutes)
            calls.minutes[codex.id] = rollouts.minutes
            calls.sessions += rollouts.sessions
            if let sample = rollouts.limits?.sample { samples[codex.id] = sample }
        }
        ledger.end(now: now)
        let history = ledger.historyProgress
        let database = FileManager.default.fileExists(atPath: t3Database) ? TokenLog(path: t3Database) : nil
        let status = limits.read(now: now, samples: samples, buckets: ledger.buckets15, fresh: history != nil, database: database)
        let sources = TokenSources.make(sessions: calls.sessions,
                                        threads: (try? database?.threads(sessions: calls.sessions.map(\.id))) ?? [:], accounts: accounts)
        guard let snapshot = LiveSnapshot.make(status: status, minutes: calls.minutes, now: now, sources: sources, accounts: accounts) else {
            return .failure(.noAccounts)
        }
        return .success(Reading(snapshot: snapshot, minute: Int(now.timeIntervalSince1970) / 60, history: history))
    }
}
