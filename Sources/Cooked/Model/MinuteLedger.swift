import Foundation

/// The minute cache: new tokens per account and minute for the last 14 days, and where each session file was read
/// up to, in `~/Library/Application Support/Cooked/minutes.json`. The readers (`TranscriptLog`, `CodexLog`) add to it.
/// - The first launch reads 14 days of files, `budget` bytes per reading, newest files first, so the island keeps
///   updating while the history fills (`historyProgress`).
/// - At start, each file continues from its saved position: lines written while Cooked was closed are counted.
final class MinuteLedger: @unchecked Sendable {
    static let days = 14
    static let keepMinutes = days * 24 * 60
    /// Bytes read per reading while the history fills.
    static let defaultBudget = 256 << 20
    /// Saved at most this often, and when the history is complete.
    static let saveEvery: TimeInterval = 60

    /// Where a file was read up to.
    struct Mark: Codable, Equatable {
        var offset: UInt64 = 0
        /// Minute → new tokens of this file, the last hour: the sessions of the last 15 minutes.
        var recent: [Int: Double] = [:]
        /// Calls already counted (Claude message ids) → their minute, the last hour: a message is written once per
        /// content block.
        var keys: [String: Int] = [:]
        /// Codex: the totals of the last event, to skip a repeated one.
        var last: String?
    }

    private struct Saved: Codable {
        var version = 1
        var minutes: [String: [Int: Double]] = [:]
        var marks: [String: Mark] = [:]
        var historyDone = false
    }

    let url: URL?
    let budget: Int
    private let lock = NSLock()
    private var saved = Saved()
    private var lastSave = Date.distantPast
    private var dirty = false
    /// This reading: bytes left to spend, and bytes still unread at the end of each file.
    private var left = 0
    private var pending: [String: UInt64] = [:]
    /// The files the readers walked in this reading; the marks of the others are dropped.
    private var seen = Set<String>()
    /// The history fill so far: bytes read, for the progress.
    private var filled: UInt64 = 0

    /// `url` nil keeps everything in memory (tests).
    init(url: URL? = nil, budget: Int = MinuteLedger.defaultBudget) {
        self.url = url
        self.budget = budget
        if let url, let data = try? Data(contentsOf: url), let s = try? JSONDecoder().decode(Saved.self, from: data), s.version == 1 {
            saved = s
        }
    }

