import Foundation
import Testing
@testable import Cooked

/// The minute cache: restarts, the first 14-day fill, old minutes, messages in several blocks, load time.
struct MinuteLedgerTests {
    let now = Date()
    var minuteNow: Int { Int(now.timeIntervalSince1970) / 60 }
    let iso: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()

    /// A Claude call `ago` minutes before `now`, with `tokens` new tokens.
    func call(_ id: String, ago: Int, tokens: Int) -> String {
        let at = iso.string(from: now.addingTimeInterval(Double(-ago * 60)))
        return #"{"type":"assistant","timestamp":"\#(at)","message":{"id":"\#(id)","usage":{"input_tokens":\#(tokens),"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":0}}}"#
    }

    func minute(ago: Int) -> Int { Int(now.addingTimeInterval(Double(-ago * 60)).timeIntervalSince1970) / 60 }

    /// A config folder with `files` session files in one project.
    func makeDir(files: Int = 1) throws -> (dir: String, urls: [URL]) {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("ledger-\(UUID().uuidString)").path
        let project = URL(fileURLWithPath: dir + "/projects/p")
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
        return (dir, (0..<files).map { project.appendingPathComponent("s\($0).jsonl") })
    }

    func append(_ lines: [String], to url: URL) throws {
        let text = Data((lines.joined(separator: "\n") + "\n").utf8)
        guard FileManager.default.fileExists(atPath: url.path) else { try text.write(to: url); return }
        let h = try FileHandle(forWritingTo: url)
        try h.seekToEnd()
        try h.write(contentsOf: text)
        try h.close()
    }

    func cacheURL() -> URL {
        FileManager.default.temporaryDirectory.appendingPathComponent("minutes-\(UUID().uuidString).json")
    }

    /// One reading, like `UsageSource` does it: begin, read, end.
    func reading(_ log: TranscriptLog, at now: Date? = nil) -> [Int: Double] {
        log.ledger.begin()
        let r = log.read(now: now ?? self.now, minutes: 14 * 24 * 60)
        log.ledger.end(now: now ?? self.now)
        return r.minutes["a"] ?? [:]
    }

    @Test func aRestartReadsTheLinesWrittenWhileClosed() throws {
        let (dir, urls) = try makeDir()
        let url = cacheURL()
        try append([call("m1", ago: 30, tokens: 100), call("m2", ago: 20, tokens: 200)], to: urls[0])
        let before = MinuteLedger(url: url)
        #expect(reading(TranscriptLog(dirs: ["a": dir], ledger: before)) == [minute(ago: 30): 100, minute(ago: 20): 200])
        before.save(force: true)
        // Cooked is closed for 10 minutes; Claude Code keeps writing
        try append((1...10).map { call("c\($0)", ago: 11 - $0, tokens: 10) }, to: urls[0])
        let after = MinuteLedger(url: url)
        let m = reading(TranscriptLog(dirs: ["a": dir], ledger: after))
        #expect(m[minute(ago: 30)] == 100 && m[minute(ago: 20)] == 200) // from the cache, not counted again
        #expect((1...10).allSatisfy { m[minute(ago: 11 - $0)] == 10 })
        #expect(m.values.reduce(0, +) == 400)
    }

    @Test func theHistoryFillsOverSeveralReadingsNewestFirst() throws {
        let (dir, urls) = try makeDir(files: 4)
        for (i, url) in urls.enumerated() {
            try append((0..<20).map { call("f\(i)m\($0)", ago: 60 * 24 * (i + 1) + $0, tokens: 1) }, to: url)
            // file 0 is the newest
            try FileManager.default.setAttributes([.modificationDate: now.addingTimeInterval(Double(-i * 3600))], ofItemAtPath: url.path)
        }
        let size = try FileManager.default.attributesOfItem(atPath: urls[0].path)[.size] as! Int
        let ledger = MinuteLedger(budget: size + 10) // about one file per reading
        let log = TranscriptLog(dirs: ["a": dir], ledger: ledger)
        var progress: [Double] = []
        var first: [Int: Double] = [:]
        for n in 0..<10 {
            let m = reading(log)
            if n == 0 { first = m }
            guard let p = ledger.historyProgress else { break }
            progress.append(p)
        }
        #expect(first.keys.allSatisfy { $0 > minute(ago: 60 * 24 + 30) }) // the newest file first
        #expect(progress.count >= 3)
        #expect(progress == progress.sorted() && progress.allSatisfy { $0 < 1 })
        #expect(ledger.historyProgress == nil)
        #expect(reading(log).values.reduce(0, +) == 80)
    }

    @Test func minutesOlderThanFourteenDaysAreDropped() {
        let ledger = MinuteLedger()
        let old = minuteNow - MinuteLedger.keepMinutes - 1, kept = minuteNow - MinuteLedger.keepMinutes + 1
        ledger.add("a", minute: old, tokens: 5)
        ledger.add("a", minute: kept, tokens: 7)
        ledger.begin()
        ledger.end(now: now)
        #expect(ledger.minutes("a", from: old - 10, to: minuteNow) == [kept: 7])
    }

    @Test func aMessageWrittenInSeveralBlocksCountsOnceAcrossReadingsAndRestarts() throws {
        let (dir, urls) = try makeDir()
        let url = cacheURL()
        try append([call("m1", ago: 5, tokens: 300)], to: urls[0])
        let ledger = MinuteLedger(url: url)
        let log = TranscriptLog(dirs: ["a": dir], ledger: ledger)
        #expect(reading(log) == [minute(ago: 5): 300])
        try append([call("m1", ago: 5, tokens: 300)], to: urls[0]) // the second block, after the reading
        #expect(reading(log) == [minute(ago: 5): 300])
        ledger.save(force: true)
        try append([call("m1", ago: 5, tokens: 300)], to: urls[0]) // a third block, while Cooked is closed
        #expect(reading(TranscriptLog(dirs: ["a": dir], ledger: MinuteLedger(url: url))) == [minute(ago: 5): 300])
    }

    @Test func aFullCacheLoadsQuickly() throws {
        let url = cacheURL()
        let full = MinuteLedger(url: url)
        // five accounts busy every minute for 14 days
        for account in ["a", "b", "c", "d", "e"] {
            for m in (minuteNow - MinuteLedger.keepMinutes + 1)...minuteNow { full.add(account, minute: m, tokens: 1234) }
        }
        full.save(force: true)
        let start = Date()
        let loaded = MinuteLedger(url: url)
        let seconds = Date().timeIntervalSince(start)
        #expect(loaded.minutes("e", from: minuteNow, to: minuteNow) == [minuteNow: 1234])
        #expect(seconds < 2, "loaded in \(seconds) s")
    }
}
