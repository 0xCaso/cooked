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

        /// As a limit sample; nil without a 5 h window (a free plan).
        var sample: LimitLog.Sample? {
            guard let fiveHour else { return nil }
            return .init(at: at.timeIntervalSince1970, five: fiveHour.usedPercent / 100,
                         fiveReset: fiveHour.resetsAt?.timeIntervalSince1970, week: (week?.usedPercent ?? 0) / 100,
                         weekReset: week?.resetsAt?.timeIntervalSince1970, status: fiveHour.usedPercent >= 100 ? "rejected" : "allowed")
        }
    }

    struct Reading {
        /// Minute → new tokens.
        var minutes: [Int: Double] = [:]
        /// Sessions with tokens in the last 15 minutes, most tokens first.
        var sessions: [TranscriptLog.Session] = []
        var limits: Limits?
    }

    private let lock = NSLock()
    /// Rollout path → (cwd, first prompt), from its first lines, read once.
    private var heads: [String: (cwd: String?, prompt: String?)] = [:]
    /// Thread names from `session_index.jsonl`, and the index's size when read.
    private var names: (size: UInt64, byID: [String: String]) = (0, [:])
    private var limits: Limits?

    /// The minute cache, where the tokens go. A ledger the reader made itself is begun and ended by `read`.
    let ledger: MinuteLedger
    private let ownsLedger: Bool

    init(id: String = Accounts.codexID, home: String, ledger: MinuteLedger? = nil) {
        self.id = id
        self.home = home
        self.ledger = ledger ?? MinuteLedger()
        ownsLedger = ledger == nil
    }

    /// The connected Codex account, if any.
    convenience init?(accounts: [ConnectedAccount], ledger: MinuteLedger? = nil) {
        guard let codex = accounts.first(where: { $0.kind == .codex }) else { return nil }
        self.init(id: codex.id, home: codex.path, ledger: ledger)
    }

    /// Minute → new tokens for the `minutes` minutes up to `now` (from the minute cache), the sessions of the last
    /// 15 minutes and the newest limits. Walks the rollouts written in the last 14 days, newest first.
    func read(now: Date, minutes: Int = 60) -> Reading {
        lock.lock()
        defer { lock.unlock() }
        if ownsLedger { ledger.begin() }
        defer { if ownsLedger { ledger.end(now: now) } }
        let minuteNow = Int(now.timeIntervalSince1970) / 60
        let since = MinuteLedger.since(now)
        var out = Reading()
        var seen = Set<String>()
        var sessions: [TranscriptLog.Session] = []
        var urls: [(url: URL, modified: Date)] = []
        let walk = FileManager.default.enumerator(at: URL(fileURLWithPath: home + "/sessions"),
                                                  includingPropertiesForKeys: [.contentModificationDateKey], options: [.skipsHiddenFiles])
        while let url = walk?.nextObject() as? URL {
            guard url.pathExtension == "jsonl", url.lastPathComponent.hasPrefix("rollout-"),
                  let modified = try? url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate,
                  modified >= since else { continue }
            urls.append((url, modified))
        }
        for (url, _) in urls.sorted(by: { $0.modified > $1.modified }) {
            seen.insert(url.path)
            let mark = ledger.read(url, now: now) { line, mark in
                guard let count = Self.parse(line) else { return }
                if let l = count.limits, l.at > limits?.at ?? .distantPast { limits = l }
                // the same totals again: a repeated event, not a new call
                if let total = count.total.map(Self.key) {
                    guard total != mark.last else { return }
                    mark.last = total
                }
                let minute = Int(count.at.timeIntervalSince1970) / 60
                guard count.tokens > 0 else { return }
                mark.recent[minute, default: 0] += count.tokens
                ledger.add(id, minute: minute, tokens: count.tokens)
            }
            var recent = 0.0, active = false
            for (minute, tokens) in mark.recent where minute <= minuteNow {
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
        }
        // after a restart the files continue from their marks: the newest rollout's last limits
        if limits == nil, let newest = urls.max(by: { $0.modified < $1.modified }) { limits = Self.lastLimits(of: newest.url) }
        heads = heads.filter { seen.contains($0.key) }
        out.minutes = ledger.minutes(id, from: minuteNow - minutes + 1, to: minuteNow)
        out.sessions = sessions.sorted { ($0.tokens, $0.id) > ($1.tokens, $1.id) }
        out.limits = limits
        return out
    }

    /// The newest limits in the last 256 KB of a rollout.
    static func lastLimits(of url: URL) -> Limits? {
        guard let handle = try? FileHandle(forReadingFrom: url), let end = try? handle.seekToEnd() else { return nil }
        defer { try? handle.close() }
        let from = end > 256 * 1024 ? end - 256 * 1024 : 0
        guard (try? handle.seek(toOffset: from)) != nil, let data = try? handle.readToEnd() else { return nil }
        return data.split(separator: UInt8(ascii: "\n")).reversed().lazy.compactMap { parse(Data($0))?.limits }.first
    }

    /// A repeated event has the same totals.
    static func key(_ u: Usage) -> String {
        "\(u.input_tokens ?? 0)/\(u.cached_input_tokens ?? 0)/\(u.output_tokens ?? 0)/\(u.total_tokens ?? 0)"
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
