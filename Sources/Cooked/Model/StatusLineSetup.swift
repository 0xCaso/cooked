import Foundation

/// Connects Claude Code's status line to Cooked's bridge, and gives it back (release spec, section 2).
/// Cooked changes only the `statusLine` key of the real `settings.json`, after a click: every other byte of the file
/// stays as it was. The bridge saves the JSON Claude Code sends for the Claude percent (`StatusLineBridge` reads it),
/// then runs the status line the user had, so it looks the same.
struct StatusLineSetup {
    /// Cooked's folder: the bridge script, the saved status lines and the bridge files.
    let support: String

    init(support: String = StatusLineSetup.defaultSupport) {
        self.support = support
    }

    static var defaultSupport: String {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].path + "/Cooked"
    }

    var script: String { support + "/statusline-bridge.sh" }
    var bridgeDir: String { support + "/bridge" }
    /// Per real settings file: the status line it had (`<key>.json`) and that line's command (`<key>.command`).
    var savedDir: String { support + "/statusline" }

    enum State: Equatable {
        /// Cooked never connected this file, or gave its status line back.
        case notConnected
        case connected
        /// Cooked connected it, but the key changed since: the Claude percent is paused until the user connects again.
        case changed
    }

    enum Failure: Error, Equatable {
        /// `settings.json` is not a JSON object: Cooked does not write it.
        case notAnObject
    }

    /// What Cooked keeps for a settings file: its real path and the `statusLine` value it had, as text, exactly.
    struct Saved: Codable, Equatable {
        let settings: String
        /// nil: the file had no status line.
        let previous: String?
    }

    /// The file Claude Code reads, through the symlinks: several accounts can share one `settings.json`.
    static func real(_ settings: String) -> String {
        URL(fileURLWithPath: settings).resolvingSymlinksInPath().path
    }

    /// A short name for a real settings file, stable across launches (FNV-1a of its path).
    static func key(_ real: String) -> String {
        var hash: UInt64 = 0xcbf2_9ce4_8422_2325
        for byte in real.utf8 { hash = (hash ^ UInt64(byte)) &* 0x100_0000_01b3 }
        return String(format: "%016llx", hash)
    }

    /// The command Claude Code runs once connected: the bridge, with the key of the line to run after it.
    func command(key: String) -> String {
        "sh " + Self.quote(script) + " " + key
    }

    static func quote(_ s: String) -> String {
        "'" + s.replacingOccurrences(of: "'", with: #"'\''"#) + "'"
    }

    // MARK: - State

    func state(settings: String) -> State {
        let real = Self.real(settings), key = Self.key(real)
        guard saved(key) != nil else { return .notConnected }
        return bridged(real: real, key: key) ? .connected : .changed
    }

    private func saved(_ key: String) -> Saved? {
        FileManager.default.contents(atPath: savedDir + "/\(key).json").flatMap { try? JSONDecoder().decode(Saved.self, from: $0) }
    }

    /// The file's `statusLine` runs this bridge.
    private func bridged(real: String, key: String) -> Bool {
        guard let text = try? String(contentsOfFile: real, encoding: .utf8),
              let member = JSONObjectText(text)?.member("statusLine"),
              let value = try? JSONSerialization.jsonObject(with: Data(member.value.utf8)) as? [String: Any] else { return false }
        return value["command"] as? String == command(key: key)
    }

    // MARK: - Connect

    /// Points the file's status line to the bridge. Copies the file once to `settings.json.cooked-backup`, saves the old
    /// `statusLine` object as it is, keeps `padding` and `refreshInterval`, and writes through a temporary file.
    func connect(settings: String) throws {
        let real = Self.real(settings), key = Self.key(real)
        let files = FileManager.default
        let exists = files.fileExists(atPath: real)
        let text = exists ? try String(contentsOfFile: real, encoding: .utf8) : "{}\n"
        guard var object = JSONObjectText(text) else { throw Failure.notAnObject }
        if bridged(real: real, key: key) { return }

        if exists, !files.fileExists(atPath: real + ".cooked-backup") {
            try files.copyItem(atPath: real, toPath: real + ".cooked-backup")
        }
        try installScript()
        let previous = object.member("statusLine")?.value
        try save(Saved(settings: real, previous: previous), key: key)

        var fields = [("type", #""command""#), ("command", Self.jsonString(command(key: key)))]
        if let previous, let old = JSONObjectText(previous) {
            for name in ["padding", "refreshInterval"] { if let m = old.member(name) { fields.append((name, m.value)) } }
        }
        object.set("statusLine", objectText: fields)
        try Self.write(object.text, to: real)
    }

    private func save(_ saved: Saved, key: String) throws {
        let files = FileManager.default
        try files.createDirectory(atPath: savedDir, withIntermediateDirectories: true)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        try encoder.encode(saved).write(to: URL(fileURLWithPath: savedDir + "/\(key).json"), options: .atomic)
        let commandFile = savedDir + "/\(key).command"
        if let previous = saved.previous,
           let old = try? JSONSerialization.jsonObject(with: Data(previous.utf8)) as? [String: Any],
           old["type"] as? String ?? "command" == "command", let command = old["command"] as? String {
            try command.write(toFile: commandFile, atomically: true, encoding: .utf8)
        } else {
            try? files.removeItem(atPath: commandFile)
        }
    }

    func installScript() throws {
        try FileManager.default.createDirectory(atPath: support, withIntermediateDirectories: true)
        try Self.bridgeScript.write(toFile: script, atomically: true, encoding: .utf8)
    }

    // MARK: - Disconnect

    /// Gives the file its old status line back, or removes the key if it had none. When the key changed since Cooked
    /// connected (the user or another tool), the file is not touched. Returns whether the file was written.
    @discardableResult
    func disconnect(settings: String) throws -> Bool {
        let real = Self.real(settings), key = Self.key(real)
        guard let saved = saved(key) else { return false }
        defer {
            try? FileManager.default.removeItem(atPath: savedDir + "/\(key).json")
            try? FileManager.default.removeItem(atPath: savedDir + "/\(key).command")
        }
        guard bridged(real: real, key: key), var object = JSONObjectText(try String(contentsOfFile: real, encoding: .utf8)) else {
            return false
        }
        if let previous = saved.previous { object.set("statusLine", value: previous) } else { object.remove("statusLine") }
        try Self.write(object.text, to: real)
        return true
    }

    // MARK: - Files

    /// Writes a temporary file next to `path`, then renames it: Claude Code never reads a half-written file.
    static func write(_ text: String, to path: String) throws {
        let files = FileManager.default
        let tmp = path + ".cooked-tmp-\(getpid())"
        guard files.createFile(atPath: tmp, contents: Data(text.utf8),
                               attributes: (try? files.attributesOfItem(atPath: path)).flatMap { a in
                                   a[.posixPermissions].map { [.posixPermissions: $0] } }) else {
            throw CocoaError(.fileWriteUnknown)
        }
        guard rename(tmp, path) == 0 else {
            try? files.removeItem(atPath: tmp)
            throw CocoaError(.fileWriteUnknown)
        }
    }

    static func jsonString(_ s: String) -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = .withoutEscapingSlashes
        return String(decoding: (try? encoder.encode(s)) ?? Data(), as: UTF8.self)
    }

    /// The bridge. `sh` only, no other tools than `tr`, `sed` and `mv`; it never waits for Cooked, so the status line
    /// works when Cooked is closed. The account comes from `transcript_path`, like `Accounts.id(folder:)`.
    static let bridgeScript = #"""
    #!/bin/sh
    # Cooked's status line bridge (https://github.com/0xCaso/cooked).
    # Saves the JSON Claude Code gives the status line, for Cooked's Claude percent, then runs the status line
    # you had before with the same JSON and prints its output. It never waits for Cooked.
    here=$(cd "$(dirname "$0")" && pwd)
    input=$(cat)
    path=$(printf '%s' "$input" | tr -d '\n' | sed -n 's/.*"transcript_path"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
    config=${path%/projects/*}
    if [ -n "$path" ] && [ "$config" != "$path" ]; then
      folder=${config##*/}
      case "$folder" in
        .claude|.claude-*)
          id="claudeAgent$(printf '%s' "${folder#.claude}" | tr '-' '_')"
          mkdir -p "$here/bridge" 2>/dev/null
          tmp="$here/bridge/.$id.$$"
          printf '%s' "$input" > "$tmp" 2>/dev/null && mv -f "$tmp" "$here/bridge/$id.json" 2>/dev/null
          ;;
      esac
    fi
    previous="$here/statusline/$1.command"
    if [ -n "$1" ] && [ -f "$previous" ]; then
      printf '%s' "$input" | sh -c "$(cat "$previous")"
    fi

    """#
}

/// The text of a JSON object, edited one top-level member at a time: every other byte stays as it was.
struct JSONObjectText {
    private(set) var text: String

    struct Member {
        let key: String
        /// From the key's opening quote to the value's end.
        let range: Range<Int>
        let valueRange: Range<Int>
        let value: String
    }

    /// nil when `text` is not a JSON object.
    init?(_ text: String) {
        guard (try? JSONSerialization.jsonObject(with: Data(text.utf8))) is [String: Any] else { return nil }
        self.text = text
    }

    private var bytes: [UInt8] { Array(text.utf8) }

    /// The top-level members in order, and the positions of the braces.
    private func scan() -> (open: Int, close: Int, members: [Member]) {
        let b = bytes
        var i = 0
        func skipSpace() { while i < b.count, [0x20, 0x09, 0x0a, 0x0d].contains(b[i]) { i += 1 } }
        func skipString() {
            i += 1
            while i < b.count, b[i] != UInt8(ascii: "\"") { i += b[i] == UInt8(ascii: "\\") ? 2 : 1 }
            i += 1
        }
        func skipValue() {
            switch b[i] {
            case UInt8(ascii: "\""): skipString()
            case UInt8(ascii: "{"), UInt8(ascii: "["):
                var depth = 0
                repeat {
                    switch b[i] {
                    case UInt8(ascii: "\""): skipString(); continue
                    case UInt8(ascii: "{"), UInt8(ascii: "["): depth += 1
                    case UInt8(ascii: "}"), UInt8(ascii: "]"): depth -= 1
                    default: break
                    }
                    i += 1
                } while depth > 0 && i < b.count
            default:
                while i < b.count, ![UInt8(ascii: ","), UInt8(ascii: "}"), UInt8(ascii: "]"), 0x20, 0x09, 0x0a, 0x0d].contains(b[i]) { i += 1 }
            }
        }
        skipSpace()
        let open = i
        i += 1
        var members: [Member] = []
        while true {
            skipSpace()
            if b[i] == UInt8(ascii: "}") { break }
            let keyStart = i
            skipString()
            let key = (try? JSONDecoder().decode(String.self, from: Data(b[keyStart..<i]))) ?? ""
            skipSpace()
            i += 1 // :
            skipSpace()
            let valueStart = i
            skipValue()
            members.append(Member(key: key, range: keyStart..<i, valueRange: valueStart..<i,
                                  value: String(decoding: b[valueStart..<i], as: UTF8.self)))
            skipSpace()
            if b[i] == UInt8(ascii: ",") { i += 1 }
        }
        return (open, i, members)
    }

    func member(_ key: String) -> Member? { scan().members.last { $0.key == key } }

    private mutating func replace(_ range: Range<Int>, with new: String) {
        let b = bytes
        text = String(decoding: b[..<range.lowerBound], as: UTF8.self) + new + String(decoding: b[range.upperBound...], as: UTF8.self)
    }

    /// The spaces before a member's key on its line: the file's own indent.
    private func indent(of m: Member) -> String {
        let b = bytes
        var i = m.range.lowerBound
        while i > 0, b[i - 1] == 0x20 || b[i - 1] == 0x09 { i -= 1 }
        return String(decoding: b[i..<m.range.lowerBound], as: UTF8.self)
    }

    /// Sets a member's value text; a new member goes last.
    mutating func set(_ key: String, value: String) {
        let (open, close, members) = scan()
        if let m = members.last(where: { $0.key == key }) {
            replace(m.valueRange, with: value)
        } else if let last = members.last {
            replace(last.range.upperBound..<last.range.upperBound,
                    with: ",\n" + indent(of: last) + StatusLineSetup.jsonString(key) + ": " + value)
        } else {
            replace(open + 1..<close, with: "\n  " + StatusLineSetup.jsonString(key) + ": " + value + "\n")
        }
    }

    /// Sets a member to an object, one field per line, indented like the file.
    mutating func set(_ key: String, objectText fields: [(String, String)]) {
        let members = scan().members
        let unit = members.first.map(indent(of:)).flatMap { $0.isEmpty ? nil : $0 } ?? "  "
        let outer = members.last(where: { $0.key == key }).map(indent(of:)) ?? members.last.map(indent(of:)) ?? unit
        let body = fields.map { outer + unit + StatusLineSetup.jsonString($0.0) + ": " + $0.1 }.joined(separator: ",\n")
        set(key, value: "{\n" + body + "\n" + outer + "}")
    }

    /// Removes a member and the comma that joined it.
    mutating func remove(_ key: String) {
        let (open, close, members) = scan()
        guard let index = members.lastIndex(where: { $0.key == key }) else { return }
        let m = members[index]
        if index > 0 {
            replace(members[index - 1].range.upperBound..<m.range.upperBound, with: "")
        } else if index + 1 < members.count {
            replace(m.range.lowerBound..<members[index + 1].range.lowerBound, with: "")
        } else {
            replace(open + 1..<close, with: "")
        }
    }
}
