import Foundation

/// New tokens per minute of the Claude accounts, read from their transcripts (`<config dir>/projects/**/*.jsonl`).
/// T3 Code's database writes a turn's tokens only when the turn ends, and never the subagents that keep running
/// after it; the transcripts have every call, with its time. "New" counts like the watcher:
/// input + cache writes + output, without cache reads.
/// Keeps each file's read position between readings, so a reading only parses the lines added since the last one.
final class TranscriptLog: @unchecked Sendable {
    /// Account id → config dir: `claudeAgent` is `~/.claude`, `claudeAgent_x_y` is `~/.claude-x-y`.
    let dirs: [String: String]

    /// What a file's lines say besides tokens, since Cooked started.
    private struct File {
        /// The working directory of the last call: the source's project.
        var cwd: String?
        /// The last titles in the lines read so far.
        var titles = Titles()
    }

    /// A session's names, from its own lines: the user's (`/rename`), Claude Code's, and the first prompt.
    struct Titles: Equatable {
        var custom: String?
        var ai: String?
        var firstPrompt: String?
        /// The first working directory in the head: the folder of a session with no new call yet.
        var cwd: String?

        /// `custom-title`, else `ai-title`, else the first prompt; `later` (newer lines) wins over `self`.
        func name(after later: Titles) -> String? {
            later.custom ?? custom ?? later.ai ?? ai ?? firstPrompt
        }
    }

    /// One Claude session in the last 15 minutes: its own calls and its subagents' calls, which run in files of their own.
    struct Session: Equatable {
        let account: String
        /// The session id, the transcript's file name; T3 Code keeps it as the thread's `nativeThreadRef.nativeId`.
        let id: String
        var cwd: String?
        /// New tokens in the last 15 minutes, subagents included.
        var tokens: Double
        /// The session itself made a call in the last 5 minutes.
        var mainActive: Bool
        /// Subagents with a call in the last 5 minutes.
        var activeSubagents: Int
        /// The session's name: its title in Claude Code, else its first prompt.
        var name: String? = nil
        /// The newest minute with a call, subagents included: an ended session shows how long ago it stopped.
        var lastMinute: Int? = nil
    }

    struct Reading {
        /// Account id → minute → new tokens.
        var minutes: [String: [Int: Double]] = [:]
        /// Sessions with tokens in the last 15 minutes, most tokens first.
        var sessions: [Session] = []
        subscript(id: String) -> [Int: Double]? { minutes[id] }
    }

    /// `<projects>/<dir>/<session>.jsonl` → (session, false); `<projects>/<dir>/<session>/subagents/agent-x.jsonl` → (session, true).
    static func session(of url: URL) -> (id: String, isSubagent: Bool) {
        let parent = url.deletingLastPathComponent()
        if parent.lastPathComponent == "subagents" {
            return (parent.deletingLastPathComponent().lastPathComponent, true)
        }
        return (url.deletingPathExtension().lastPathComponent, false)
    }

    private let lock = NSLock()
    private var files: [String: File] = [:]
    /// Main transcript path → the titles in its first `headBytes`, read once: the first prompt, and titles the tail lacks.
    private var heads: [String: Titles] = [:]
    static let headBytes = 512 * 1024

    /// The minute cache, where the tokens go. A ledger the reader made itself is begun and ended by `read`.
    let ledger: MinuteLedger
    private let ownsLedger: Bool

    init(dirs: [String: String], ledger: MinuteLedger? = nil) {
        self.dirs = dirs
        self.ledger = ledger ?? MinuteLedger()
        ownsLedger = ledger == nil
    }

    /// The connected Claude accounts' folders.
    convenience init(accounts: [ConnectedAccount], ledger: MinuteLedger? = nil) {
        self.init(dirs: Dictionary(accounts.filter { $0.kind == .claude }.map { ($0.id, $0.path) }, uniquingKeysWith: { a, _ in a }),
                  ledger: ledger)
    }

    static func dir(for id: String, home: String) -> String {
        let suffix = id.dropFirst("claudeAgent".count).replacingOccurrences(of: "_", with: "-")
        return home + "/.claude" + suffix
    }

