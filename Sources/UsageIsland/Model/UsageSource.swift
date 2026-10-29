import Foundation

/// Where the live island reads from: the watcher's status.json and T3 Code's database.
struct UsageSource: Sendable {
    var statusPath = NSHomeDirectory() + "/.t3/usage-watch/status.json"
    var databasePath = TokenLog.defaultPath
    /// Adds up the calls of running turns across readings.
    let running = RunningTurns()
    /// The Claude accounts' minutes; nil reads only the database.
    var transcripts: TranscriptLog? = .standard()
    /// status.json older than this means the watcher has stopped (it runs every 30 s).
    static let staleAfter: TimeInterval = 5 * 60

    struct Reading: Equatable {
        let snapshot: UsageSnapshot
        /// Wall-clock minute (Unix seconds / 60) of the last value in `snapshot.series`.
        let minute: Int
    }

    enum Problem: Error, Equatable {
        case noStatus, watcherStopped, noAccounts
        var message: String {
            switch self {
            case .noStatus: "No status from usage-watch"
            case .watcherStopped: "usage-watch has stopped"
            case .noAccounts: "No account data yet"
            }
        }
    }

    func read(now: Date = Date()) -> Result<Reading, Problem> {
        let url = URL(fileURLWithPath: statusPath)
        guard let data = try? Data(contentsOf: url), let status = try? StatusFile.decode(data) else {
            return .failure(.noStatus)
        }
        let modified = (try? url.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate ?? now
        guard now.timeIntervalSince(modified) < Self.staleAfter else { return .failure(.watcherStopped) }
        // Without the database the island still has the percentages; only the histogram stays empty.
        var minutes = (try? TokenLog(path: databasePath).read(now: now, running: running)) ?? TokenLog.Minutes()
        // The transcripts have every call of the Claude accounts, subagents included: they replace the database there.
        for (id, perMinute) in transcripts?.read(now: now) ?? [:] { minutes.byAccount[id] = perMinute }
        guard let snapshot = LiveSnapshot.make(status: status, minutes: minutes, now: now) else {
            return .failure(.noAccounts)
        }
        return .success(Reading(snapshot: snapshot, minute: Int(now.timeIntervalSince1970) / 60))
    }
}
