import Foundation
import SQLite3
import Testing
@testable import Cooked

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

/// The connected accounts of the tests, with the names and order the island had before the account store.
let testAccounts = [("claudeAgent_work", "work"), ("claudeAgent_team", "team"), ("claudeAgent", "personal"),
                    ("claudeAgent_side", "side"), ("codex", "codex")]
    .map { ConnectedAccount(id: $0.0, name: $0.1, path: "/h/" + $0.0) }

/// A source reading only the limit logs in `dir`.
func source(_ dir: String) -> UsageSource {
    var s = UsageSource(accounts: testAccounts, ledger: MinuteLedger())
    s.limits = LimitLog(dir: dir)
    s.databasePath = "/nonexistent"
    s.transcripts = nil
    s.codex = nil
    return s
}

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
        let s = try #require(LiveSnapshot.make(status: try status(), minutes: minutes, now: now, calendar: calendar, accounts: testAccounts))
        // only accounts with 5h data, work first
        #expect(s.accounts.map(\.short) == ["work", "personal"])
        var work = s.accounts[0]
        // even pace: 3 h 34 min left of the 5 h window, about 5 h 54 min of the week
        #expect(abs((work.fiveHourElapsed ?? 0) - 0.2862) < 0.001)
        #expect(abs((work.weekElapsed ?? 0) - 0.9490) < 0.001)
        #expect(work.weekReset == "Fri")
        work.fiveHourElapsed = nil
        work.weekElapsed = nil
        work.weekReset = nil
        #expect(work == Account(id: "claudeAgent_work", short: "work", fiveHourPercent: 48, fiveHourReset: "19:00", isLive: true,
                               fullAt: "18:04", weekPercent: 87))
        #expect(s.accounts[1].weekPercent == 81)
        // both live: only work has minutes in the last 15
        #expect(s.activeID == "claudeAgent_work")
        // the chart's last 4 whole minutes and the 51 s of this one
        #expect(s.rateNow == Int(Double(4189 + 27004) / (4 + 51.0 / 60)))
        #expect(s.threshold == 617_094 / 15)
        #expect(s.series.count == 1440) // a day, for the 1d range
        #expect(s.series.last == 4189)
        #expect(s.series[1438] == 27004)
        #expect(s.series[1380] == 100) // the hour's first minute
        #expect(s.clock == "15:25")
        // 18:04 is 158 min away: not urgent
        #expect(s.urgency == nil)
    }

    /// The popover's chart sums every account; the island keeps the active one (2026-10-30-aggregated-popover-chart).
    @Test func popoverChartSumsEveryAccount() throws {
        var minutes = TokenLog.Minutes()
        minutes.byAccount["claudeAgent_work"] = [minuteNow: 4000, minuteNow - 1: 20000]
        minutes.byAccount["claudeAgent"] = [minuteNow: 1000, minuteNow - 2: 3000]
        minutes.byAccount["codex"] = [minuteNow - 1: 500]
        var st = try status()
        st.totalThreshold15m = 900_000
        let s = try #require(LiveSnapshot.make(status: st, minutes: minutes, now: now, calendar: calendar, accounts: testAccounts))
        #expect(s.activeID == "claudeAgent_work")
        #expect(s.series.last == 5000)
        #expect(s.series[1438] == 20500)
        #expect(s.series[1437] == 3000)
        let gone = 4 + 51.0 / 60
        #expect(s.rateNow == Int(24000 / gone))
        #expect(s.totalRate == Int(28500 / gone))
        #expect(s.threshold == 617_094 / 15)
        #expect(s.chartThreshold == 60_000)
        // without a summed p99, the largest account threshold
        st.totalThreshold15m = nil
        #expect(LiveSnapshot.make(status: st, minutes: minutes, now: now, calendar: calendar, accounts: testAccounts)?.chartThreshold == 617_094 / 15)
    }

    @Test func urgentWhenFiveHourReachesNinety() throws {
        let st = try status { $0.accounts["claudeAgent"]?.fiveHourPct = 91 }
        let s = try #require(LiveSnapshot.make(status: st, minutes: .init(), now: now, calendar: calendar, accounts: testAccounts))
        let u = try #require(s.urgency)
        #expect(s.activeID == "claudeAgent")
        #expect(u.percent == 91)
        #expect(u.left == 9) // the card counts what is left, like the popover
        // no ETA: the headline gives the reset, not "full at —"
        #expect(u.lead == "resets ")
        #expect(u.fullAt == "20:20")
        #expect(u.detail == "personal")
        #expect(u.source == "Idle for 5 min") // no minutes in this fixture: never "0/min"
    }

    @Test func urgentWhenFullWithinTwentyMinutes() throws {
        let st = try status { $0.accounts["claudeAgent_work"]?.projection = .init(window: "5 min", eta: "15:40") }
        // the card names the thread its button opens: work's top source, not the overall top one
        let sources = [TokenSource(id: "p", threadID: "tp", project: "p", detail: "Other account", account: "personal",
                                   rate: 300_000, agents: nil),
                       TokenSource(id: "w", threadID: "tw", project: "p", detail: "Refactor usage page", account: "work",
                                   rate: 184_000, agents: nil)]
        let s = try #require(LiveSnapshot.make(status: st, minutes: .init(), now: now, calendar: calendar, sources: sources, accounts: testAccounts))
        let u = try #require(s.urgency)
        #expect(u.lead == "full at ")
        #expect(u.fullAt == "15:40")
        #expect(u.detail == "work in 14 min")
        #expect(u.source == "184k/min from “Refactor usage page”")
        #expect(IslandView.peakThread(s) == "tw")
    }

    @Test func urgentOnASpikeMinute() throws {
        var minutes = TokenLog.Minutes()
        minutes.byAccount["claudeAgent"] = [minuteNow - 1: 425_706 / 15 + 1]
        let s = try #require(LiveSnapshot.make(status: try status(), minutes: minutes, now: now, calendar: calendar, accounts: testAccounts))
        #expect(s.activeID == "claudeAgent")
        #expect(s.isUrgent)
        // an older spike does not count
        minutes.byAccount["claudeAgent"] = [minuteNow - 2: 1_000_000]
        #expect(LiveSnapshot.make(status: try status(), minutes: minutes, now: now, calendar: calendar, accounts: testAccounts)?.isUrgent == false)
    }

    @Test func urgentWhenTheLimitIsHit() throws {
        let st = try status { $0.accounts["claudeAgent_work"]?.limitStatus = "rejected" }
        #expect(LiveSnapshot.make(status: st, minutes: .init(), now: now, calendar: calendar, accounts: testAccounts)?.urgency != nil)
        let warn = try status { $0.accounts["claudeAgent_work"]?.limitStatus = "allowed_warning" }
        #expect(LiveSnapshot.make(status: warn, minutes: .init(), now: now, calendar: calendar, accounts: testAccounts)?.urgency == nil)
    }

    @Test func staleAndIdleAccountsAreNeverUrgent() throws {
        let stale = try status {
            $0.accounts["claudeAgent"]?.fiveHourPct = 97
            $0.accounts["claudeAgent"]?.note = "5h window already reset, no new data"
        }
        let s = try #require(LiveSnapshot.make(status: stale, minutes: .init(), now: now, calendar: calendar, accounts: testAccounts))
        #expect(s.urgency == nil)
        #expect(s.accounts[1].fiveHourPercent == 0)
        #expect(s.accounts[1].fiveHourReset == "—")
        let idle = try status {
            $0.accounts["claudeAgent"]?.fiveHourPct = 97
            $0.accounts["claudeAgent"]?.live = false
        }
        #expect(LiveSnapshot.make(status: idle, minutes: .init(), now: now, calendar: calendar, accounts: testAccounts)?.urgency == nil)
    }

    @Test func noFiveHourDataMeansNoSnapshot() throws {
        let st = try status { $0.accounts = $0.accounts.filter { $0.key == "codex" } }
        #expect(LiveSnapshot.make(status: st, minutes: .init(), now: now, calendar: calendar, accounts: testAccounts) == nil)
    }

    @Test func fullAtComesFromTheProjection() throws {
        let st = try status {
            $0.accounts["claudeAgent"]?.fiveHourPct = 103
            $0.accounts["claudeAgent_work"]?.projection = nil
        }
        let s = try #require(LiveSnapshot.make(status: st, minutes: .init(), now: now, calendar: calendar, accounts: testAccounts))
        // no projection: not expected to fill up; at 100% and over: already full
        #expect(s.accounts.map(\.fullAt) == [nil, "now"])
        let withProjection = try #require(LiveSnapshot.make(status: try status(), minutes: .init(), now: now, calendar: calendar, accounts: testAccounts))
        #expect(withProjection.accounts[0].fullAt == "18:04")
    }

    /// A projection after the reset says nothing: the row leaves "full at" empty.
    @Test func fullAtOnlyBeforeTheReset() throws {
        let late = try status { $0.accounts["claudeAgent_work"]?.projection = .init(window: "30 min", eta: "19:30") }
        #expect(LiveSnapshot.make(status: late, minutes: .init(), now: now, calendar: calendar, accounts: testAccounts)?.accounts[0].fullAt == nil)
        let atReset = try status { $0.accounts["claudeAgent_work"]?.projection = .init(window: "30 min", eta: "19:00") }
        #expect(LiveSnapshot.make(status: atReset, minutes: .init(), now: now, calendar: calendar, accounts: testAccounts)?.accounts[0].fullAt == nil)
    }

    /// The island follows the account the chart's minutes (the transcripts) show burning most, not the database's
    /// finished turns: on 30/10 13:44 the database had work ahead while team burned 5 times more.
    @Test func activeAccountIsTheOneTheMinutesShowBurning() throws {
        let st = try status { $0.accounts["claudeAgent"]?.tokensFresh?["15m"] = 1 }
        var minutes = TokenLog.Minutes()
        minutes.byAccount["claudeAgent"] = [minuteNow - 3: 500_000]
        minutes.byAccount["claudeAgent_work"] = [minuteNow - 3: 100_000]
        let s = try #require(LiveSnapshot.make(status: st, minutes: minutes, now: now, calendar: calendar, accounts: testAccounts))
        #expect(s.activeID == "claudeAgent")
    }

    /// The island counts what is left, like the popover: 18% used is 82%, a blocked account is 0%.
    @Test func islandCountsWhatIsLeft() {
        var a = Account(id: "x", short: "x", fiveHourPercent: 18, fiveHourReset: "20:20", isLive: true)
        #expect(a.fiveHourLeft == 82)
        a.isBlocked = true
        #expect(a.fiveHourLeft == 0)
        #expect(Account(id: "y", short: "y", fiveHourPercent: 103, fiveHourReset: "-", isLive: true).fiveHourLeft == 0)
    }

    /// An ETA already gone by says nothing: no "full 15:20" at 15:25, no "in 0 min" peak.
    @Test func etaInThePastIsDropped() throws {
        let st = try status { $0.accounts["claudeAgent_work"]?.projection = .init(window: "5 min", eta: "15:20") }
        let s = try #require(LiveSnapshot.make(status: st, minutes: .init(), now: now, calendar: calendar, accounts: testAccounts))
        #expect(s.accounts[0].fullAt == nil)
        #expect(s.urgency == nil)
    }

    /// A live blocked account in the peak card: "blocked until" the reset; a thread under 1 token/min is not named.
    @Test func blockedPeakSaysBlockedUntil() throws {
        let st = try status {
            $0.accounts["claudeAgent"]?.fiveHourPct = 103
            $0.accounts["claudeAgent"]?.limitStatus = "rejected"
        }
        let slow = [TokenSource(id: "o", project: "p", detail: "Old thread", account: "personal", rate: 0.2, agents: nil)]
        let u = try #require(LiveSnapshot.make(status: st, minutes: .init(), now: now, calendar: calendar, sources: slow, accounts: testAccounts)?.urgency)
        #expect(u.lead == "free at ")
        #expect(u.fullAt == "20:20")
        #expect(u.detail == "personal is cooked")
        #expect(u.source == "Idle for 5 min")
    }

    /// `limit_status` "rejected": "blocked until" the 5h reset, no "full at".
    @Test func rejectedAccountIsBlockedUntilTheReset() throws {
        let st = try status {
            $0.accounts["claudeAgent"]?.fiveHourPct = 103
            $0.accounts["claudeAgent"]?.limitStatus = "rejected"
            $0.accounts["claudeAgent"]?.live = false
            $0.accounts["claudeAgent"]?.lastEventAt = "13:42"
        }
        let s = try #require(LiveSnapshot.make(status: st, minutes: .init(), now: now, calendar: calendar, accounts: testAccounts))
        let personal = s.accounts[1]
        #expect(personal.isBlocked)
        #expect(personal.fiveHourReset == "20:20")
        #expect(personal.fullAt == nil)
        #expect(!personal.isUnused)
        #expect(personal.weekPercent == 81)
        #expect(!s.accounts[0].isBlocked)
        // a stale window is not blocked any more
        let stale = try status {
            $0.accounts["claudeAgent"]?.limitStatus = "rejected"
            $0.accounts["claudeAgent"]?.note = "5h window already reset, no new data"
        }
        #expect(LiveSnapshot.make(status: stale, minutes: .init(), now: now, calendar: calendar, accounts: testAccounts)?.accounts[1].isBlocked == false)
    }

    /// No event in the last 5 hours: the account is hidden from the popover.
    @Test func unusedAccountsFold() throws {
        let st = try status {
            $0.accounts["codex"]?.fiveHourPct = 35
            $0.accounts["codex"]?.fiveHourResetsAt = "24/10 03:55"
            $0.accounts["codex"]?.sevenDayPct = 6
            $0.accounts["codex"]?.sevenDayResetsAt = "30/10 21:55"
            $0.accounts["codex"]?.lastEventAt = "24/10 00:52"
            $0.accounts["codex"]?.note = "5h window already reset, no new data"
            $0.accounts["claudeAgent"]?.lastEventAt = "10:26" // 4 h 59 min ago: still used today
        }
        let s = try #require(LiveSnapshot.make(status: st, minutes: .init(), now: now, calendar: calendar, accounts: testAccounts))
        #expect(s.rows.map(\.short) == ["work", "personal"])
        #expect(s.unused.map(\.short) == ["codex"])
        #expect(s.unused[0].weekPercent == 6)

        let old = try status { $0.accounts["claudeAgent"]?.lastEventAt = "10:20" }
        #expect(LiveSnapshot.make(status: old, minutes: .init(), now: now, calendar: calendar, accounts: testAccounts)?.unused.map(\.short) == ["personal"])
        // without a last event: not live and no tokens in the last 15 minutes
        let quiet = try status {
            $0.accounts["claudeAgent"]?.lastEventAt = nil
            $0.accounts["claudeAgent"]?.live = false
            $0.accounts["claudeAgent"]?.tokensFresh = ["15m": 0]
        }
        #expect(LiveSnapshot.make(status: quiet, minutes: .init(), now: now, calendar: calendar, accounts: testAccounts)?.unused.map(\.short) == ["personal"])
        let busy = try status {
            $0.accounts["claudeAgent"]?.lastEventAt = nil
            $0.accounts["claudeAgent"]?.live = false
        }
        #expect(LiveSnapshot.make(status: busy, minutes: .init(), now: now, calendar: calendar, accounts: testAccounts)?.unused.isEmpty == true)
    }

    /// The account the island shows always keeps its row.
    @Test func activeAccountNeverFolds() throws {
        let st = try status {
            for id in ["claudeAgent", "claudeAgent_work"] { $0.accounts[id]?.lastEventAt = "01/10 10:00" }
        }
        let s = try #require(LiveSnapshot.make(status: st, minutes: .init(), now: now, calendar: calendar, accounts: testAccounts))
        #expect(s.rows.map(\.id) == [s.activeID])
    }

    /// A week that reset with no new data shows no week %.
    @Test func staleWeekHasNoPercent() throws {
        let st = try status { $0.accounts["claudeAgent"]?.sevenDayResetsAt = "16/10 16:00" }
        #expect(LiveSnapshot.make(status: st, minutes: .init(), now: now, calendar: calendar, accounts: testAccounts)?.accounts[1].weekPercent == nil)
    }

    @Test func shortNames() {
        #expect(LiveSnapshot.short(id: "claudeAgent_work", accounts: testAccounts) == "work")
        #expect(LiveSnapshot.short(id: "claudeAgent_team", accounts: testAccounts) == "team")
        #expect(LiveSnapshot.short(id: "claudeAgent_new", accounts: testAccounts) == "claudeAgent_new") // not in the store
    }
}