    static var defaultURL: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Cooked/minutes.json")
    }

    /// 0...1 while the first 14 days are read; nil when complete.
    var historyProgress: Double? {
        lock.withLock {
            guard !saved.historyDone else { return nil }
            let rest = pending.values.reduce(0, +)
            return filled + rest == 0 ? 0 : Double(filled) / Double(filled + rest)
        }
    }

    // MARK: A reading

    /// Starts a reading, before every reader reads.
    func begin() {
        lock.withLock {
            left = budget
            seen = []
        }
    }

    /// Ends a reading, after every reader: drops minutes older than 14 days and files no reader saw, then saves when due.
    func end(now: Date) {
        lock.withLock {
            let oldest = Int(now.timeIntervalSince1970) / 60 - Self.keepMinutes
            for (id, m) in saved.minutes { saved.minutes[id] = m.filter { $0.key > oldest } }
            saved.marks = saved.marks.filter { seen.contains($0.key) }
            pending = pending.filter { seen.contains($0.key) }
            if !saved.historyDone, pending.values.allSatisfy({ $0 == 0 }) {
                saved.historyDone = true
                lastSave = .distantPast // save the complete history now
            }
        }
        save(now: now)
    }

    func save(now: Date = Date(), force: Bool = false) {
        lock.lock()
        guard let url, dirty, force || now.timeIntervalSince(lastSave) >= Self.saveEvery,
              let data = try? JSONEncoder().encode(saved) else { lock.unlock(); return }
        dirty = false
        lastSave = now
        lock.unlock()
        try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? data.write(to: url, options: .atomic)
    }

    // MARK: For the readers

    /// A file a reader walked in this reading; its mark, if it has one.
    func see(_ path: String) -> Mark? {
        lock.withLock {
            seen.insert(path)
            return saved.marks[path]
        }
    }

    /// How many bytes a file may read now: all of them once the history is complete, else what is left of the budget.
    func allowance(_ path: String, unread: UInt64) -> UInt64 {
        lock.withLock {
            if saved.historyDone { return unread }
            let n = min(unread, UInt64(max(0, left)))
            left -= Int(n)
            return n
        }
    }

    /// After a file's lines are parsed: its new mark and the bytes it still has unread.
    func update(_ path: String, mark: Mark, unread: UInt64, read: UInt64, now: Date) {
        lock.withLock {
            let hour = Int(now.timeIntervalSince1970) / 60 - 60
            var m = mark
            m.recent = m.recent.filter { $0.key > hour }
            m.keys = m.keys.filter { $0.value > hour }
            if saved.marks[path] != m { saved.marks[path] = m; dirty = true }
            pending[path] = unread
            if !saved.historyDone { filled += read }
        }
    }

    func add(_ account: String, minute: Int, tokens: Double) {
        guard tokens > 0 else { return }
        lock.withLock {
            saved.minutes[account, default: [:]][minute, default: 0] += tokens
            dirty = true
        }
    }

    /// Minute → new tokens of an account, from `first` to `last`.
    func minutes(_ account: String, from first: Int, to last: Int) -> [Int: Double] {
        lock.withLock { (saved.minutes[account] ?? [:]).filter { $0.key >= first && $0.key <= last } }
    }

    /// The new tokens of each 15-minute bucket with tokens, per account and every account summed: the alert
    /// thresholds' 14 days.
    func buckets15() -> LimitLog.Buckets {
        lock.withLock {
            var out = LimitLog.Buckets()
            var total: [Int: Double] = [:]
            for (id, minutes) in saved.minutes {
                var own: [Int: Double] = [:]
                for (minute, tokens) in minutes {
                    own[minute / 15, default: 0] += tokens
                    total[minute / 15, default: 0] += tokens
                }
                out.byAccount[id] = own.keys.sorted().map { own[$0]! }
            }
            out.total = total.keys.sorted().map { total[$0]! }
            return out
        }
    }

    /// Where a reader starts a file it has no mark for: 14 days back.
    static func since(_ now: Date) -> Date { now.addingTimeInterval(-Double(keepMinutes) * 60) }
}

extension MinuteLedger {
    /// Reads a file's complete new lines, within the allowance, and moves its mark. A file with no mark starts
    /// 14 days back. `line` parses one line and adds its call (`add`, and the mark's `recent` and `keys`).
    @discardableResult
    func read(_ url: URL, now: Date, line: (Data, inout Mark) -> Void) -> Mark {
        let path = url.path
        var mark = see(path) ?? Mark(offset: TranscriptLog.start(of: url, after: Self.since(now)))
        guard let handle = try? FileHandle(forReadingFrom: url), let end = try? handle.seekToEnd() else { return mark }
        defer { try? handle.close() }
        if end < mark.offset { mark = Mark() } // rewritten
        let n = allowance(path, unread: end - mark.offset)
        var consumed: UInt64 = 0
        if n > 0, (try? handle.seek(toOffset: mark.offset)) != nil, let data = try? handle.read(upToCount: Int(n)),
           let last = data.lastIndex(of: UInt8(ascii: "\n")) {
            for l in data[data.startIndex...last].split(separator: UInt8(ascii: "\n")) { line(Data(l), &mark) }
            consumed = UInt64(last - data.startIndex + 1)
            mark.offset += consumed
        }
        update(path, mark: mark, unread: end - mark.offset, read: consumed, now: now)
        return mark
    }
}
