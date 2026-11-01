import Foundation

/// New tokens per minute, the 5 h and week percent, and the sessions of a Codex account, read from its rollouts
/// (`<codex home>/sessions/<year>/<month>/<day>/rollout-<time>-<id>.jsonl`). Codex writes a `token_count` event after
/// each call, with that call's usage (`last_token_usage`) and the account's limits (`rate_limits`). Rollouts from the
/// Codex CLI, the Codex app and T3 Code are all written here. "New" counts like the T3 Code database:
/// input − cached input + output.
/// Keeps each file's read position between readings, so a reading only parses the lines added since the last one.
final class CodexLog: @unchecked Sendable {
    /// The account id (`codex`) and its folder (`~/.codex`).
    let id: String
    let home: String

    /// The limits of the newest `token_count` that has them.
    struct Limits: Equatable {
        struct Window: Equatable {
            /// 0...100.
            let usedPercent: Double
            let resetsAt: Date?
        }
        var fiveHour: Window?
        var week: Window?
        /// When Codex wrote them.
        let at: Date
    }

    struct Reading {
        /// Minute → new tokens.
        var minutes: [Int: Double] = [:]
        /// Sessions with tokens in the last 15 minutes, most tokens first.
        var sessions: [TranscriptLog.Session] = []
        var limits: Limits?
    }

    private struct File {
        var offset: UInt64 = 0
        /// Event time (ISO string) → (minute, new tokens).
        var calls: [String: (minute: Int, tokens: Double)] = [:]
        /// The totals of the last `token_count`: Codex sometimes writes the same event twice.
        var lastTotal: Usage?
        var limits: Limits?
    }

    private let lock = NSLock()
    private var files: [String: File] = [:]
    /// Rollout path → (cwd, first prompt), from its first lines, read once.
    private var heads: [String: (cwd: String?, prompt: String?)] = [:]
    /// Thread names from `session_index.jsonl`, and the index's size when read.
    private var names: (size: UInt64, byID: [String: String]) = (0, [:])
    private var limits: Limits?

    init(id: String = Accounts.codexID, home: String) {
        self.id = id
        self.home = home
    }

    /// The connected Codex account, if any.
    convenience init?(accounts: [ConnectedAccount]) {
        guard let codex = accounts.first(where: { $0.kind == .codex }) else { return nil }
        self.init(id: codex.id, home: codex.path)
    }

    func read(now: Date, minutes: Int = 60) -> Reading {
        lock.lock()
        defer { lock.unlock() }
        let minuteNow = Int(now.timeIntervalSince1970) / 60
        let first = minuteNow - minutes + 1
        let since = now.addingTimeInterval(-Double(minutes + 1) * 60)
        var out = Reading()
        var seen = Set<String>()
        var sessions: [TranscriptLog.Session] = []
        let root = URL(fileURLWithPath: home + "/sessions")
        let walk = FileManager.default.enumerator(at: root, includingPropertiesForKeys: [.contentModificationDateKey],
                                                  options: [.skipsHiddenFiles])
        while let url = walk?.nextObject() as? URL {
            guard url.pathExtension == "jsonl", url.lastPathComponent.hasPrefix("rollout-"),
                  let modified = try? url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate,
                  modified >= since else { continue }
            seen.insert(url.path)
            var file = files[url.path] ?? File(offset: TranscriptLog.start(of: url, after: since))
            Self.readNew(url, into: &file)
            if let l = file.limits, l.at > limits?.at ?? .distantPast { limits = l }
            var recent = 0.0, active = false
            for (minute, tokens) in file.calls.values where minute >= first && minute <= minuteNow {
                out.minutes[minute, default: 0] += tokens
                if minute > minuteNow - 15 { recent += tokens }
                if minute > minuteNow - 5 { active = true }
            }
            if recent > 0 {
                let head = heads[url.path] ?? Self.head(of: url)
                heads[url.path] = head
                let sid = Self.session(of: url)
                sessions.append(.init(account: id, id: sid, cwd: head.cwd, tokens: recent, mainActive: active, activeSubagents: 0,
                                      name: threadName(sid) ?? head.prompt))
            }
            file.calls = file.calls.filter { $0.value.minute >= first }
            files[url.path] = file
        }
        files = files.filter { seen.contains($0.key) }
        heads = heads.filter { seen.contains($0.key) }
        out.sessions = sessions.sorted { ($0.tokens, $0.id) > ($1.tokens, $1.id) }
        out.limits = limits
        return out
    }

    /// `rollout-2026-10-20T09-05-37-01a15821-434f-....jsonl` → the thread id, the last 36 characters.
    static func session(of url: URL) -> String {
        String(url.deletingPathExtension().lastPathComponent.suffix(36))
    }

    /// The thread's name in `session_index.jsonl`, the newest line for that id; re-read when the index grows.
    private func threadName(_ sid: String) -> String? {
        let path = home + "/session_index.jsonl"
        let size = ((try? FileManager.default.attributesOfItem(atPath: path))?[.size] as? NSNumber)?.uint64Value ?? 0
        if size != names.size {
            names = (size, Self.threadNames(in: (try? Data(contentsOf: URL(fileURLWithPath: path))) ?? Data()))
        }
        return names.byID[sid]
    }

