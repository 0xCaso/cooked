import Foundation
import SQLite3
import Testing
@testable import UsageIsland

/// status.json of 29/10 15:25, as the watcher wrote it.
let statusFixture = """
{
 "updated_at": "2026-10-29T15:25:51",
 "accounts": {
  "claudeAgent": {
   "name": "Personal Claude",
   "tokens_fresh": {"1m": 204350, "5m": 204350, "15m": 204350, "1h": 204350, "today": 1339559},
   "spike_threshold_15m": 425706,
   "five_hour_pct": 18, "five_hour_resets_at": "20:20",
   "seven_day_pct": 81, "seven_day_resets_at": "30/10 10:00",
   "limit_status": "allowed_warning", "last_event_at": "15:25", "live": true,
   "slopes_pct_per_min": {}
  },
  "claudeAgent_work": {
   "name": "claude-work",
   "tokens_fresh": {"1m": 4189, "5m": 58613, "15m": 250660, "1h": 988783, "today": 3893535},
   "spike_threshold_15m": 617094,
   "five_hour_pct": 48, "five_hour_resets_at": "19:00",
   "seven_day_pct": 87, "seven_day_resets_at": "30/10 00:00",
   "limit_status": "allowed_warning", "last_event_at": "15:25", "live": true,
   "slopes_pct_per_min": {"5 min": 0.22, "30 min": 0.33},
   "projection": {"window": "30 min", "eta": "18:04", "before_reset": true}
  },
  "codex": {"name": "Codex", "tokens_fresh": {"1m": 0, "5m": 0, "15m": 900, "1h": 900, "today": 900}, "spike_threshold_15m": 350000}
 },
 "alerts_this_run": [],
 "run_seconds": 0.792
}
"""

struct LiveDataTests {
    let calendar: Calendar = {
        var c = Calendar(identifier: .gregorian)
        c.timeZone = TimeZone(identifier: "Europe/Rome")!
        return c
    }()

    func date(_ day: Int, _ month: Int, _ h: Int, _ m: Int, _ s: Int = 0) -> Date {
        calendar.date(from: DateComponents(year: 2026, month: month, day: day, hour: h, minute: m, second: s))!
    }

    var now: Date { date(29, 10, 15, 25, 51) }
    var minuteNow: Int { Int(now.timeIntervalSince1970) / 60 }

    func status(_ edit: (inout StatusFile) -> Void = { _ in }) throws -> StatusFile {
        var s = try StatusFile.decode(Data(statusFixture.utf8))
        edit(&s)
        return s
    }

    @Test func decodesTheWatcherFile() throws {
        let s = try status()
        #expect(s.accounts.keys.sorted() == ["claudeAgent", "claudeAgent_work", "codex"])
        let work = try #require(s.accounts["claudeAgent_work"])
        #expect(work.fiveHourPct == 48)
        #expect(work.tokensFresh?["15m"] == 250_660)
        #expect(work.spikeThreshold15m == 617_094)
        #expect(work.projection == .init(window: "30 min", eta: "18:04"))
        #expect(s.accounts["codex"]?.fiveHourPct == nil)
    }

    @Test func parsesWatcherTimes() {
        #expect(WatcherTime.parse("18:04", now: now, calendar: calendar) == date(29, 10, 18, 4))
        #expect(WatcherTime.parse("30/10 00:00", now: now, calendar: calendar) == date(30, 10, 0, 0))
        #expect(WatcherTime.parse("bad", now: now, calendar: calendar) == nil)
        let newYearsEve = date(30, 12, 12, 0)
        let next = WatcherTime.parse("02/01 10:00", now: newYearsEve, calendar: calendar)
        #expect(next.map { calendar.component(.year, from: $0) } == 2027)
    }

