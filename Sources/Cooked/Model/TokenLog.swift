import Foundation
import SQLite3

/// T3 Code's own database, read-only: the T3 Code extra. It names the sessions T3 Code started by their thread, and
/// gives the account of a thread for T3 Code's limit logs. No number of the island comes from it.
struct TokenLog {
    let path: String

    static let defaultPath = NSHomeDirectory() + "/.t3/userdata/statev2.sqlite"

    /// A T3 Code thread and its project, as the app names them.
    struct Thread: Equatable {
        let id: String
        let title: String
        /// The project's title; nil for "No project" (T3's scratch folder).
        let project: String?
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

    /// T3 Code thread id → its account (`providerInstanceId`), for rate-limit events that do not name it.
    func threadAccounts() throws -> [String: String] {
        let db = try open()
        defer { sqlite3_close(db) }
        var out: [String: String] = [:]
        try query(db, "SELECT thread_id, json_extract(payload_json,'$.providerInstanceId') FROM orchestration_v2_projection_threads",
                  since: nil) { row in
            if let id = row.text(0), let account = row.text(1) { out[id] = account }
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

    enum Failure: Error { case open(String), query(String) }

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