    /// Account id → minute → new tokens, for the `minutes` minutes up to `now` (from the minute cache), and the sessions
    /// of the last 15 minutes. Accounts without a transcript directory are left out, so the caller keeps its other
    /// source for them. Walks the files written in the last 14 days, newest first.
    func read(now: Date, minutes: Int = 60) -> Reading {
        lock.lock()
        defer { lock.unlock() }
        if ownsLedger { ledger.begin() }
        defer { if ownsLedger { ledger.end(now: now) } }
        let minuteNow = Int(now.timeIntervalSince1970) / 60
        let since = MinuteLedger.since(now)
        var out = Reading()
        var sessions: [String: Session] = [:]
        var mainPaths: [String: String] = [:]
        var seen = Set<String>()
        for (id, dir) in dirs {
            let root = URL(fileURLWithPath: dir + "/projects")
            var isDir: ObjCBool = false
            guard FileManager.default.fileExists(atPath: root.path, isDirectory: &isDir), isDir.boolValue,
                  let walk = FileManager.default.enumerator(at: root, includingPropertiesForKeys: [.contentModificationDateKey],
                                                            options: [.skipsHiddenFiles]) else { continue }
            var urls: [(url: URL, modified: Date)] = []
            for case let url as URL in walk where url.pathExtension == "jsonl" {
                guard let modified = try? url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate,
                      modified >= since else { continue }
                urls.append((url, modified))
            }
            for (url, _) in urls.sorted(by: { $0.modified > $1.modified }) {
                seen.insert(url.path)
                var file = files[url.path] ?? File()
                let mark = ledger.read(url, now: now) { line, mark in
                    if let call = Self.parse(line) {
                        if let cwd = call.cwd { file.cwd = cwd }
                        // a message is written once per content block, with the same usage; `/compact` copies
                        // older calls again
                        guard mark.keys[call.id] == nil, call.minute >= mark.newest ?? .min else { return }
                        mark.keys[call.id] = call.minute
                        mark.newest = max(mark.newest ?? .min, call.minute)
                        mark.recent[call.minute, default: 0] += call.tokens
                        ledger.add(id, minute: call.minute, tokens: call.tokens)
                    } else {
                        Self.title(line, into: &file.titles)
                    }
                }
                files[url.path] = file
                var recent = 0.0, active = false, last: Int?
                for (minute, tokens) in mark.recent where minute <= minuteNow {
                    if minute > minuteNow - 15 { recent += tokens; last = max(last ?? minute, minute) }
                    if minute > minuteNow - 5 { active = true }
                }
                if recent > 0 {
                    let (sid, isSub) = Self.session(of: url)
                    mainPaths[sid] = isSub ? url.deletingLastPathComponent().deletingLastPathComponent().path + ".jsonl" : url.path
                    var s = sessions[sid] ?? Session(account: id, id: sid, cwd: nil, tokens: 0, mainActive: false, activeSubagents: 0)
                    s.tokens += recent
                    if let last { s.lastMinute = max(s.lastMinute ?? last, last) }
                    if isSub { s.activeSubagents += active ? 1 : 0 } else { s.mainActive = active }
                    // a subagent may run in another folder: the session's own cwd wins
                    if !isSub || s.cwd == nil { s.cwd = file.cwd ?? s.cwd }
                    sessions[sid] = s
                }
            }
            out.minutes[id] = ledger.minutes(id, from: minuteNow - minutes + 1, to: minuteNow)
        }
        files = files.filter { seen.contains($0.key) }
        for (sid, path) in mainPaths {
            let head = heads[path] ?? Self.headTitles(of: URL(fileURLWithPath: path))
            heads[path] = head
            sessions[sid]?.name = head.name(after: files[path]?.titles ?? Titles())
            // the cwd of a session resumed after a restart, before its first new call
            if sessions[sid]?.cwd == nil { sessions[sid]?.cwd = head.cwd }
        }
        heads = heads.filter { mainPaths.values.contains($0.key) }
        out.sessions = sessions.values.sorted { ($0.tokens, $0.id) > ($1.tokens, $1.id) }
        return out
    }

