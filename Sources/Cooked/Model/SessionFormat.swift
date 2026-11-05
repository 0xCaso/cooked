import Foundation

/// Which session file format a file has. Claude Code and Codex do not document their files, so a reader first checks
/// that the lines are ones it knows: a file with lines but none known means the format changed, which the app shows
/// as "format changed", never as 0 tokens.
enum SessionFormat: Equatable {
    case claude, codex, empty, formatChanged

    /// Claude transcript line kinds that every session has; other kinds (attachment, mode, ...) come and go.
    static let claudeKinds: Set<String> = ["user", "assistant", "ai-title", "custom-title", "last-prompt", "summary"]

    private struct Line: Decodable {
        struct Payload: Decodable { let type: String? }
        let type: String?
        let sessionId: String?
        let payload: Payload?
    }

    static func check(_ lines: some Sequence<some DataProtocol>) -> SessionFormat {
        var any = false
        for raw in lines {
            let data = Data(raw)
            guard !data.allSatisfy({ $0 == UInt8(ascii: " ") }) else { continue }
            any = true
            guard let line = try? JSONDecoder().decode(Line.self, from: data), let type = line.type else { continue }
            if type == "session_meta" || (type == "event_msg" && line.payload?.type == "token_count") { return .codex }
            if claudeKinds.contains(type), line.sessionId != nil { return .claude }
        }
        return any ? .formatChanged : .empty
    }

    /// Checks the first `limit` bytes of a file, so a long transcript costs one read.
    static func check(file: URL, limit: Int = 256 * 1024) -> SessionFormat {
        guard let handle = try? FileHandle(forReadingFrom: file) else { return .empty }
        defer { try? handle.close() }
        guard let data = try? handle.read(upToCount: limit), !data.isEmpty else { return .empty }
        var lines = data.split(separator: UInt8(ascii: "\n"))
        if data.count == limit, data.last != UInt8(ascii: "\n"), lines.count > 1 { lines.removeLast() } // cut line
        return check(lines)
    }

    /// The newest file's path when its format changed, else nil. `cache` keeps each file's answer, except "empty":
    /// a new file has no lines yet.
    static func newestChanged(_ urls: [(url: URL, modified: Date)], cache: inout [String: SessionFormat]) -> String? {
        guard let newest = urls.max(by: { $0.modified < $1.modified })?.url else { return nil }
        let format = cache[newest.path] ?? check(file: newest)
        cache = format == .empty ? [:] : [newest.path: format]
        return format == .formatChanged ? newest.path : nil
    }
}
