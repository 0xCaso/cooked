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
            try append((0..<20).map { call("f\(i)m\($0)", ago: 60 * 24 * (i + 1) + 19 - $0, tokens: 1) }, to: url) // in time order, like a transcript
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
    /// After `/compact`, Claude Code writes copies of the latest calls again, with the same id and time. Copies of
    /// calls older than the 1-hour keys must not count again, in a later reading or after a restart.
    @Test func compactCopiesOfOldCallsDoNotCountAgain() throws {
        let (dir, urls) = try makeDir()
        let url = cacheURL()
        try append([call("m1", ago: 180, tokens: 100), call("m2", ago: 120, tokens: 200)], to: urls[0])
        let before = MinuteLedger(url: url)
        #expect(reading(TranscriptLog(dirs: ["a": dir], ledger: before)).values.reduce(0, +) == 300)
        before.save(force: true)
        // the compaction copies m1 and m2, then a new call
        try append([call("m1", ago: 180, tokens: 100), call("m2", ago: 120, tokens: 200), call("m3", ago: 1, tokens: 7)], to: urls[0])
        let after = MinuteLedger(url: url)
        let log = TranscriptLog(dirs: ["a": dir], ledger: after)
        let m = reading(log)
        #expect(m == [minute(ago: 180): 100, minute(ago: 120): 200, minute(ago: 1): 7])
        // a second compaction in the same running app
        try append([call("m2", ago: 120, tokens: 200), call("m3", ago: 1, tokens: 7), call("m4", ago: 0, tokens: 5)], to: urls[0])
        #expect(reading(log).values.reduce(0, +) == 312)
    }

    /// A cache from before this fix is dropped, so the 14 days are read again without the copies.
    @Test func anOldCacheIsReadAgain() throws {
        let url = cacheURL()
        try Data(#"{"version":1,"minutes":{"a":{"100":5}},"marks":{},"historyDone":true}"#.utf8).write(to: url)
        #expect(MinuteLedger(url: url).historyProgress != nil)
    }
    /// A file seen for the first time is read from 14 days back. A `/compact` copy with an older time, after newer
    /// calls, must not move the start past those calls.
    @Test func anOldCopyDoesNotMoveTheStartPastNewerCalls() throws {
        let (_, urls) = try makeDir()
        let days = MinuteLedger.keepMinutes
        let old = (0..<40).map { call("o\($0)", ago: days + 200 - $0, tokens: 1) }
        let inWindow = (0..<40).map { call("n\($0)", ago: days - 100 + 40 - $0, tokens: 1) }
        let copy = call("o0", ago: days + 200, tokens: 1)
        try append(old + inWindow + [copy] + [call("z", ago: 1, tokens: 1)], to: urls[0])
        let firstNew = try Data(contentsOf: urls[0]).range(of: Data(#""id":"n0""#.utf8))!.lowerBound
        // with some block sizes the copy is the first time in a block
        for block in stride(from: 300, through: 3000, by: 7) {
            let start = TranscriptLog.start(of: urls[0], after: MinuteLedger.since(now), block: UInt64(block))
            #expect(start <= UInt64(firstNew), "block \(block)")
        }
    }
}