    @Test func calmSnapshotFromTheFixture() throws {
        var minutes = TokenLog.Minutes()
        minutes.byAccount["claudeAgent_work"] = [minuteNow: 4189, minuteNow - 1: 27004, minuteNow - 59: 100, minuteNow - 60: 999]
        let s = try #require(LiveSnapshot.make(status: try status(), minutes: minutes, now: now, calendar: calendar))
        // only accounts with 5h data, work first
        #expect(s.accounts.map(\.short) == ["work", "personal"])
        #expect(s.accounts[0] == Account(id: "claudeAgent_work", short: "work", fiveHourPercent: 48, fiveHourReset: "19:00", isLive: true,
                                       fullAt: "18:04"))
        // both live: work burned more in the last 15 min (250k vs 204k)
        #expect(s.activeID == "claudeAgent_work")
        #expect(s.rateNow == 58613 / 5)
        #expect(s.threshold == 617_094 / 15)
        #expect(s.series.count == 60)
        #expect(s.series.last == 4189)
        #expect(s.series[58] == 27004)
        #expect(s.series[0] == 100) // minute − 60 is outside the hour
        #expect(s.clock == "15:25")
        #expect(s.historyStart == "14:26")
        #expect(s.week == WeekLine(short: "work", percent: 87, reset: "Fri 00:00"))
        // 18:04 is 158 min away: not urgent
        #expect(s.urgency == nil)
    }

    @Test func urgentWhenFiveHourReachesNinety() throws {
        let st = try status { $0.accounts["claudeAgent"]?.fiveHourPct = 91 }
        let s = try #require(LiveSnapshot.make(status: st, minutes: .init(), now: now, calendar: calendar))
        let u = try #require(s.urgency)
        #expect(s.activeID == "claudeAgent")
        #expect(u.percent == 91)
        #expect(u.fullAt == "—")
        #expect(u.detail == "personal · resets 20:20")
        #expect(u.switchLabel == "Switch to work · 48%")
        #expect(u.source == "41k/min now")
    }

    @Test func urgentWhenFullWithinTwentyMinutes() throws {
        let st = try status { $0.accounts["claudeAgent_work"]?.projection = .init(window: "5 min", eta: "15:40") }
        var minutes = TokenLog.Minutes()
        minutes.topThread["claudeAgent_work"] = ("Refactor usage page", 920_000)
        let u = try #require(LiveSnapshot.make(status: st, minutes: minutes, now: now, calendar: calendar)?.urgency)
        #expect(u.fullAt == "15:40")
        #expect(u.detail == "work · in 14 min")
        #expect(u.source == "184k/min from “Refactor usage page”")
        #expect(u.switchLabel == "Switch to personal · 18%")
    }

    @Test func urgentOnASpikeMinute() throws {
        var minutes = TokenLog.Minutes()
        minutes.byAccount["claudeAgent"] = [minuteNow - 1: 425_706 / 15 + 1]
        let s = try #require(LiveSnapshot.make(status: try status(), minutes: minutes, now: now, calendar: calendar))
        #expect(s.activeID == "claudeAgent")
        #expect(s.isUrgent)
        // an older spike does not count
        minutes.byAccount["claudeAgent"] = [minuteNow - 2: 1_000_000]
        #expect(LiveSnapshot.make(status: try status(), minutes: minutes, now: now, calendar: calendar)?.isUrgent == false)
    }

    @Test func urgentWhenTheLimitIsHit() throws {
        let st = try status { $0.accounts["claudeAgent_work"]?.limitStatus = "rejected" }
        #expect(LiveSnapshot.make(status: st, minutes: .init(), now: now, calendar: calendar)?.urgency != nil)
        let warn = try status { $0.accounts["claudeAgent_work"]?.limitStatus = "allowed_warning" }
        #expect(LiveSnapshot.make(status: warn, minutes: .init(), now: now, calendar: calendar)?.urgency == nil)
    }

    @Test func staleAndIdleAccountsAreNeverUrgent() throws {
        let stale = try status {
            $0.accounts["claudeAgent"]?.fiveHourPct = 97
            $0.accounts["claudeAgent"]?.note = "5h window already reset, no new data"
        }
        let s = try #require(LiveSnapshot.make(status: stale, minutes: .init(), now: now, calendar: calendar))
        #expect(s.urgency == nil)
        #expect(s.accounts[1].fiveHourPercent == 0)
        #expect(s.accounts[1].fiveHourReset == "—")
        let idle = try status {
            $0.accounts["claudeAgent"]?.fiveHourPct = 97
            $0.accounts["claudeAgent"]?.live = false
        }
        #expect(LiveSnapshot.make(status: idle, minutes: .init(), now: now, calendar: calendar)?.urgency == nil)
    }

    @Test func noFiveHourDataMeansNoSnapshot() throws {
        let st = try status { $0.accounts = $0.accounts.filter { $0.key == "codex" } }
        #expect(LiveSnapshot.make(status: st, minutes: .init(), now: now, calendar: calendar) == nil)
    }

    @Test func fullAtComesFromTheProjection() throws {
        let st = try status {
            $0.accounts["claudeAgent"]?.fiveHourPct = 103
            $0.accounts["claudeAgent_work"]?.projection = nil
        }
        let s = try #require(LiveSnapshot.make(status: st, minutes: .init(), now: now, calendar: calendar))
        // no projection: not expected to fill up; at 100% and over: already full
        #expect(s.accounts.map(\.fullAt) == [nil, "now"])
        let withProjection = try #require(LiveSnapshot.make(status: try status(), minutes: .init(), now: now, calendar: calendar))
        #expect(withProjection.accounts[0].fullAt == "18:04")
    }

    @Test func shortNames() {
        #expect(LiveSnapshot.short(id: "claudeAgent_work", name: "claude-work") == "work")
        #expect(LiveSnapshot.short(id: "claudeAgent_team", name: "claude-work-team") == "team")
        #expect(LiveSnapshot.short(id: "claudeAgent_new", name: "claude-new") == "new")
    }
}