    static func threadNames(in index: Data) -> [String: String] {
        struct Line: Decodable { let id: String?; let thread_name: String? }
        var out: [String: String] = [:]
        for line in index.split(separator: UInt8(ascii: "\n")) {
            guard let l = try? JSONDecoder().decode(Line.self, from: Data(line)), let id = l.id,
                  let name = TranscriptLog.clean(l.thread_name) else { continue }
            out[id] = name // lines are in time order: the last one wins
        }
        return out
    }

    // MARK: Lines

    struct Usage: Decodable, Equatable {
        let input_tokens: Double?
        let cached_input_tokens: Double?
        let output_tokens: Double?
        let total_tokens: Double?
        /// input − cached + output.
        var new: Double { max(0, (input_tokens ?? 0) - (cached_input_tokens ?? 0)) + (output_tokens ?? 0) }
    }

    private struct Line: Decodable {
        struct Payload: Decodable {
            struct Info: Decodable {
                let total_token_usage: Usage?
                let last_token_usage: Usage?
            }
            struct RateLimits: Decodable {
                struct Window: Decodable {
                    let used_percent: Double?
                    let window_minutes: Int?
                    let resets_at: Double?
                }
                let primary: Window?
                let secondary: Window?
            }
            let type: String?
            let info: Info?
            let rate_limits: RateLimits?
            // session_meta and user_message
            let cwd: String?
            let message: String?
        }
        let timestamp: String?
        let type: String?
        let payload: Payload?
    }

    /// One `token_count` line → its time, new tokens, totals and limits.
    struct Count: Equatable {
        let at: Date
        let stamp: String
        let tokens: Double
        let total: Usage?
        let limits: Limits?
    }

    static func parse(_ line: Data) -> Count? {
        guard line.range(of: Data(#""token_count""#.utf8)) != nil,
              let l = try? JSONDecoder().decode(Line.self, from: line), let p = l.payload, p.type == "token_count",
              let stamp = l.timestamp, let at = iso.date(from: stamp) else { return nil }
        var limits: Limits?
        if let r = p.rate_limits {
            var l = Limits(at: at)
            for w in [r.primary, r.secondary].compactMap({ $0 }) {
                let window = Limits.Window(usedPercent: w.used_percent ?? 0, resetsAt: w.resets_at.map { Date(timeIntervalSince1970: $0) })
                switch w.window_minutes {
                case 300: l.fiveHour = window
                case 10080: l.week = window
                default: break // other plans' windows (a free plan has 30 days) are not shown
                }
            }
            limits = l
        }
        return Count(at: at, stamp: stamp, tokens: p.info?.last_token_usage?.new ?? 0, total: p.info?.total_token_usage, limits: limits)
    }

    private static func readNew(_ url: URL, into file: inout File) {
        guard let handle = try? FileHandle(forReadingFrom: url) else { return }
        defer { try? handle.close() }
        guard let end = try? handle.seekToEnd() else { return }
        if end < file.offset { file = File() } // rewritten
        guard end > file.offset, (try? handle.seek(toOffset: file.offset)) != nil,
              let data = try? handle.readToEnd(), let last = data.lastIndex(of: UInt8(ascii: "\n")) else { return }
        for line in data[data.startIndex...last].split(separator: UInt8(ascii: "\n")) {
            guard let count = parse(Data(line)) else { continue }
            if let limits = count.limits { file.limits = limits }
            // the same totals again: a repeated event, not a new call
            if let total = count.total, total == file.lastTotal { continue }
            file.lastTotal = count.total
            if count.tokens > 0 { file.calls[count.stamp] = (Int(count.at.timeIntervalSince1970) / 60, count.tokens) }
        }
        file.offset += UInt64(last - data.startIndex + 1)
    }

    /// The working folder (`session_meta`) and the first prompt (`user_message`), from the first 256 KB.
    static func head(of url: URL) -> (cwd: String?, prompt: String?) {
        guard let handle = try? FileHandle(forReadingFrom: url) else { return (nil, nil) }
        defer { try? handle.close() }
        guard let data = try? handle.read(upToCount: 256 * 1024) else { return (nil, nil) }
        var cwd: String?, prompt: String?
        for line in data.split(separator: UInt8(ascii: "\n")) {
            let line = Data(line)
            guard (cwd == nil && line.range(of: Data(#""session_meta""#.utf8)) != nil)
                    || (prompt == nil && line.range(of: Data(#""user_message""#.utf8)) != nil),
                  let l = try? JSONDecoder().decode(Line.self, from: line) else { continue }
            if l.type == "session_meta" { cwd = l.payload?.cwd }
            if l.payload?.type == "user_message", let m = l.payload?.message, !m.hasPrefix("<") { prompt = TranscriptLog.clean(m) }
            if cwd != nil && prompt != nil { break }
        }
        return (cwd, prompt)
    }

    private nonisolated(unsafe) static let iso: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()
}
