import Foundation

/// The accounts' 5 h and week percent, and the paces and alert thresholds built from them, in the form the watcher
/// script's status.json had. The samples come from:
/// - Claude: the status line bridge files (`StatusLineBridge`, terminal `claude` sessions) and T3 Code's provider logs
///   (`~/.t3/userdata/logs/provider/events.<thread>.log`, read-only: the Agent SDK runs no status line, and logs each
///   `rate_limit_event` when the percent changes). The newest sample of the two wins.
/// - Codex: the `rate_limits` of its rollouts (`CodexLog`).
/// The alert thresholds are the p99 of the 15-minute buckets in the minute cache.
/// Keeps each log file's read position and the samples of the last hour across readings.
final class LimitLog: @unchecked Sendable {
    /// T3 Code's provider logs; nil reads none.
    let dir: String?

    static let defaultDir = NSHomeDirectory() + "/.t3/userdata/logs/provider"
    static let claudeFallback15m = 500_000
    static let codexFallback15m = 350_000
    /// Files not written for this long hold no new events.
    static let fileMaxAge: TimeInterval = 6 * 3600
    /// The first reading after a launch looks back a week, the longest window: an account used days ago still has a
    /// week percent.
    static let firstReadMaxAge: TimeInterval = 7 * 86400
    /// A file seen for the first time is read from this far before its end.
    static let tail: UInt64 = 2 << 20
    /// Samples older than this before the last one are dropped: the paces look back 30 minutes at most.
    static let sampleKeep: TimeInterval = 3600
    /// An account with an event this recent is live.
    static let liveMaxAge: TimeInterval = 600
    /// No projection under this share used: right after a reset the pace says nothing.
    static let projectionMinimum = 0.20
    /// The paces: window, label, fewest samples. A pace needs samples over `coverage` of its window.
    static let paces: [(window: TimeInterval, label: String, minimum: Int)] = [(300, "5 min", 3), (1800, "30 min", 2)]
    static let coverage = 0.75
    /// An account needs this many 15-min buckets of history for its own p99.
    static let minimumBuckets = 30
    static let thresholdsMaxAge: TimeInterval = 3600

    struct Sample: Codable, Equatable {
        /// Unix seconds.
        let at: Double
        /// Share used, 0...1 (over 1 when blocked).
        let five: Double
        let fiveReset: Double?
        let week: Double
        let weekReset: Double?
        let status: String
    }

    /// 15-minute buckets of new tokens: each account's, and every account summed.
    struct Buckets: Equatable {
        var byAccount: [String: [Double]] = [:]
        var total: [Double] = []
    }

    private struct File { var offset: UInt64 }

    private let lock = NSLock()
    private var files: [String: File] = [:]
    /// From the bridge and the rollouts.
    private var samples: [String: [Sample]] = [:]
    /// From T3 Code's logs, joined with the samples above for a Claude account.
    private var logged: [String: [Sample]] = [:]
    private var thresholds: (at: Double, values: [String: Int], total: Int)?
    /// The newest sample of each account, kept across launches in `store`: an account with no new event keeps its
    /// last known percent (its 5 h reset or week reset already gone by shows as such, see `entry`).
    private var last: [String: Sample] = [:]
    let store: URL?
    private var firstRead = true

    /// `store` nil keeps the last samples in memory only (tests).
    init(dir: String? = LimitLog.defaultDir, store: URL? = nil) {
        self.dir = dir
        self.store = store
        if let store, let data = try? Data(contentsOf: store),
           let saved = try? JSONDecoder().decode([String: Sample].self, from: data) { last = saved }
    }