struct TokenLogTests {
    /// A database with the two tables the query reads, and the thread titles.
    func makeDatabase(_ turns: [(instance: String, thread: String, completed: String, input: Int, cached: Int, output: Int)]) throws -> String {
        let path = FileManager.default.temporaryDirectory.appendingPathComponent("tokenlog-\(UUID().uuidString).sqlite").path
        var db: OpaquePointer?
        #expect(sqlite3_open(path, &db) == SQLITE_OK)
        defer { sqlite3_close(db) }
        var sql = """
            CREATE TABLE orchestration_v2_projection_provider_turns (provider_turn_id TEXT PRIMARY KEY, thread_id TEXT,
              provider_thread_id TEXT, completed_at TEXT, payload_json TEXT);
            CREATE TABLE orchestration_v2_projection_provider_threads (provider_thread_id TEXT PRIMARY KEY, provider TEXT,
              provider_instance_id TEXT);
            CREATE TABLE orchestration_v2_projection_threads (thread_id TEXT PRIMARY KEY, title TEXT);
            """
        for (i, t) in turns.enumerated() {
            sql += """
                INSERT OR IGNORE INTO orchestration_v2_projection_provider_threads VALUES ('pt-\(t.thread)', 'claudeAgent', '\(t.instance)');
                INSERT OR IGNORE INTO orchestration_v2_projection_threads VALUES ('\(t.thread)', 'Thread \(t.thread)');
                INSERT INTO orchestration_v2_projection_provider_turns VALUES ('turn-\(i)', '\(t.thread)', 'pt-\(t.thread)', '\(t.completed)',
                  '{"turnTokenUsage":{"inputTokens":\(t.input),"cachedInputTokens":\(t.cached),"outputTokens":\(t.output)}}');
                """
        }
        #expect(sqlite3_exec(db, sql, nil, nil, nil) == SQLITE_OK)
        return path
    }

    @Test func groupsNewTokensByMinuteAndAccount() throws {
        let now = ISO8601DateFormatter().date(from: "2026-10-29T14:25:51Z")!
        let path = try makeDatabase([
            ("claudeAgent_work", "a", "2026-10-29T14:25:10.000Z", 1000, 200, 50), // 850
            ("claudeAgent_work", "a", "2026-10-29T14:25:40.000Z", 100, 0, 50),    // 150
            ("claudeAgent_work", "b", "2026-10-29T14:24:00.000Z", 5000, 0, 0),
            ("claudeAgent", "c", "2026-10-29T14:22:00.000Z", 300, 0, 0),
            ("claudeAgent", "c", "2026-10-29T13:20:00.000Z", 9999, 0, 0),         // over an hour ago
        ])
        let m = try TokenLog(path: path).read(now: now)
        let minute = Int(now.timeIntervalSince1970) / 60
        #expect(m.byAccount["claudeAgent_work"] == [minute: 1000, minute - 1: 5000])
        #expect(m.byAccount["claudeAgent"] == [minute - 3: 300])
        #expect(m.topThread["claudeAgent_work"]?.title == "Thread b")
        #expect(m.topThread["claudeAgent_work"]?.tokens == 5000)
        #expect(m.topThread["claudeAgent"]?.title == "Thread c")
    }

