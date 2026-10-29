import Foundation
import SQLite3

/// New tokens per minute and account, read from T3 Code's own database, read-only.
/// "New" counts the way the watcher does: input − cached input + output, per completed turn.
struct TokenLog {
    let path: String

    static let defaultPath = NSHomeDirectory() + "/.t3/userdata/statev2.sqlite"

    struct Minutes: Equatable {
        /// Account id → minute (Unix seconds / 60) → new tokens.
        var byAccount: [String: [Int: Double]] = [:]
        /// Account id → the thread with the most new tokens in the last 5 minutes.
        var topThread: [String: (title: String, tokens: Double)] = [:]
        /// Threads with turns finished in the last 15 minutes, for the accounts without transcripts (Codex).
        var recentThreads: [RecentThread] = []

        static func == (a: Minutes, b: Minutes) -> Bool {
            a.byAccount == b.byAccount && a.recentThreads == b.recentThreads
                && a.topThread.mapValues { "\($0.title)|\($0.tokens)" } == b.topThread.mapValues { "\($0.title)|\($0.tokens)" }
        }
    }

    /// A T3 Code thread and its project, as the app names them.
    struct Thread: Equatable {
        let id: String
        let title: String
        /// The project's title; nil for "No project" (T3's scratch folder).
        let project: String?
    }

    struct RecentThread: Equatable {
        let account: String
        let thread: Thread
        let tokens: Double
    }

    private static let threadColumns = """
        t.thread_id, t.title, NULLIF(p.title, 'No project')
        """
    private static func threadJoin(on threadID: String) -> String {
        """
        LEFT JOIN orchestration_v2_projection_threads t ON t.thread_id = \(threadID)
        LEFT JOIN projection_projects p ON p.project_id = t.project_id
        """
    }

