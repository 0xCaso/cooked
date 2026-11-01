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

    init(dirs: [String: String]) { self.dirs = dirs }

    /// The connected Claude accounts' folders.
    convenience init(accounts: [ConnectedAccount]) {
        self.init(dirs: Dictionary(accounts.filter { $0.kind == .claude }.map { ($0.id, $0.path) }, uniquingKeysWith: { a, _ in a }))
    }

    static func dir(for id: String, home: String) -> String {
        let suffix = id.dropFirst("claudeAgent".count).replacingOccurrences(of: "_", with: "-")
        return home + "/.claude" + suffix
    }

    /// Account id → minute → new tokens, for the `minutes` minutes up to `now`, and the sessions of the last 15
    /// minutes. Accounts without a transcript directory are left out, so the caller keeps its other source for them.
    func read(now: Date, minutes: Int = 60) -> Reading {
        lock.lock()
        defer { lock.unlock() }
        let minuteNow = Int(now.timeIntervalSince1970) / 60
        let first = minuteNow - minutes + 1
        let since = now.addingTimeInterval(-Double(minutes + 1) * 60)
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
            var account: [Int: Double] = [:]
            for case let url as URL in walk where url.pathExtension == "jsonl" {
                guard let modified = try? url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate,
                      modified >= since else { continue }
                seen.insert(url.path)
                var file = files[url.path] ?? File(offset: Self.start(of: url, after: since))
                Self.readNew(url, into: &file)
                var recent = 0.0, active = false
                for (minute, tokens) in file.messages.values where minute >= first && minute <= minuteNow {
                    account[minute, default: 0] += tokens
                    if minute > minuteNow - 15 { recent += tokens }
                    if minute > minuteNow - 5 { active = true }
                }
                if recent > 0 {
                    let (sid, isSub) = Self.session(of: url)
                    mainPaths[sid] = isSub ? url.deletingLastPathComponent().deletingLastPathComponent().path + ".jsonl" : url.path
                    var s = sessions[sid] ?? Session(account: id, id: sid, cwd: nil, tokens: 0, mainActive: false, activeSubagents: 0)
                    s.tokens += recent
                    if isSub { s.activeSubagents += active ? 1 : 0 } else { s.mainActive = active }
                    // a subagent may run in another folder: the session's own cwd wins
                    if !isSub || s.cwd == nil { s.cwd = file.cwd ?? s.cwd }
                    sessions[sid] = s
                }
                file.messages = file.messages.filter { $0.value.minute >= first }
                files[url.path] = file
            }
            out.minutes[id] = account
        }
        files = files.filter { seen.contains($0.key) }
        for (sid, path) in mainPaths {
            let head = heads[path] ?? Self.headTitles(of: URL(fileURLWithPath: path))
            heads[path] = head
            sessions[sid]?.name = head.name(after: files[path]?.titles ?? Titles())
        }
        heads = heads.filter { mainPaths.values.contains($0.key) }
        out.sessions = sessions.values.sorted { ($0.tokens, $0.id) > ($1.tokens, $1.id) }
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
            let line = Data(line)
            if let call = parse(line) {
                file.messages[call.id] = (call.minute, call.tokens)
                if let cwd = call.cwd { file.cwd = cwd }
            } else {
                title(line, into: &file.titles)
            }
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
