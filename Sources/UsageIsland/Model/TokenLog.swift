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

        static func == (a: Minutes, b: Minutes) -> Bool {
            a.byAccount == b.byAccount
                && a.topThread.mapValues { "\($0.title)|\($0.tokens)" } == b.topThread.mapValues { "\($0.title)|\($0.tokens)" }
        }
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
    func read(now: Date, minutes: Int = 60) throws -> Minutes {
        var db: OpaquePointer?
        guard sqlite3_open_v2("file:\(path)?mode=ro", &db, SQLITE_OPEN_READONLY | SQLITE_OPEN_URI, nil) == SQLITE_OK else {
            sqlite3_close(db)
            throw Failure.open(path)
        }
        defer { sqlite3_close(db) }
        sqlite3_busy_timeout(db, 2000)

        let minuteNow = Int(now.timeIntervalSince1970) / 60
        var out = Minutes()
        let perMinute = """
            SELECT COALESCE(pth.provider_instance_id, pth.provider), CAST(strftime('%s', pt.completed_at) AS INTEGER) / 60, SUM(\(Self.fresh))
            \(Self.from) WHERE pt.completed_at >= ? GROUP BY 1, 2
            """
        try query(db, perMinute, since: Double((minuteNow - minutes + 1) * 60)) { row in
            guard let id = row.text(0) else { return }
            out.byAccount[id, default: [:]][Int(row.int(1))] = Double(row.int(2))
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
        return out
    }

    enum Failure: Error { case open(String), query(String) }

    private struct Row {
        let stmt: OpaquePointer
        func text(_ i: Int32) -> String? { sqlite3_column_text(stmt, i).map { String(cString: $0) } }
        func int(_ i: Int32) -> Int64 { sqlite3_column_int64(stmt, i) }
    }

    private func query(_ db: OpaquePointer?, _ sql: String, since: Double, row: (Row) -> Void) throws {
        var stmt: OpaquePointer?
        guard sqlite3_prepare_v2(db, sql, -1, &stmt, nil) == SQLITE_OK, let stmt else {
            throw Failure.query(String(cString: sqlite3_errmsg(db)))
        }
        defer { sqlite3_finalize(stmt) }
        let iso = Self.iso.string(from: Date(timeIntervalSince1970: since))
        sqlite3_bind_text(stmt, 1, iso, -1, unsafeBitCast(-1, to: sqlite3_destructor_type.self))
        while sqlite3_step(stmt) == SQLITE_ROW { row(Row(stmt: stmt)) }
    }

    /// The same text form as `completed_at`, so the comparison in SQL is a plain string comparison.
    private nonisolated(unsafe) static let iso: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()
}