    /// The T3 Code threads of these Claude sessions (`nativeThreadRef.nativeId`), by session id.
    func threads(sessions ids: [String]) throws -> [String: Thread] {
        guard !ids.isEmpty else { return [:] }
        let db = try open()
        defer { sqlite3_close(db) }
        let list = ids.map { "'\($0.replacingOccurrences(of: "'", with: "''"))'" }.joined(separator: ",")
        let sql = """
            SELECT json_extract(pth.payload_json,'$.nativeThreadRef.nativeId') AS native, \(Self.threadColumns)
            FROM orchestration_v2_projection_provider_threads pth \(Self.threadJoin(on: "pth.thread_id"))
            WHERE native IN (\(list)) AND t.thread_id IS NOT NULL
            """
        var out: [String: Thread] = [:]
        try query(db, sql, since: nil) { row in
            guard let native = row.text(0), let id = row.text(1), let title = row.text(2) else { return }
            out[native] = Thread(id: id, title: title, project: row.text(3))
        }
        return out
    }

    private func open() throws -> OpaquePointer? {
        var db: OpaquePointer?
        guard sqlite3_open_v2("file:\(path)?mode=ro", &db, SQLITE_OPEN_READONLY | SQLITE_OPEN_URI, nil) == SQLITE_OK else {
            sqlite3_close(db)
            throw Failure.open(path)
        }
        sqlite3_busy_timeout(db, 2000)
        return db
    }

    private static let fresh = """
        COALESCE(json_extract(pt.payload_json,'$.turnTokenUsage.inputTokens'),0)
        - COALESCE(json_extract(pt.payload_json,'$.turnTokenUsage.cachedInputTokens'),0)
        + COALESCE(json_extract(pt.payload_json,'$.turnTokenUsage.outputTokens'),0)
        """
    private static let from = """
        FROM orchestration_v2_projection_provider_turns pt
        LEFT JOIN orchestration_v2_projection_provider_threads pth USING(provider_thread_id)
        """

    /// The last `minutes` minutes up to `now`. Throws when the database cannot be opened.
    /// A finished turn's tokens are spread evenly over the minutes it ran, because T3 Code only writes them when
    /// the turn ends. Turns still running come from `running`, which adds up their calls between readings.
    func read(now: Date, minutes: Int = 60, running: RunningTurns? = nil) throws -> Minutes {
        let db = try open()
        defer { sqlite3_close(db) }

        let minuteNow = Int(now.timeIntervalSince1970) / 60
        let windowStart = Double((minuteNow - minutes + 1) * 60)
        var out = Minutes()
        let finished = """
            SELECT COALESCE(pth.provider_instance_id, pth.provider),
              CAST(strftime('%s', COALESCE(pt.started_at, pt.completed_at)) AS INTEGER),
              CAST(strftime('%s', pt.completed_at) AS INTEGER), \(Self.fresh)
            \(Self.from) WHERE pt.completed_at >= ?
            """
        try query(db, finished, since: windowStart) { row in
            guard let id = row.text(0) else { return }
            for (minute, tokens) in Self.spread(Double(row.int(3)), from: Double(row.int(1)), to: Double(row.int(2)))
            where minute >= minuteNow - minutes + 1 && minute <= minuteNow {
                out.byAccount[id, default: [:]][minute, default: 0] += tokens
            }
        }
        if let running {
            var samples: [RunningTurns.Sample] = []
            let live = """
                SELECT pt.provider_turn_id, COALESCE(pth.provider_instance_id, pth.provider),
                  COALESCE(json_extract(pt.payload_json,'$.tokenUsage.inputTokens'),0),
                  COALESCE(json_extract(pt.payload_json,'$.tokenUsage.cachedInputTokens'),0),
                  COALESCE(json_extract(pt.payload_json,'$.tokenUsage.outputTokens'),0),
                  json_extract(pt.payload_json,'$.tokenUsage.updatedAt')
                \(Self.from) WHERE pt.completed_at IS NULL AND pt.started_at >= ?
                """
            try query(db, live, since: now.timeIntervalSince1970 - 6 * 3600) { row in
                guard let turn = row.text(0), let id = row.text(1), let stamp = row.text(5),
                      let at = Self.iso.date(from: stamp) else { return }
                samples.append(.init(turn: turn, account: id, input: Double(row.int(2)), cached: Double(row.int(3)),
                                     output: Double(row.int(4)), at: at))
            }
            for (id, perMinute) in running.record(samples) {
                for (minute, tokens) in perMinute where minute >= minuteNow - minutes + 1 && minute <= minuteNow {
                    out.byAccount[id, default: [:]][minute, default: 0] += tokens
                }
            }
        }
        let top = """
            SELECT COALESCE(pth.provider_instance_id, pth.provider), t.title, SUM(\(Self.fresh)) AS n
            \(Self.from) LEFT JOIN orchestration_v2_projection_threads t ON t.thread_id = pt.thread_id
            WHERE pt.completed_at >= ? GROUP BY 1, pt.thread_id ORDER BY n DESC
            """
        try query(db, top, since: now.timeIntervalSince1970 - 300) { row in
            guard let id = row.text(0), let title = row.text(1), out.topThread[id] == nil else { return }
            out.topThread[id] = (title, Double(row.int(2)))
        }
        let recent = """
            SELECT COALESCE(pth.provider_instance_id, pth.provider), \(Self.threadColumns), SUM(\(Self.fresh)) AS n
            \(Self.from) \(Self.threadJoin(on: "pt.thread_id"))
            WHERE pt.completed_at >= ? AND t.thread_id IS NOT NULL GROUP BY 1, t.thread_id HAVING n > 0 ORDER BY n DESC
            """
        try query(db, recent, since: now.timeIntervalSince1970 - 900) { row in
            guard let account = row.text(0), let id = row.text(1), let title = row.text(2) else { return }
            out.recentThreads.append(RecentThread(account: account, thread: Thread(id: id, title: title, project: row.text(3)),
                                                  tokens: Double(row.int(4))))
        }
        return out
    }

    enum Failure: Error { case open(String), query(String) }

    /// `tokens` spread evenly over the seconds from `start` to `end`, per minute (Unix seconds / 60).
    static func spread(_ tokens: Double, from start: Double, to end: Double) -> [Int: Double] {
        guard tokens > 0 else { return [:] }
        guard end > start else { return [Int(end) / 60: tokens] }
        var out: [Int: Double] = [:]
        var t = start
        while t < end {
            let next = min(end, (t / 60).rounded(.down) * 60 + 60)
            out[Int(t) / 60, default: 0] += tokens * (next - t) / (end - start)
            t = next
        }
        return out
    }

    private struct Row {
        let stmt: OpaquePointer
        func text(_ i: Int32) -> String? { sqlite3_column_text(stmt, i).map { String(cString: $0) } }
        func int(_ i: Int32) -> Int64 { sqlite3_column_int64(stmt, i) }
    }

    /// `since` binds the query's one parameter; nil for a query without one.
    private func query(_ db: OpaquePointer?, _ sql: String, since: Double?, row: (Row) -> Void) throws {
        var stmt: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK, let stmt else {
            throw Failure.query(String(cString: sqlite3_errmsg(db)))
        }
        defer { sqlite3_finalize(stmt) }
        if let since {
            let iso = Self.iso.string(from: Date(timeIntervalSince1970: since))
            sqlite3_bind_text(stmt, 1, iso, -1, unsafeBitCast(-1, to: sqlite3_destructor_type.self))
        }
        while sqlite3_step(stmt) == SQLITE_ROW { row(Row(stmt: stmt)) }
    }

    /// The same text form as `completed_at`, so the comparison in SQL is a plain string comparison.
    private nonisolated(unsafe) static let iso: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()
}