    @Test func missingDatabaseThrows() {
        #expect(throws: TokenLog.Failure.self) { try TokenLog(path: "/nonexistent/statev2.sqlite").read(now: Date()) }
    }
}

struct UsageSourceTests {
    func write(_ text: String, modified: Date) throws -> String {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("status-\(UUID().uuidString).json")
        try Data(text.utf8).write(to: url)
        try FileManager.default.setAttributes([.modificationDate: modified], ofItemAtPath: url.path)
        return url.path
    }

    @Test func readsAFreshFile() throws {
        let path = try write(statusFixture, modified: Date())
        let r = UsageSource(statusPath: path, databasePath: "/nonexistent").read()
        let reading = try r.get()
        #expect(reading.snapshot.accounts.count == 2)
        #expect(reading.snapshot.series == Array(repeating: 0, count: 60)) // no database: empty histogram
    }

    @Test func reportsProblems() throws {
        #expect(UsageSource(statusPath: "/nonexistent").read() == .failure(.noStatus))
        let old = try write(statusFixture, modified: Date().addingTimeInterval(-600))
        #expect(UsageSource(statusPath: old).read() == .failure(.watcherStopped))
        let empty = try write(#"{"updated_at": "x", "accounts": {}}"#, modified: Date())
        #expect(UsageSource(statusPath: empty).read() == .failure(.noAccounts))
    }
}

struct LiveHistogramTests {
    // threshold 40000 → top = 64000 unless the data is higher; 18 rows
    func rows(_ v: Double) -> Double { v / 64000 * 18 }
    let epoch = 6_000.0 // a whole minute: chart time 0 = minute 100

    @Test func showsTheValuesAsTheyAre() {
        let h = LiveHistogram(values: Array(repeating: 32000, count: 60), threshold: 40000, minute: 100, epoch: epoch)
        #expect(h.heights(at: 10, still: false).allSatisfy { abs($0 - 9) < 1e-9 })
        #expect(h.thresholdRow == 18 - 11)
    }

    @Test func shiftsWhenTheWallClockMinuteCloses() {
        let values = (0..<60).map { Double(1000 * ($0 + 1)) }
        let h = LiveHistogram(values: values, threshold: 40000, minute: 100, epoch: epoch)
        _ = h.heights(at: 59, still: false)
        let atShift = h.heights(at: 60, still: false)
        #expect(abs(atShift[10] - rows(11000)) < 1e-9) // eases from where it was
        let settled = h.heights(at: 60.5, still: false)
        #expect(abs(settled[10] - rows(12000)) < 1e-9)
        #expect(settled[59] == 1) // the new minute starts empty
    }

    @Test func newDataEasesInFromTheScreen() {
        let h = LiveHistogram(values: Array(repeating: 16000, count: 60), threshold: 40000, minute: 100, epoch: epoch)
        _ = h.heights(at: 10, still: false)
        h.update(values: Array(repeating: 32000, count: 60), threshold: 40000, minute: 100, at: 10)
        #expect(abs(h.heights(at: 10, still: false)[0] - rows(16000)) < 1e-9)
        let mid = h.heights(at: 10.25, still: false)[0]
        #expect(mid > rows(16000) && mid < rows(32000))
        #expect(abs(h.heights(at: 10.5, still: false)[0] - rows(32000)) < 1e-9)
    }

    @Test func catchesUpAfterSeveralMinutesUnseen() {
        let h = LiveHistogram(values: Array(repeating: 32000, count: 60), threshold: 40000, minute: 100, epoch: epoch)
        let after = h.heights(at: 3 * 60 + 5, still: false) // three minutes closed while nothing was drawn
        #expect(after.suffix(3).allSatisfy { abs($0 - 9) < 1e-9 }) // they ease down from what was on screen
        #expect(h.heights(at: 3 * 60 + 6, still: false).suffix(3).allSatisfy { $0 == 1 })
    }
}
