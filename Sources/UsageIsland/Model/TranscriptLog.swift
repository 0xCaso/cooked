import Foundation

/// New tokens per minute of the Claude accounts, read from their transcripts (`<config dir>/projects/**/*.jsonl`).
/// T3 Code's database writes a turn's tokens only when the turn ends, and never the subagents that keep running
/// after it; the transcripts have every call, with its time. "New" counts like the watcher:
/// input + cache writes + output, without cache reads.
/// Keeps each file's read position between readings, so a reading only parses the lines added since the last one.
final class TranscriptLog: @unchecked Sendable {
    /// Account id → config dir: `claudeAgent` is `~/.claude`, `claudeAgent_x_y` is `~/.claude-x-y`.
    let dirs: [String: String]

    private struct File {
        var offset: UInt64 = 0
        /// Message id → (minute, new tokens). A message is written once per content block, with the same usage.
        var messages: [String: (minute: Int, tokens: Double)] = [:]
    }

    private let lock = NSLock()
    private var files: [String: File] = [:]

    init(dirs: [String: String]) { self.dirs = dirs }

    static func standard(home: String = NSHomeDirectory()) -> TranscriptLog {
        let ids = ["claudeAgent", "claudeAgent_work", "claudeAgent_team", "claudeAgent_side"]
        return TranscriptLog(dirs: Dictionary(uniqueKeysWithValues: ids.map { ($0, dir(for: $0, home: home)) }))
    }

    static func dir(for id: String, home: String) -> String {
        let suffix = id.dropFirst("claudeAgent".count).replacingOccurrences(of: "_", with: "-")
        return home + "/.claude" + suffix
    }

    /// Account id → minute → new tokens, for the `minutes` minutes up to `now`. Accounts without a transcript
    /// directory are left out, so the caller keeps its other source for them.
    func read(now: Date, minutes: Int = 60) -> [String: [Int: Double]] {
        lock.lock()
        defer { lock.unlock() }
        let minuteNow = Int(now.timeIntervalSince1970) / 60
        let first = minuteNow - minutes + 1
        let since = now.addingTimeInterval(-Double(minutes + 1) * 60)
        var out: [String: [Int: Double]] = [:]
        var seen = Set<String>()
        for (id, dir) in dirs {
            let root = URL(fileURLWithPath: dir + "/projects")
            var isDir: ObjCBool = false
            guard FileManager.default.fileExists(atPath: root.path, isDirectory: &isDir), isDir.boolValue,
                  let walk = FileManager.default.enumerator(at: root, includingPropertiesForKeys: [.contentModificationDateKey],
                                                            options: [.skipsHiddenFiles]) else { continue }
            var account: [Int: Double] = [:]
            for case let url as URL in walk where url.pathExtension == "jsonl" {
                guard let modified = try? url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate,
                      modified >= since else { continue }
                seen.insert(url.path)
                var file = files[url.path] ?? File(offset: Self.start(of: url, after: since))
                Self.readNew(url, into: &file)
                for (minute, tokens) in file.messages.values where minute >= first && minute <= minuteNow {
                    account[minute, default: 0] += tokens
                }
                file.messages = file.messages.filter { $0.value.minute >= first }
                files[url.path] = file
            }
            out[id] = account
        }
        files = files.filter { seen.contains($0.key) }
        return out
    }

    /// Where to start reading a file seen for the first time: transcripts grow to hundreds of MB, so step back from
    /// the end in blocks until a block starts before `since`. Lines are in time order. Returns a line start.
    static func start(of url: URL, after since: Date, block: UInt64 = 4 << 20) -> UInt64 {
        guard let handle = try? FileHandle(forReadingFrom: url), let end = try? handle.seekToEnd() else { return 0 }
        defer { try? handle.close() }
        let key = Data(#""timestamp":""#.utf8)
        var start = end
        while start > 0 {
            start = start > block ? start - block : 0
            guard (try? handle.seek(toOffset: start)) != nil, let chunk = try? handle.read(upToCount: Int(block)) else { return 0 }
            guard let hit = chunk.range(of: key), chunk.count >= hit.upperBound + 24,
                  let at = iso.date(from: String(decoding: chunk[hit.upperBound..<hit.upperBound + 24], as: UTF8.self))
            else { continue }
            if at < since {
                guard start > 0, let newline = chunk.firstIndex(of: UInt8(ascii: "\n")) else { return 0 }
                return start + UInt64(newline - chunk.startIndex + 1)
            }
        }
        return 0
    }

    /// Parses the complete lines written since `file.offset`.
    private static func readNew(_ url: URL, into file: inout File) {
        guard let handle = try? FileHandle(forReadingFrom: url) else { return }
        defer { try? handle.close() }
        guard let end = try? handle.seekToEnd() else { return }
        if end < file.offset { file = File() } // rewritten
        guard end > file.offset, (try? handle.seek(toOffset: file.offset)) != nil,
              let data = try? handle.readToEnd(), let last = data.lastIndex(of: UInt8(ascii: "\n")) else { return }
        for line in data[data.startIndex...last].split(separator: UInt8(ascii: "\n")) {
            if let (id, minute, tokens) = parse(Data(line)) { file.messages[id] = (minute, tokens) }
        }
        file.offset += UInt64(last - data.startIndex + 1)
    }

    private struct Line: Decodable {
        struct Message: Decodable {
            struct Usage: Decodable {
                let input_tokens: Double?
                let cache_creation_input_tokens: Double?
                let output_tokens: Double?
            }
            let id: String?
            let usage: Usage?
        }
        let type: String?
        let timestamp: String?
        let requestId: String?
        let message: Message?
    }

    /// One transcript line → (message id, minute, new tokens), or nil when it is not an assistant call.
    static func parse(_ line: Data) -> (String, Int, Double)? {
        // most lines are tool output: skip them before decoding
        guard line.range(of: Data(#""usage""#.utf8)) != nil,
              let l = try? JSONDecoder().decode(Line.self, from: line), l.type == "assistant",
              let u = l.message?.usage, let stamp = l.timestamp, let at = iso.date(from: stamp) else { return nil }
        let id = l.message?.id ?? l.requestId ?? stamp
        let tokens = (u.input_tokens ?? 0) + (u.cache_creation_input_tokens ?? 0) + (u.output_tokens ?? 0)
        return (id, Int(at.timeIntervalSince1970) / 60, tokens)
    }

    private nonisolated(unsafe) static let iso: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()
}
