import Foundation
import Testing
@testable import Cooked

/// Connecting the status line bridge changes only the `statusLine` key, and disconnecting gives back the exact old text.
struct StatusLineSetupTests {
    let root: String
    let setup: StatusLineSetup

    init() throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent("statusline-\(UUID().uuidString)").path
        try FileManager.default.createDirectory(atPath: root + "/.claude", withIntermediateDirectories: true)
        setup = StatusLineSetup(support: root + "/Cooked")
    }

    var settings: String { root + "/.claude/settings.json" }

    func text(_ path: String) throws -> String { try String(contentsOfFile: path, encoding: .utf8) }

    func statusLine(_ path: String) throws -> [String: Any] {
        let object = try JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: path))) as? [String: Any]
        return try #require(object?["statusLine"] as? [String: Any])
    }

    @Test func noStatusLine() throws {
        let old = """
        {
          "model": "opus",
          "env": {
            "A": "1"
          }
        }

        """
        try old.write(toFile: settings, atomically: true, encoding: .utf8)
        try setup.connect(settings: settings)
        let key = StatusLineSetup.key(StatusLineSetup.real(settings))
        #expect(try text(settings) == """
        {
          "model": "opus",
          "env": {
            "A": "1"
          },
          "statusLine": {
            "type": "command",
            "command": "sh '\(root)/Cooked/statusline-bridge.sh' \(key)"
          }
        }

        """)
        #expect(try text(settings + ".cooked-backup") == old)
        #expect(setup.state(settings: settings) == .connected)
        #expect(FileManager.default.isExecutableFile(atPath: "/bin/sh") && FileManager.default.fileExists(atPath: setup.script))

        #expect(try setup.disconnect(settings: settings))
        #expect(try text(settings) == old)
        #expect(setup.state(settings: settings) == .notConnected)
    }

    @Test func anOldCommandIsKeptAndGivenBack() throws {
        let old = #"{"model":"opus","statusLine":{"type":"command","command":"\"/n/node\" \"/p/statusline.js\"","padding":0,"refreshInterval":5},"theme":"dark"}"#
        try old.write(toFile: settings, atomically: true, encoding: .utf8)
        try setup.connect(settings: settings)
        let now = try text(settings)
        #expect(now.hasPrefix(#"{"model":"opus","statusLine":{"#) && now.hasSuffix(#"},"theme":"dark"}"#))
        let line = try statusLine(settings)
        #expect(line["command"] as? String == setup.command(key: StatusLineSetup.key(StatusLineSetup.real(settings))))
        #expect(line["padding"] as? Int == 0 && line["refreshInterval"] as? Int == 5)
        let key = StatusLineSetup.key(StatusLineSetup.real(settings))
        #expect(try text(setup.savedDir + "/\(key).command") == #""/n/node" "/p/statusline.js""#)

        try setup.disconnect(settings: settings)
        #expect(try text(settings) == old)
    }

    @Test func aSymlinkIsFollowedAndKept() throws {
        let shared = root + "/.claude/settings.json"
        try "{\n  \"a\": 1\n}\n".write(toFile: shared, atomically: true, encoding: .utf8)
        try FileManager.default.createDirectory(atPath: root + "/.claude-work", withIntermediateDirectories: true)
        let link = root + "/.claude-work/settings.json"
        try FileManager.default.createSymbolicLink(atPath: link, withDestinationPath: shared)

        try setup.connect(settings: link)
        #expect(try FileManager.default.destinationOfSymbolicLink(atPath: link) == shared)
        #expect(try statusLine(shared)["type"] as? String == "command")
        #expect(setup.state(settings: shared) == .connected && setup.state(settings: link) == .connected)
        // the second account shares the file: nothing more to do
        let once = try text(shared)
        try setup.connect(settings: shared)
        #expect(try text(shared) == once)
        #expect(!FileManager.default.fileExists(atPath: link + ".cooked-backup"))
    }

    @Test func aChangedKeyIsNotTouched() throws {
        try "{}".write(toFile: settings, atomically: true, encoding: .utf8)
        try setup.connect(settings: settings)
        let changed = #"{"statusLine":{"type":"command","command":"claude-hud"}}"#
        try changed.write(toFile: settings, atomically: true, encoding: .utf8)
        #expect(setup.state(settings: settings) == .changed)
        #expect(try !setup.disconnect(settings: settings))
        #expect(try text(settings) == changed)

        // connect again wraps the new command
        try setup.connect(settings: settings)
        let key = StatusLineSetup.key(StatusLineSetup.real(settings))
        #expect(try text(setup.savedDir + "/\(key).command") == "claude-hud")
        try setup.disconnect(settings: settings)
        #expect(try text(settings) == changed)
    }

    @Test func theBackupIsMadeOnce() throws {
        try "{\"a\":1}".write(toFile: settings, atomically: true, encoding: .utf8)
        try "older".write(toFile: settings + ".cooked-backup", atomically: true, encoding: .utf8)
        try setup.connect(settings: settings)
        #expect(try text(settings + ".cooked-backup") == "older")
    }

    @Test func notAnObjectIsNotWritten() throws {
        try "[1, 2]".write(toFile: settings, atomically: true, encoding: .utf8)
        #expect(throws: StatusLineSetup.Failure.notAnObject) { try setup.connect(settings: settings) }
        #expect(try text(settings) == "[1, 2]")
    }

    @Test func removingTheOnlyKeyLeavesAnEmptyObject() {
        var object = JSONObjectText(#"{"statusLine": {"command": "x"}}"#)!
        object.remove("statusLine")
        #expect(object.text == "{}")
        var first = JSONObjectText("{\n  \"statusLine\": 1,\n  \"b\": \"}\"\n}")!
        first.remove("statusLine")
        #expect(first.text == "{\n  \"b\": \"}\"\n}")
    }

    /// The script saves the JSON for the account in `transcript_path`, then prints the old status line's output.
    @Test func theBridgeSavesTheJSONAndRunsTheOldLine() throws {
        try #"{"statusLine":{"type":"command","command":"read x; echo \"old line $x\" | cut -c1-12"}}"#
            .write(toFile: settings, atomically: true, encoding: .utf8)
        try setup.connect(settings: settings)
        let input = #"{"transcript_path":"/h/.claude-team/projects/p/s.jsonl","rate_limits":{"five_hour":{"used_percentage":42,"resets_at":1900000000}}}"#
        let line = try statusLine(settings)
        let (out, status) = try run(line["command"] as! String, input: input)
        #expect(status == 0)
        #expect(out == "old line {\"t\n")
        #expect(try text(setup.bridgeDir + "/claudeAgent_team.json") == input)
        #expect(StatusLineBridge.samples(dir: setup.bridgeDir)["claudeAgent_team"]?.five == 0.42)

        // with no old line it prints nothing, and `~/.claude` is `claudeAgent`
        try setup.disconnect(settings: settings)
        try "{}".write(toFile: settings, atomically: true, encoding: .utf8)
        try setup.connect(settings: settings)
        let (none, _) = try run(try statusLine(settings)["command"] as! String,
                                input: #"{"transcript_path":"/h/.claude/projects/p/s.jsonl"}"#)
        #expect(none.isEmpty)
        #expect(FileManager.default.fileExists(atPath: setup.bridgeDir + "/claudeAgent.json"))
    }

    func run(_ command: String, input: String) throws -> (String, Int32) {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/bin/sh")
        p.arguments = ["-c", command]
        let stdin = Pipe(), stdout = Pipe()
        p.standardInput = stdin
        p.standardOutput = stdout
        try p.run()
        stdin.fileHandleForWriting.write(Data(input.utf8))
        try stdin.fileHandleForWriting.close()
        p.waitUntilExit()
        return (String(decoding: stdout.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self), p.terminationStatus)
    }
}