struct TokenLogTests {
    /// A database with the two tables the query reads, and the thread titles.
    func makeDatabase(_ turns: [(instance: String, thread: String, completed: String, input: Int, cached: Int, output: Int)],
                      started: [Int: String] = [:], running: [(instance: String, started: String, json: String)] = []) throws -> String {
        let path = FileManager.default.temporaryDirectory.appendingPathComponent("tokenlog-\(UUID().uuidString).sqlite").path
        var db: OpaquePointer?
        #expect(sqlite3_open(path, &db) == SQLITE_OK)
        defer { sqlite3_close(db) }
        var sql = """
            CREATE TABLE orchestration_v2_projection_provider_turns (provider_turn_id TEXT PRIMARY KEY, thread_id TEXT,
              provider_thread_id TEXT, started_at TEXT, completed_at TEXT, payload_json TEXT);
            CREATE TABLE orchestration_v2_projection_provider_threads (provider_thread_id TEXT PRIMARY KEY, provider TEXT,
              provider_instance_id TEXT, thread_id TEXT, payload_json TEXT);
            CREATE TABLE orchestration_v2_projection_threads (thread_id TEXT PRIMARY KEY, title TEXT, project_id TEXT);
            CREATE TABLE projection_projects (project_id TEXT PRIMARY KEY, title TEXT);
            INSERT INTO projection_projects VALUES ('p-brain', 'garden-notes'), ('p-none', 'No project');
            """
        for (i, t) in turns.enumerated() {
            sql += """
                INSERT OR IGNORE INTO orchestration_v2_projection_provider_threads VALUES ('pt-\(t.thread)', 'claudeAgent', '\(t.instance)',
                  '\(t.thread)', '{"nativeThreadRef":{"nativeId":"session-\(t.thread)"}}');
                INSERT OR IGNORE INTO orchestration_v2_projection_threads VALUES ('\(t.thread)', 'Thread \(t.thread)',
                  '\(t.thread == "c" ? "p-none" : "p-brain")');
                INSERT INTO orchestration_v2_projection_provider_turns VALUES ('turn-\(i)', '\(t.thread)', 'pt-\(t.thread)',
                  \(started[i].map { "'\($0)'" } ?? "NULL"), '\(t.completed)',
                  '{"turnTokenUsage":{"inputTokens":\(t.input),"cachedInputTokens":\(t.cached),"outputTokens":\(t.output)}}');
                """
        }
        for (i, r) in running.enumerated() {
            sql += """
                INSERT OR IGNORE INTO orchestration_v2_projection_provider_threads VALUES ('pt-r\(i)', 'claudeAgent', '\(r.instance)', NULL, '{}');
                INSERT INTO orchestration_v2_projection_provider_turns VALUES ('run-\(i)', 'r\(i)', 'pt-r\(i)', '\(r.started)', NULL, '\(r.json)');
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
    }

    @Test func spreadsAFinishedTurnOverTheMinutesItRan() throws {
        let now = ISO8601DateFormatter().date(from: "2026-10-29T14:25:51Z")!
        let minute = Int(now.timeIntervalSince1970) / 60
        // 3000 tokens over 14:22:30 → 14:25:30: half a minute, two whole minutes, half a minute
        let path = try makeDatabase([("claudeAgent_work", "a", "2026-10-29T14:25:30.000Z", 3000, 0, 0)],
                                    started: [0: "2026-10-29T14:22:30.000Z"])
        let m = try TokenLog(path: path).read(now: now)
        #expect(m.byAccount["claudeAgent_work"] == [minute - 3: 500, minute - 2: 1000, minute - 1: 1000, minute: 500])
    }

    @Test func spreadKeepsTheTotal() {
        let parts = TokenLog.spread(7000, from: 100, to: 1000)
        #expect(abs(parts.values.reduce(0, +) - 7000) < 1e-6)
        #expect(parts.keys.sorted() == Array(1...16))
        #expect(TokenLog.spread(500, from: 120, to: 120) == [2: 500])
        #expect(TokenLog.spread(0, from: 0, to: 600).isEmpty)
    }

    @Test func countsARunningTurnBetweenReadings() throws {
        let now = ISO8601DateFormatter().date(from: "2026-10-29T14:25:51Z")!
        let minute = Int(now.timeIntervalSince1970) / 60
        func usage(_ input: Int, _ cached: Int, _ output: Int, _ at: String) -> String {
            #"{"tokenUsage":{"inputTokens":\#(input),"cachedInputTokens":\#(cached),"outputTokens":\#(output),"updatedAt":"\#(at)"}}"#
        }
        let running = RunningTurns()
        // first reading: only the last call is known, 1000 new input + 200 output
        let first = try makeDatabase([], running: [("claudeAgent_work", "2026-10-29T14:20:00.000Z", usage(150_000, 149_000, 200, "2026-10-29T14:24:10.000Z"))])
        #expect(try TokenLog(path: first).read(now: now, running: running).byAccount["claudeAgent_work"] == [minute - 1: 1200])
        // later: the context grew by 30000 over several calls, so they all count
        let second = try makeDatabase([], running: [("claudeAgent_work", "2026-10-29T14:20:00.000Z", usage(180_000, 179_500, 300, "2026-10-29T14:25:20.000Z"))])
        #expect(try TokenLog(path: second).read(now: now, running: running).byAccount["claudeAgent_work"] == [minute - 1: 1200, minute: 30300])
        // the same reading again adds nothing
        #expect(try TokenLog(path: second).read(now: now, running: running).byAccount["claudeAgent_work"] == [minute - 1: 1200, minute: 30300])
        // a finished turn leaves the running set: its total comes from the finished query instead
        let done = try makeDatabase([])
        #expect(try TokenLog(path: done).read(now: now, running: running).byAccount["claudeAgent_work"] == nil)
    }

    @Test func recentThreadsAndSessionThreads() throws {
        let now = ISO8601DateFormatter().date(from: "2026-10-29T14:25:51Z")!
        let path = try makeDatabase([
            ("codex", "a", "2026-10-29T14:25:10.000Z", 1000, 200, 50),        // 850
            ("codex", "a", "2026-10-29T14:20:00.000Z", 150, 0, 0),            // 150
            ("claudeAgent", "c", "2026-10-29T14:22:00.000Z", 300, 0, 0),
            ("claudeAgent", "c", "2026-10-29T14:05:00.000Z", 9999, 0, 0),     // over 15 min ago
        ])
        let log = TokenLog(path: path)
        let m = try log.read(now: now)
        #expect(m.recentThreads == [
            .init(account: "codex", thread: .init(id: "a", title: "Thread a", project: "garden-notes"), tokens: 1000),
            .init(account: "claudeAgent", thread: .init(id: "c", title: "Thread c", project: nil), tokens: 300),
        ])
        let threads = try log.threads(sessions: ["session-c", "session-x", "it's"])
        #expect(threads == ["session-c": .init(id: "c", title: "Thread c", project: nil)])
    }

    @Test func missingDatabaseThrows() {
        #expect(throws: TokenLog.Failure.self) { try TokenLog(path: "/nonexistent/statev2.sqlite").read(now: Date()) }
    }
}

struct TranscriptLogTests {
    let now = ISO8601DateFormatter().date(from: "2026-10-29T14:25:51Z")!
    var minute: Int { Int(now.timeIntervalSince1970) / 60 }

    func line(_ id: String, _ at: String, input: Int, write: Int, read: Int, output: Int) -> String {
        #"{"type":"assistant","timestamp":"\#(at)","message":{"id":"\#(id)","usage":{"input_tokens":\#(input),"cache_creation_input_tokens":\#(write),"cache_read_input_tokens":\#(read),"output_tokens":\#(output)}}}"#
    }

    func makeHome() throws -> (home: String, file: URL) {
        let home = FileManager.default.temporaryDirectory.appendingPathComponent("home-\(UUID().uuidString)").path
        let dir = URL(fileURLWithPath: TranscriptLog.dir(for: "claudeAgent_work", home: home) + "/projects/p/s/subagents")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return (home, dir.appendingPathComponent("agent-1.jsonl"))
    }

    @Test func configDirs() {
        #expect(TranscriptLog.dir(for: "claudeAgent", home: "/h") == "/h/.claude")
        #expect(TranscriptLog.dir(for: "claudeAgent_team", home: "/h") == "/h/.claude-team")
        #expect(TranscriptLog.dir(for: "claudeAgent_side", home: "/h") == "/h/.claude-side")
    }

    @Test func countsNewTokensPerMinuteOnceAndReadsOnlyNewLines() throws {
        let (home, file) = try makeHome()
        let text = [
            line("m1", "2026-10-29T14:24:10.000Z", input: 2, write: 1000, read: 90000, output: 100), // 1102
            line("m1", "2026-10-29T14:24:11.000Z", input: 2, write: 1000, read: 90000, output: 100), // same message, second block
            #"{"type":"user","timestamp":"2026-10-29T14:24:12.000Z","message":{"content":"tool output"}}"#,
            line("m2", "2026-10-29T14:25:05.000Z", input: 5, write: 0, read: 91000, output: 400),     // 405
            line("old", "2026-10-29T13:10:00.000Z", input: 9999, write: 0, read: 0, output: 0),      // over an hour ago
        ].joined(separator: "\n") + "\n"
        try text.write(to: file, atomically: true, encoding: .utf8)
        let log = TranscriptLog(dirs: ["claudeAgent_work": TranscriptLog.dir(for: "claudeAgent_work", home: home),
                                       "claudeAgent": TranscriptLog.dir(for: "claudeAgent", home: home)])
        let first = log.read(now: now)
        #expect(first["claudeAgent_work"] == [minute - 1: 1102, minute: 405])
        #expect(first["claudeAgent"] == nil) // no directory: the database stays the source
        // a half-written line waits for its newline
        let more = line("m3", "2026-10-29T14:25:30.000Z", input: 0, write: 0, read: 0, output: 50)
        let handle = try FileHandle(forWritingTo: file)
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(more.prefix(40).utf8))
        #expect(log.read(now: now)["claudeAgent_work"] == [minute - 1: 1102, minute: 405])
        try handle.write(contentsOf: Data((more.dropFirst(40) + "\n").utf8))
        try handle.close()
        #expect(log.read(now: now)["claudeAgent_work"] == [minute - 1: 1102, minute: 455])
    }
}

extension TranscriptLogTests {
    @Test func firstReadStartsNearTheWindow() throws {
        let (_, file) = try makeHome()
        // 300 lines, one per minute from 10:00; each about 200 bytes
        let start = ISO8601DateFormatter().date(from: "2026-10-29T10:00:00Z")!
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        let lines = (0..<300).map { line("m\($0)", f.string(from: start.addingTimeInterval(Double($0) * 60)), input: 1, write: 0, read: 0, output: 0) }
        try (lines.joined(separator: "\n") + "\n").write(to: file, atomically: true, encoding: .utf8)
        let since = start.addingTimeInterval(240 * 60)
        let offset = TranscriptLog.start(of: file, after: since, block: 4096)
        let text = try String(contentsOf: file, encoding: .utf8)
        let tail = String(text.utf8.dropFirst(Int(offset)))!
        #expect(offset > 0)
        #expect(tail.hasPrefix(#"{"type""#)) // a line start
        let firstMinute = Int(tail.split(separator: "\n")[0].split(separator: "\"")[13].dropFirst())! // "m<n>"
        #expect(firstMinute <= 240 && firstMinute > 200) // before the window, but not far before
        #expect(TranscriptLog.start(of: file, after: start.addingTimeInterval(-60), block: 4096) == 0)
    }
}

struct UsageSourceTests {
    @Test func readsTheLimitsFromTheProviderLogs() throws {
        let now = Date()
        let dir = try LimitLogTests.logDir([LimitLogTests.claudeLine(at: now.addingTimeInterval(-60), account: "claudeAgent_work",
                                                                    five: 0.48, fiveReset: now.addingTimeInterval(3600))])
        let reading = try source(dir).read(now: now).get()
        #expect(reading.snapshot.accounts.map(\.short) == ["work"])
        #expect(reading.snapshot.accounts[0].fiveHourPercent == 48)
        #expect(reading.snapshot.series == Array(repeating: 0, count: 1440)) // no database: empty histogram
    }

    @Test func reportsNoAccounts() throws {
        let dir = try LimitLogTests.logDir([])
        #expect(source(dir).read() == .failure(.noAccounts))
    }
}

struct LiveHistogramTests {
    let epoch = 6_000.0 // a whole minute: chart time 0 = minute 100

    @Test func showsTheValuesAsTheyAre() {
        let h = LiveHistogram(values: Array(repeating: 32000, count: 60), threshold: 40000, minute: 100, epoch: epoch)
        let history = h.history(at: 10, still: false)
        #expect(history.values.allSatisfy { $0 == 32000 })
        #expect(history.top == 64000 && history.threshold == 40000)
    }

    @Test func shiftsWhenTheWallClockMinuteCloses() {
        let values = (0..<60).map { Double(1000 * ($0 + 1)) }
        let h = LiveHistogram(values: values, threshold: 40000, minute: 100, epoch: epoch)
        #expect(h.history(at: 59, still: false).values[10] == 11000)
        let after = h.history(at: 60, still: false)
        #expect(after.values[10] == 12000)
        #expect(after.values[59] == 0) // the new minute starts empty
    }

    @Test func newDataReplacesTheHistory() {
        let h = LiveHistogram(values: Array(repeating: 16000, count: 60), threshold: 40000, minute: 100, epoch: epoch)
        _ = h.history(at: 10, still: false)
        h.update(values: Array(repeating: 32000, count: 60), threshold: 30000, minute: 100)
        let history = h.history(at: 10, still: false)
        #expect(history.values.allSatisfy { $0 == 32000 })
        #expect(history.threshold == 30000)
    }

    @Test func catchesUpAfterSeveralMinutesUnseen() {
        let h = LiveHistogram(values: Array(repeating: 32000, count: 60), threshold: 40000, minute: 100, epoch: epoch)
        let after = h.history(at: 3 * 60 + 5, still: false) // three minutes closed while nothing was drawn
        #expect(after.values.suffix(3).allSatisfy { $0 == 0 })
        #expect(after.values.prefix(57).allSatisfy { $0 == 32000 })
    }
}

extension TranscriptLogTests {
    /// A session's own file and its subagents' files make one session; the cwd comes from the session's own calls.
    @Test func groupsSubagentsUnderTheirSession() throws {
        let (home, sub) = try makeHome() // projects/p/s/subagents/agent-1.jsonl
        let main = sub.deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("s.jsonl")
        func call(_ id: String, _ at: String, _ tokens: Int, cwd: String) -> String {
            #"{"type":"assistant","timestamp":"\#(at)","cwd":"\#(cwd)","message":{"id":"\#(id)","usage":{"input_tokens":\#(tokens),"output_tokens":0}}}"#
        }
        try (call("m1", "2026-10-29T14:24:00.000Z", 3000, cwd: "/r/cooked") + "\n").write(to: main, atomically: true, encoding: .utf8)
        try (call("s1", "2026-10-29T14:25:00.000Z", 1500, cwd: "/tmp/wt") + "\n").write(to: sub, atomically: true, encoding: .utf8)
        let other = sub.deletingLastPathComponent().appendingPathComponent("agent-2.jsonl")
        try (call("s2", "2026-10-29T14:15:00.000Z", 1500, cwd: "/tmp/wt") + "\n").write(to: other, atomically: true, encoding: .utf8)
        let r = TranscriptLog(dirs: ["claudeAgent_work": TranscriptLog.dir(for: "claudeAgent_work", home: home)]).read(now: now)
        #expect(r.sessions == [.init(account: "claudeAgent_work", id: "s", cwd: "/r/cooked", tokens: 6000,
                                     mainActive: true, activeSubagents: 1)])
    }
}

struct TokenSourcesTests {
    let thread = TokenLog.Thread(id: "t1", title: "popover data", project: "cooked")

    @Test func namesSessionsByTheirT3ThreadInRateOrder() {
        let sessions: [TranscriptLog.Session] = [
            .init(account: "claudeAgent_work", id: "s1", cwd: "/r/cooked", tokens: 1_695_000, mainActive: true, activeSubagents: 3),
            .init(account: "claudeAgent_team", id: "s2", cwd: "/Users/m/.t3/scratch/2026-x", tokens: 60_000, mainActive: false, activeSubagents: 0),
            .init(account: "claudeAgent", id: "s3", cwd: nil, tokens: 10, mainActive: true, activeSubagents: 0), // under 1/min
            .init(account: "claudeAgent", id: "s4", cwd: "/r/atlas", tokens: 15_000, mainActive: false, activeSubagents: 2),
        ]
        let recent: [TokenLog.RecentThread] = [
            .init(account: "codex", thread: .init(id: "t9", title: "Fix CI", project: nil), tokens: 30_000),
            .init(account: "claudeAgent_work", thread: thread, tokens: 999_999), // has transcripts: counted from them
        ]
        let out = TokenSources.make(sessions: sessions, threads: ["s1": thread], recent: recent,
                                    transcriptAccounts: ["claudeAgent_work", "claudeAgent_team", "claudeAgent"], accounts: testAccounts)
        #expect(out == [
            .init(id: "t1", threadID: "t1", project: "cooked", detail: "popover data", account: "work", rate: 113_000, agents: "1 agent + 3 sub"),
            .init(id: "s2", project: "t3 scratch", detail: "Claude Code session", account: "team", rate: 4000, agents: nil),
            .init(id: "t9", threadID: "t9", project: "t3 scratch", detail: "Fix CI", account: "codex", rate: 2000, agents: nil),
            .init(id: "s4", project: "atlas", detail: "Claude Code session", account: "personal", rate: 1000, agents: "2 sub"),
        ])
        #expect(TokenSources.make(sessions: [], threads: [:], recent: [], transcriptAccounts: [], accounts: testAccounts).isEmpty)
    }

    /// The card shows 3 and "N more"; the list behind it stops at 8 (2026-10-30-sources-expand).
    @Test func keepsUpToEightSources() {
        let sessions = (1...10).map { TranscriptLog.Session(account: "claudeAgent", id: "s\($0)", cwd: nil, tokens: Double($0) * 1500,
                                                            mainActive: true, activeSubagents: 0) }
        let out = TokenSources.make(sessions: sessions, threads: [:], recent: [], transcriptAccounts: [], accounts: testAccounts)
        #expect(out.map(\.id) == ["s10", "s9", "s8", "s7", "s6", "s5", "s4", "s3"])
    }

    @Test func mergesSessionsOfOneThread() {
        let sessions: [TranscriptLog.Session] = [
            .init(account: "claudeAgent_work", id: "a", cwd: nil, tokens: 15_000, mainActive: false, activeSubagents: 0),
            .init(account: "claudeAgent_work", id: "b", cwd: nil, tokens: 30_000, mainActive: true, activeSubagents: 0),
        ]
        let out = TokenSources.make(sessions: sessions, threads: ["a": thread, "b": thread], recent: [], transcriptAccounts: [], accounts: testAccounts)
        #expect(out.map(\.rate) == [3000])
        #expect(out.first?.agents == "1 agent")
    }

    @Test func agentLabels() {
        #expect(TokenSources.agents(main: true, subagents: 0) == "1 agent")
        #expect(TokenSources.agents(main: true, subagents: 3) == "1 agent + 3 sub")
        #expect(TokenSources.agents(main: false, subagents: 2) == "2 sub")
        #expect(TokenSources.agents(main: false, subagents: 0) == nil)
        #expect(TokenSources.project(cwd: "/Users/m/Desktop/GitHub/garden-notes") == "garden-notes")
        #expect(TokenSources.project(cwd: nil) == "unknown")
    }
}

struct T3CodeTests {
    @Test func threadLink() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("t3-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let file = dir.appendingPathComponent("environment-id")
        try "6ee1c3a6-fb6a\n".write(to: file, atomically: true, encoding: .utf8)
        #expect(T3Code.link(thread: "0c24a363", environmentFile: file.path)?.absoluteString
                == "t3code://threads/6ee1c3a6-fb6a/0c24a363")
        #expect(T3Code.link(thread: "x", environmentFile: "/nonexistent") == nil)
    }
}