    static var defaultStore: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("Cooked/limits.json")
    }

    /// Adds this reading's samples, reads the new log events and builds every account's limits. `buckets` is called
    /// when the thresholds are due (hourly, or every reading while `fresh`: the cache is still filling). `database`
    /// gives the account of a T3 Code thread whose events do not name it.
    func read(now: Date, samples new: [String: Sample] = [:], buckets: () -> Buckets = { Buckets() }, fresh: Bool = false,
              database: TokenLog? = nil, calendar: Calendar = .current) -> StatusFile {
        lock.lock()
        defer { lock.unlock() }
        for (id, sample) in new { samples[id, default: []].append(sample) }
        if let dir { readLogs(dir, now: now, database: database) }
        samples = samples.mapValues(Self.prune)
        logged = logged.mapValues(Self.prune)

        let t = now.timeIntervalSince1970
        // A Claude account in the terminal and in T3 Code has both: one list, so the newest sample wins.
        var all = logged.filter { Self.isClaude($0.key) }
        for (id, s) in samples where !s.isEmpty { all[id] = Self.prune((all[id] ?? []) + s) }
        var changed = false
        for (id, s) in all {
            guard let newest = s.last, newest.at > last[id]?.at ?? -.infinity else { continue }
            last[id] = newest
            changed = true
        }
        for (id, sample) in last where all[id] == nil { all[id] = [sample] }
        if changed, let store, let data = try? JSONEncoder().encode(last) {
            try? FileManager.default.createDirectory(at: store.deletingLastPathComponent(), withIntermediateDirectories: true)
            try? data.write(to: store, options: .atomic)
        }
        if fresh || thresholds.map({ t - $0.at >= Self.thresholdsMaxAge }) ?? true {
            let b = buckets()
            let values = Self.thresholds(buckets: b.byAccount, ids: Set(all.keys))
            thresholds = (t, values, Self.totalThreshold(buckets: b.total, fallback: values.values.max() ?? Self.claudeFallback15m))
        }
        var accounts: [String: StatusFile.Entry] = [:]
        for (id, s) in all where !s.isEmpty {
            accounts[id] = Self.entry(id: id, samples: s, threshold: thresholds?.values[id], now: now, calendar: calendar)
        }
        return StatusFile(updatedAt: ISO8601DateFormatter().string(from: now), accounts: accounts,
                          totalThreshold15m: thresholds?.total)
    }

    private func readLogs(_ dir: String, now: Date, database: TokenLog?) {
        var threadAccounts: [String: String]?
        let names = (try? FileManager.default.contentsOfDirectory(atPath: dir)) ?? []
        var seen = Set<String>()
        let maxAge = firstRead ? Self.firstReadMaxAge : Self.fileMaxAge
        firstRead = false
        for name in names where name.hasPrefix("events.") && name.hasSuffix(".log") {
            let path = dir + "/" + name
            guard let modified = (try? FileManager.default.attributesOfItem(atPath: path))?[.modificationDate] as? Date,
                  now.timeIntervalSince(modified) < maxAge else { continue }
            seen.insert(path)
            let thread = String(name.dropFirst("events.".count).dropLast(".log".count))
            var file = files[path] ?? File(offset: Self.start(of: path))
            for (account, sample) in Self.readNew(path, into: &file) {
                let id = account ?? {
                    if threadAccounts == nil { threadAccounts = (try? database?.threadAccounts()) ?? [:] }
                    return threadAccounts?[thread]
                }()
                if let id { logged[id, default: []].append(sample) }
            }
            files[path] = file
        }
        files = files.filter { seen.contains($0.key) }
    }

    static func isClaude(_ id: String) -> Bool { id.hasPrefix("claudeAgent") }

    /// Sorted, one per time, only the current 5 h window (the last sample's reset), the last hour before the last one.
    static func prune(_ s: [Sample]) -> [Sample] {
        let sorted = Dictionary(s.map { ($0.at, $0) }, uniquingKeysWith: { _, b in b }).values.sorted { $0.at < $1.at }
        guard let last = sorted.last else { return [] }
        return sorted.filter { $0.fiveReset == last.fiveReset && $0.at >= last.at - sampleKeep }
    }

    /// The account's limits, as the watcher wrote them.
    static func entry(id: String, samples: [Sample], threshold: Int?, now: Date, calendar: Calendar) -> StatusFile.Entry {
        let last = samples[samples.count - 1]
        let t = now.timeIntervalSince1970
        let live = t - last.at <= liveMaxAge
        let reset = last.fiveReset
        let stale = reset.map { $0 <= t } ?? false
        var entry = StatusFile.Entry(
            name: id, tokensFresh: nil, spikeThreshold15m: threshold ?? (isClaude(id) ? claudeFallback15m : codexFallback15m),
            fiveHourPct: Int((last.five * 100).rounded()), fiveHourResetsAt: reset.map { clock($0, now, calendar) },
            sevenDayPct: Int((last.week * 100).rounded()), sevenDayResetsAt: last.weekReset.map { clock($0, now, calendar) },
            limitStatus: last.status, lastEventAt: clock(last.at, now, calendar), live: live, projection: nil,
            note: stale ? "5h window already reset, no new data" : nil)
        if live, let reset, !stale, last.five >= projectionMinimum {
            var best: (eta: Double, label: String)?
            for pace in paces {
                guard let slope = slope(samples, window: pace.window, minimum: pace.minimum), slope > 0 else { continue }
                let eta = last.at + (1 - last.five) / slope
                if eta < reset, eta < best?.eta ?? .infinity { best = (eta, pace.label) }
            }
            entry.projection = best.map { .init(window: $0.label, eta: clock($0.eta, now, calendar)) }
        }
        return entry
    }

    /// Share per second over the window before the last sample; nil with too few samples or under `coverage` of the
    /// window (the script took 7.5 minutes for "30 min" and saw the work account full at 13:57; it was at 37% at 13:44).
    static func slope(_ samples: [Sample], window: TimeInterval, minimum: Int) -> Double? {
        guard let last = samples.last else { return nil }
        let inside = samples.filter { $0.at >= last.at - window }
        guard inside.count >= minimum, let first = inside.first, last.at - first.at >= window * coverage else { return nil }
        return (last.five - first.five) / (last.at - first.at)
    }

    /// p99 of the summed 15-min buckets; with little history, `fallback` (the largest account threshold).
    static func totalThreshold(buckets: [Double], fallback: Int) -> Int {
        p99(buckets) ?? fallback
    }

    static func p99(_ values: [Double]) -> Int? {
        guard values.count >= minimumBuckets else { return nil }
        let sorted = values.sorted()
        return Int(sorted[min(sorted.count - 1, sorted.count * 99 / 100)])
    }

    /// p99 of each account's 15-min buckets. An account with little history takes the largest p99 of the other Claude
    /// accounts (the same person's pace), else a fixed fallback: the team account sat at the fallback on 30/10.
    static func thresholds(buckets: [String: [Double]], ids: Set<String> = []) -> [String: Int] {
        var own: [String: Int] = [:]
        for (id, values) in buckets { own[id] = p99(values) }
        let claude = Set(buckets.keys).union(ids).filter(isClaude)
        var out: [String: Int] = [:]
        for id in Set(buckets.keys).union(ids) {
            let others = isClaude(id) ? claude.filter { $0 != id }.compactMap { own[$0] }.max() : nil
            out[id] = own[id] ?? others ?? (isClaude(id) ? claudeFallback15m : codexFallback15m)
        }
        return out
    }

    /// "HH:MM" on `now`'s day, else "dd/mm HH:MM", local time (`WatcherTime` reads it back).
    static func clock(_ t: Double, _ now: Date, _ calendar: Calendar) -> String {
        let d = Date(timeIntervalSince1970: t)
        let c = calendar.dateComponents([.day, .month, .hour, .minute], from: d)
        let hm = String(format: "%02d:%02d", c.hour ?? 0, c.minute ?? 0)
        return calendar.isDate(d, inSameDayAs: now) ? hm : String(format: "%02d/%02d ", c.day ?? 0, c.month ?? 0) + hm
    }

    /// A file seen for the first time: its last `tail` bytes, from a line start.
    static func start(of path: String) -> UInt64 {
        guard let handle = FileHandle(forReadingAtPath: path), let end = try? handle.seekToEnd() else { return 0 }
        defer { try? handle.close() }
        guard end > tail else { return 0 }
        let from = end - tail
        guard (try? handle.seek(toOffset: from)) != nil, let chunk = try? handle.read(upToCount: 1 << 20),
              let newline = chunk.firstIndex(of: UInt8(ascii: "\n")) else { return end }
        return from + UInt64(newline - chunk.startIndex + 1)
    }

    /// The events in the complete lines written since `file.offset`.
    private static func readNew(_ path: String, into file: inout File) -> [(String?, Sample)] {
        guard let handle = FileHandle(forReadingAtPath: path) else { return [] }
        defer { try? handle.close() }
        guard let end = try? handle.seekToEnd() else { return [] }
        if end < file.offset { file.offset = 0 } // rewritten
        guard end > file.offset, (try? handle.seek(toOffset: file.offset)) != nil,
              let data = try? handle.readToEnd(), let lastNewline = data.lastIndex(of: UInt8(ascii: "\n")) else { return [] }
        file.offset += UInt64(lastNewline - data.startIndex + 1)
        return data[data.startIndex...lastNewline].split(separator: UInt8(ascii: "\n")).compactMap { parse(Data($0)) }
    }

    private static let claudeKey = Data(#""rate_limit_event""#.utf8)
    private static let codexKey = Data(#""account/rateLimits/updated""#.utf8)
    private nonisolated(unsafe) static let iso: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()

    /// `[2026-10-30T11:59:13.253Z] NTIVE: {…}` → the account named in its session id (nil: ask the thread) and the sample.
    /// Claude: `rate_limit_event`, five_hour and seven_day utilization 0...1. Codex: `account/rateLimits/updated`,
    /// primary (300 min) and secondary, percents 0...100.
    static func parse(_ line: Data) -> (String?, Sample)? {
        let claude = line.range(of: claudeKey) != nil
        guard claude || line.range(of: codexKey) != nil,
              line.first == UInt8(ascii: "["), let close = line.firstIndex(of: UInt8(ascii: "]")),
              let at = iso.date(from: String(decoding: line[line.index(after: line.startIndex)..<close], as: UTF8.self)),
              let brace = line[close...].firstIndex(of: UInt8(ascii: "{")),
              let obj = try? JSONSerialization.jsonObject(with: line[brace...]) as? [String: Any],
              let payload = (obj["event"] as? [String: Any])?["payload"] as? [String: Any] else { return nil }
        let session = obj["providerSessionId"] as? String ?? ""
        let account = instance(in: session)
        let t = at.timeIntervalSince1970
        if let info = payload["rate_limit_info"] as? [String: Any] {
            guard let windows = info["unifiedWindows"] as? [String: Any],
                  let five = windows["five_hour"] as? [String: Any] else { return nil }
            let week = windows["seven_day"] as? [String: Any] ?? [:]
            return (account, Sample(at: t, five: number(five["utilization"]), fiveReset: optionalNumber(five["resetsAt"]),
                                    week: number(week["utilization"]), weekReset: optionalNumber(week["resetsAt"]),
                                    status: info["status"] as? String ?? "?"))
        }
        guard let limits = (payload["params"] as? [String: Any])?["rateLimits"] as? [String: Any],
              let primary = limits["primary"] as? [String: Any], number(primary["windowDurationMins"]) == 300 else { return nil }
        let secondary = limits["secondary"] as? [String: Any] ?? [:]
        let hit = limits["rateLimitReachedType"].map { !($0 is NSNull) } ?? false
        return (account ?? "codex", Sample(at: t, five: number(primary["usedPercent"]) / 100, fiveReset: optionalNumber(primary["resetsAt"]),
                                           week: number(secondary["usedPercent"]) / 100, weekReset: optionalNumber(secondary["resetsAt"]),
                                           status: hit ? "rejected" : "allowed"))
    }

    /// `provider-session:provider-instance:claudeAgent_team:thread:…` → "claudeAgent_team".
    static func instance(in session: String) -> String? {
        guard let r = session.range(of: "provider-instance:") else { return nil }
        let rest = session[r.upperBound...]
        let id = rest.prefix { $0.isLetter || $0.isNumber || $0 == "_" || $0 == "-" }
        return id.isEmpty || !rest.dropFirst(id.count).hasPrefix(":") ? nil : String(id)
    }

    private static func number(_ v: Any?) -> Double { optionalNumber(v) ?? 0 }
    private static func optionalNumber(_ v: Any?) -> Double? { (v as? NSNumber)?.doubleValue }
}