    /// Where to start reading a file seen for the first time: transcripts grow to hundreds of MB, so step back from
    /// the end in blocks until every time in a block is before `since`, and start at that block's last line.
    /// Calls are in time order, but `/compact` copies older lines after newer ones. Returns a line start.
    static func start(of url: URL, after since: Date, block: UInt64 = 4 << 20) -> UInt64 {
        guard let handle = try? FileHandle(forReadingFrom: url), let end = try? handle.seekToEnd() else { return 0 }
        defer { try? handle.close() }
        let key = Data(#""timestamp":""#.utf8)
        var start = end
        while start > 0 {
            start = start > block ? start - block : 0
            guard (try? handle.seek(toOffset: start)) != nil, let chunk = try? handle.read(upToCount: Int(block)) else { return 0 }
            var newest: Date?
            var from = chunk.startIndex
            while let hit = chunk.range(of: key, in: from..<chunk.endIndex) {
                from = hit.upperBound
                guard chunk.endIndex >= hit.upperBound + 24,
                      let at = iso.date(from: String(decoding: chunk[hit.upperBound..<hit.upperBound + 24], as: UTF8.self))
                else { continue }
                newest = max(newest ?? at, at)
            }
            if let newest, newest < since {
                guard let newline = chunk.lastIndex(of: UInt8(ascii: "\n")) else { return 0 }
                return start + UInt64(newline - chunk.startIndex + 1)
            }
        }
        return 0
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
        let cwd: String?
        let message: Message?
    }

    private struct TitleLine: Decodable {
        struct Message: Decodable {
            enum Content: Decodable {
                case text(String), blocks([Block])
                struct Block: Decodable { let type: String?; let text: String? }
                init(from decoder: Decoder) throws {
                    let c = try decoder.singleValueContainer()
                    if let s = try? c.decode(String.self) { self = .text(s) } else { self = .blocks(try c.decode([Block].self)) }
                }
            }
            let content: Content?
        }
        let type: String?
        let customTitle: String?
        let aiTitle: String?
        let isMeta: Bool?
        let cwd: String?
        let message: Message?
    }

    /// Takes a title or the first prompt from a line that is not a call.
    static func title(_ line: Data, into titles: inout Titles) {
        let needsPrompt = titles.firstPrompt == nil
        guard line.range(of: Data(#""custom-title""#.utf8)) != nil || line.range(of: Data(#""ai-title""#.utf8)) != nil
                || (needsPrompt && line.range(of: Data(#""type":"user""#.utf8)) != nil),
              let l = try? JSONDecoder().decode(TitleLine.self, from: line) else { return }
        switch l.type {
        case "custom-title": titles.custom = clean(l.customTitle) ?? titles.custom
        case "ai-title": titles.ai = clean(l.aiTitle) ?? titles.ai
        case "user" where needsPrompt && l.isMeta != true:
            if titles.cwd == nil { titles.cwd = l.cwd }
            let text: String? = switch l.message?.content {
            case .text(let s): s
            case .blocks(let blocks): blocks.first { $0.type == "text" }?.text
            case nil: nil
            }
            // command output and injected context start with a tag; tool results have no text block
            if let text, !text.hasPrefix("<") { titles.firstPrompt = clean(text) }
        default: break
        }
    }

    /// The first non-empty line, at most 80 characters.
    static func clean(_ s: String?) -> String? {
        guard let line = s?.split(whereSeparator: \.isNewline).lazy
            .map({ $0.trimmingCharacters(in: .whitespaces) }).first(where: { !$0.isEmpty }) else { return nil }
        return line.count > 80 ? String(line.prefix(79)) + "…" : line
    }

    /// The titles in the first `headBytes` of a transcript.
    static func headTitles(of url: URL) -> Titles {
        var titles = Titles()
        guard let handle = try? FileHandle(forReadingFrom: url) else { return titles }
        defer { try? handle.close() }
        guard let data = try? handle.read(upToCount: headBytes) else { return titles }
        for line in data.split(separator: UInt8(ascii: "\n")) { title(Data(line), into: &titles) }
        return titles
    }

    /// One transcript line → the call, or nil when it is not an assistant call.
    static func parse(_ line: Data) -> (id: String, minute: Int, tokens: Double, cwd: String?)? {
        // most lines are tool output: skip them before decoding
        guard line.range(of: Data(#""usage""#.utf8)) != nil,
              let l = try? JSONDecoder().decode(Line.self, from: line), l.type == "assistant",
              let u = l.message?.usage, let stamp = l.timestamp, let at = iso.date(from: stamp) else { return nil }
        let id = l.message?.id ?? l.requestId ?? stamp
        let tokens = (u.input_tokens ?? 0) + (u.cache_creation_input_tokens ?? 0) + (u.output_tokens ?? 0)
        return (id, Int(at.timeIntervalSince1970) / 60, tokens, l.cwd)
    }

    private nonisolated(unsafe) static let iso: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()
}
