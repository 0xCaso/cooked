//! The live snapshot from the watcher's limits and the minutes, and the readers it builds on.
mod common;

use common::{
    ROME, append_raw, claude_line, log_dir, rome, source, status_fixture, temp_dir, test_accounts,
    token_source, uuid,
};
use cooked_model::clock::{self, FixedOffset, iso_date, iso_string};
use cooked_model::histogram_clock::LiveHistogram;
use cooked_model::live_snapshot::{self, Minutes};
use cooked_model::notice::Notice;
use cooked_model::status_file::{Entry, Projection, StatusFile, WatcherTime};
use cooked_model::t3_code;
use cooked_model::token_log::{Thread, TokenLog};
use cooked_model::token_sources;
use cooked_model::transcript_log::{Session, TranscriptLog};
use cooked_model::usage_snapshot::{Account, UsageSnapshot};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn now() -> f64 {
    rome(29, 10, 15, 25, 51)
}

fn minute_now() -> i64 {
    now() as i64 / 60
}

fn status(edit: impl FnOnce(&mut StatusFile)) -> StatusFile {
    let mut s = status_fixture();
    edit(&mut s);
    s
}

fn entry<'a>(s: &'a mut StatusFile, id: &str) -> &'a mut Entry {
    s.accounts.get_mut(id).expect("account in the fixture")
}

fn projection(window: &str, eta: &str) -> Option<Projection> {
    Some(Projection {
        window: window.to_owned(),
        eta: eta.to_owned(),
    })
}

fn make(st: &StatusFile, minutes: &Minutes) -> Option<UsageSnapshot> {
    live_snapshot::make(st, minutes, now(), &ROME, &[], &test_accounts())
}

fn none() -> Minutes {
    Minutes::new()
}

#[test]
fn decodes_the_watcher_file() {
    let s = status(|_| {});
    assert_eq!(
        s.accounts.keys().cloned().collect::<Vec<_>>(),
        ["claudeAgent", "claudeAgent_work", "codex"]
    );
    let work = &s.accounts["claudeAgent_work"];
    assert_eq!(work.five_hour_pct, Some(48));
    assert_eq!(
        work.tokens_fresh
            .as_ref()
            .and_then(|t| t.get("15m"))
            .copied(),
        Some(250_660)
    );
    assert_eq!(work.spike_threshold_15m, Some(617_094));
    assert_eq!(work.projection, projection("30 min", "18:04"));
    assert_eq!(s.accounts["codex"].five_hour_pct, None);
}

#[test]
fn parses_watcher_times() {
    assert_eq!(
        WatcherTime::parse("18:04", now(), &ROME),
        Some(rome(29, 10, 18, 4, 0))
    );
    assert_eq!(
        WatcherTime::parse("30/10 00:00", now(), &ROME),
        Some(rome(30, 10, 0, 0, 0))
    );
    assert_eq!(WatcherTime::parse("bad", now(), &ROME), None);
    let new_years_eve = rome(30, 12, 12, 0, 0);
    let next = WatcherTime::parse("02/01 10:00", new_years_eve, &ROME);
    assert_eq!(next.map(|t| clock::local(t, &ROME).year), Some(2027));
}

#[test]
fn calm_snapshot_from_the_fixture() {
    let m = minute_now();
    let minutes = Minutes::from([(
        "claudeAgent_work".to_owned(),
        HashMap::from([
            (m, 4189.0),
            (m - 1, 27004.0),
            (m - 59, 100.0),
            (m - 60, 999.0),
        ]),
    )]);
    let s = make(&status(|_| {}), &minutes).unwrap();
    // only accounts with 5h data, work first
    assert_eq!(
        s.accounts
            .iter()
            .map(|a| a.short.as_str())
            .collect::<Vec<_>>(),
        ["work", "personal"]
    );
    let mut work = s.accounts[0].clone();
    // even pace: 3 h 34 min left of the 5 h window, about 5 h 54 min of the week
    assert!((work.five_hour_elapsed.unwrap_or(0.0) - 0.2862).abs() < 0.001);
    assert!((work.week_elapsed.unwrap_or(0.0) - 0.9490).abs() < 0.001);
    assert_eq!(work.week_reset.as_deref(), Some("Fri"));
    work.five_hour_elapsed = None;
    work.week_elapsed = None;
    work.week_reset = None;
    let mut expected = Account::new("claudeAgent_work", "work", 48, "19:00", true);
    expected.full_at = Some("18:04".to_owned());
    expected.week_percent = Some(87);
    assert_eq!(work, expected);
    assert_eq!(s.accounts[1].week_percent, Some(81));
    // both live: only work has minutes in the last 15
    assert_eq!(s.active_id, "claudeAgent_work");
    // the chart's last 4 whole minutes and the 51 s of this one
    assert_eq!(
        s.rate_now,
        ((4189.0 + 27004.0) / (4.0 + 51.0 / 60.0)) as i64
    );
    assert_eq!(s.threshold, 617_094 / 15);
    assert_eq!(s.series.len(), 1440); // a day, for the 1d range
    assert_eq!(s.series.last(), Some(&4189.0));
    assert_eq!(s.series[1438], 27004.0);
    assert_eq!(s.series[1380], 100.0); // the hour's first minute
    assert_eq!(s.clock, "15:25");
    // 18:04 is 158 min away: not urgent
    assert_eq!(s.urgency, None);
}

/// The popover's chart sums every account; the island keeps the active one (2026-10-30-aggregated-popover-chart).
#[test]
fn popover_chart_sums_every_account() {
    let m = minute_now();
    let minutes = Minutes::from([
        (
            "claudeAgent_work".to_owned(),
            HashMap::from([(m, 4000.0), (m - 1, 20000.0)]),
        ),
        (
            "claudeAgent".to_owned(),
            HashMap::from([(m, 1000.0), (m - 2, 3000.0)]),
        ),
        ("codex".to_owned(), HashMap::from([(m - 1, 500.0)])),
    ]);
    let mut st = status(|_| {});
    st.total_threshold_15m = Some(900_000);
    let s = make(&st, &minutes).unwrap();
    assert_eq!(s.active_id, "claudeAgent_work");
    assert_eq!(s.series.last(), Some(&5000.0));
    assert_eq!(s.series[1438], 20500.0);
    assert_eq!(s.series[1437], 3000.0);
    let gone = 4.0 + 51.0 / 60.0;
    assert_eq!(s.rate_now, (24000.0 / gone) as i64);
    assert_eq!(s.total_rate, Some((28500.0 / gone) as i64));
    assert_eq!(s.threshold, 617_094 / 15);
    assert_eq!(s.chart_threshold(), 60_000);
    // without a summed p99, the largest account threshold
    st.total_threshold_15m = None;
    assert_eq!(
        make(&st, &minutes).map(|s| s.chart_threshold()),
        Some(617_094 / 15)
    );
}

/// 91% used with no projection: not red, it is not expected to fill before the reset (2026-11-16-red-account-rule).
#[test]
fn ninety_percent_alone_is_not_urgent() {
    let st = status(|s| entry(s, "claudeAgent").five_hour_pct = Some(91));
    let s = make(&st, &none()).unwrap();
    assert_eq!(s.urgency, None);
}

#[test]
fn urgent_when_full_within_twenty_minutes() {
    let st = status(|s| entry(s, "claudeAgent_work").projection = projection("5 min", "15:40"));
    // the card names the thread its button opens: work's top source, not the overall top one
    let sources = [
        token_source(
            "p",
            Some("tp"),
            "p",
            "Other account",
            "personal",
            300_000.0,
            None,
        ),
        token_source(
            "w",
            Some("tw"),
            "p",
            "Refactor usage page",
            "work",
            184_000.0,
            None,
        ),
    ];
    let s = live_snapshot::make(&st, &none(), now(), &ROME, &sources, &test_accounts()).unwrap();
    let u = s.urgency.clone().unwrap();
    assert_eq!(u.lead, "full at ");
    assert_eq!(u.full_at, "15:40");
    assert_eq!(u.detail, "work in 14 min");
    assert_eq!(u.source, "184k/min from “Refactor usage page”");
    assert_eq!(s.peak_thread(), Some("tw"));
}

/// A spike alone is not red: on 16/11 the personal account was red at 36% used, 111k/min over a 59k/min p99.
#[test]
fn a_spike_alone_is_not_urgent() {
    let minutes = Minutes::from([(
        "claudeAgent".to_owned(),
        HashMap::from([(minute_now() - 1, (425_706 / 15 * 3) as f64)]),
    )]);
    let s = make(&status(|_| {}), &minutes).unwrap();
    assert_eq!(s.active_id, "claudeAgent");
    assert!(!s.is_urgent());
}

/// The week fills within 20 minutes: urgent, and the account carries "full" for its week pill.
#[test]
fn urgent_when_the_week_fills_soon() {
    let st = status(|s| {
        entry(s, "claudeAgent_work").projection = None;
        entry(s, "claudeAgent").week_projection = projection("1 h", "15:40");
    });
    let s = make(&st, &none()).unwrap();
    assert_eq!(s.accounts[1].week_full_at.as_deref(), Some("15:40"));
    assert_eq!(s.accounts[1].full_at, None);
    let u = s.urgency.clone().unwrap();
    assert_eq!(s.active_id, "claudeAgent");
    assert_eq!(u.lead, "full at ");
    assert_eq!(u.full_at, "15:40");
    assert_eq!(u.detail, "personal week in 14 min");
    // later: the week pill is red, the island is not urgent
    let later = status(|s| {
        entry(s, "claudeAgent_work").projection = None;
        entry(s, "claudeAgent").week_projection = projection("1 h", "18:00");
    });
    let l = make(&later, &none()).unwrap();
    assert_eq!(l.accounts[1].week_full_at.as_deref(), Some("18:00"));
    assert_eq!(l.urgency, None);
}

#[test]
fn urgent_when_the_limit_is_hit() {
    let st = status(|s| entry(s, "claudeAgent_work").limit_status = Some("rejected".to_owned()));
    assert!(make(&st, &none()).unwrap().urgency.is_some());
    let warn =
        status(|s| entry(s, "claudeAgent_work").limit_status = Some("allowed_warning".to_owned()));
    assert_eq!(make(&warn, &none()).unwrap().urgency, None);
}

#[test]
fn stale_and_idle_accounts_are_never_urgent() {
    let stale = status(|s| {
        let e = entry(s, "claudeAgent");
        e.five_hour_pct = Some(97);
        e.note = Some("5h window already reset, no new data".to_owned());
    });
    let s = make(&stale, &none()).unwrap();
    assert_eq!(s.urgency, None);
    assert_eq!(s.accounts[1].five_hour_percent, 0);
    assert_eq!(s.accounts[1].five_hour_reset, "—");
    let idle = status(|s| {
        let e = entry(s, "claudeAgent");
        e.five_hour_pct = Some(97);
        e.live = Some(false);
    });
    assert_eq!(make(&idle, &none()).unwrap().urgency, None);
}

#[test]
fn no_five_hour_data_means_no_snapshot() {
    let st = status(|s| s.accounts.retain(|k, _| k == "codex"));
    assert_eq!(make(&st, &none()), None);
}

#[test]
fn full_at_comes_from_the_projection() {
    let st = status(|s| {
        entry(s, "claudeAgent").five_hour_pct = Some(103);
        entry(s, "claudeAgent_work").projection = None;
    });
    let s = make(&st, &none()).unwrap();
    // no projection: not expected to fill up; at 100% and over: already full
    assert_eq!(
        s.accounts
            .iter()
            .map(|a| a.full_at.as_deref())
            .collect::<Vec<_>>(),
        [None, Some("now")]
    );
    let with_projection = make(&status(|_| {}), &none()).unwrap();
    assert_eq!(
        with_projection.accounts[0].full_at.as_deref(),
        Some("18:04")
    );
}

/// A projection after the reset says nothing: the row leaves "full at" empty.
#[test]
fn full_at_only_before_the_reset() {
    let late = status(|s| entry(s, "claudeAgent_work").projection = projection("30 min", "19:30"));
    assert_eq!(make(&late, &none()).unwrap().accounts[0].full_at, None);
    let at_reset =
        status(|s| entry(s, "claudeAgent_work").projection = projection("30 min", "19:00"));
    assert_eq!(make(&at_reset, &none()).unwrap().accounts[0].full_at, None);
}

/// The island follows the account the chart's minutes (the transcripts) show burning most, not the database's
/// finished turns: on 30/10 13:44 the database had work ahead while team burned 5 times more.
#[test]
fn active_account_is_the_one_the_minutes_show_burning() {
    let st = status(|s| {
        if let Some(t) = entry(s, "claudeAgent").tokens_fresh.as_mut() {
            t.insert("15m".to_owned(), 1);
        }
    });
    let m = minute_now();
    let minutes = Minutes::from([
        (
            "claudeAgent".to_owned(),
            HashMap::from([(m - 3, 500_000.0)]),
        ),
        (
            "claudeAgent_work".to_owned(),
            HashMap::from([(m - 3, 100_000.0)]),
        ),
    ]);
    assert_eq!(make(&st, &minutes).unwrap().active_id, "claudeAgent");
}

/// The island counts what is left, like the popover: 18% used is 82%, a blocked account is 0%.
#[test]
fn island_counts_what_is_left() {
    let mut a = Account::new("x", "x", 18, "20:20", true);
    assert_eq!(a.five_hour_left(), 82);
    a.is_blocked = true;
    assert_eq!(a.five_hour_left(), 0);
    assert_eq!(Account::new("y", "y", 103, "-", true).five_hour_left(), 0);
}

/// An ETA already gone by says nothing: no "full 15:20" at 15:25, no "in 0 min" peak.
#[test]
fn eta_in_the_past_is_dropped() {
    let st = status(|s| entry(s, "claudeAgent_work").projection = projection("5 min", "15:20"));
    let s = make(&st, &none()).unwrap();
    assert_eq!(s.accounts[0].full_at, None);
    assert_eq!(s.urgency, None);
}

/// A live blocked account in urgency: "blocked until" the reset; a thread under 1 token/min is not named.
#[test]
fn blocked_peak_says_blocked_until() {
    let st = status(|s| {
        let e = entry(s, "claudeAgent");
        e.five_hour_pct = Some(103);
        e.limit_status = Some("rejected".to_owned());
    });
    let slow = [token_source(
        "o",
        None,
        "p",
        "Old thread",
        "personal",
        0.2,
        None,
    )];
    let u = live_snapshot::make(&st, &none(), now(), &ROME, &slow, &test_accounts())
        .unwrap()
        .urgency
        .unwrap();
    assert_eq!(u.lead, "free at ");
    assert_eq!(u.full_at, "20:20");
    assert_eq!(u.detail, "personal is cooked");
    assert_eq!(u.source, "Idle for 5 min");
}

/// `limit_status` "rejected": "blocked until" the 5h reset, no "full at".
#[test]
fn rejected_account_is_blocked_until_the_reset() {
    let st = status(|s| {
        let e = entry(s, "claudeAgent");
        e.five_hour_pct = Some(103);
        e.limit_status = Some("rejected".to_owned());
        e.live = Some(false);
        e.last_event_at = Some("13:42".to_owned());
    });
    let s = make(&st, &none()).unwrap();
    let personal = &s.accounts[1];
    assert!(personal.is_blocked);
    assert_eq!(personal.five_hour_reset, "20:20");
    assert_eq!(personal.full_at, None);
    assert!(!personal.is_unused);
    assert_eq!(personal.week_percent, Some(81));
    assert!(!s.accounts[0].is_blocked);
    // a stale window is not blocked any more
    let stale = status(|s| {
        let e = entry(s, "claudeAgent");
        e.limit_status = Some("rejected".to_owned());
        e.note = Some("5h window already reset, no new data".to_owned());
    });
    assert!(!make(&stale, &none()).unwrap().accounts[1].is_blocked);
}

fn shorts(v: Vec<&Account>) -> Vec<String> {
    v.iter().map(|a| a.short.clone()).collect()
}

/// No event in the last 5 hours: the account is hidden from the popover.
#[test]
fn unused_accounts_fold() {
    let st = status(|s| {
        let c = entry(s, "codex");
        c.five_hour_pct = Some(35);
        c.five_hour_resets_at = Some("24/10 03:55".to_owned());
        c.seven_day_pct = Some(6);
        c.seven_day_resets_at = Some("30/10 21:55".to_owned());
        c.last_event_at = Some("24/10 00:52".to_owned());
        c.note = Some("5h window already reset, no new data".to_owned());
        entry(s, "claudeAgent").last_event_at = Some("10:26".to_owned()); // 4 h 59 min ago: still used today
    });
    let s = make(&st, &none()).unwrap();
    assert_eq!(shorts(s.rows()), ["work", "personal"]);
    assert_eq!(shorts(s.unused()), ["codex"]);
    assert_eq!(s.unused()[0].week_percent, Some(6));

    let old = status(|s| entry(s, "claudeAgent").last_event_at = Some("10:20".to_owned()));
    assert_eq!(shorts(make(&old, &none()).unwrap().unused()), ["personal"]);
    // without a last event: not live and no tokens in the last 15 minutes
    let quiet = status(|s| {
        let e = entry(s, "claudeAgent");
        e.last_event_at = None;
        e.live = Some(false);
        e.tokens_fresh = Some([("15m".to_owned(), 0)].into());
    });
    assert_eq!(
        shorts(make(&quiet, &none()).unwrap().unused()),
        ["personal"]
    );
    let busy = status(|s| {
        let e = entry(s, "claudeAgent");
        e.last_event_at = None;
        e.live = Some(false);
    });
    assert!(make(&busy, &none()).unwrap().unused().is_empty());
}

/// The account the island shows always keeps its row.
#[test]
fn active_account_never_folds() {
    let st = status(|s| {
        for id in ["claudeAgent", "claudeAgent_work"] {
            entry(s, id).last_event_at = Some("01/10 10:00".to_owned());
        }
    });
    let s = make(&st, &none()).unwrap();
    assert_eq!(
        s.rows().iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
        [s.active_id.as_str()]
    );
}

/// A week that reset with no new data shows no week %.
#[test]
fn stale_week_has_no_percent() {
    let st =
        status(|s| entry(s, "claudeAgent").seven_day_resets_at = Some("16/10 16:00".to_owned()));
    assert_eq!(make(&st, &none()).unwrap().accounts[1].week_percent, None);
}

#[test]
fn short_names() {
    let accounts = test_accounts();
    assert_eq!(live_snapshot::short("claudeAgent_work", &accounts), "work");
    assert_eq!(live_snapshot::short("claudeAgent_team", &accounts), "team");
    assert_eq!(
        live_snapshot::short("claudeAgent_new", &accounts),
        "claudeAgent_new"
    ); // not in the store
}

/// On the day the week resets the pill shows a time, not a day,
/// and the week % stays whole: it is still the week's percent until the reset, and gone after.
#[test]
fn week_percent_whole_on_reset_day() {
    // personal's week resets today at 18:00
    let st = status(|s| {
        let e = entry(s, "claudeAgent");
        e.seven_day_pct = Some(98);
        e.seven_day_resets_at = Some("29/10 18:00".to_owned());
    });
    let s = make(&st, &none()).unwrap();
    let personal = &s.accounts[1];
    assert_eq!(personal.week_percent, Some(98));
    assert_eq!(Account::left(personal.week_percent.unwrap_or(0)), 2);
    assert_eq!(personal.week_reset.as_deref(), Some("18:00"));
    assert!(personal.week_elapsed.is_some_and(|e| e > 0.98 && e < 1.0));
    // a later day shows the day
    let tomorrow =
        status(|s| entry(s, "claudeAgent").seven_day_resets_at = Some("30/10 18:00".to_owned()));
    assert_eq!(
        make(&tomorrow, &none()).unwrap().accounts[1]
            .week_reset
            .as_deref(),
        Some("Fri")
    );
    // right after the reset, with no new data: no week % and no reset text
    let gone =
        status(|s| entry(s, "claudeAgent").seven_day_resets_at = Some("29/10 15:20".to_owned()));
    let g = make(&gone, &none()).unwrap();
    assert_eq!(g.accounts[1].week_percent, None);
    assert_eq!(g.accounts[1].week_reset, None);
    assert_eq!(g.accounts[1].week_elapsed, None);
}

/// Red means a window runs out before its reset. A spike, 99% used or a week
/// projection at or after the week's reset are not red.
#[test]
fn red_only_when_window_runs_out_before_reset() {
    let m = minute_now();
    let spike = Minutes::from([(
        "claudeAgent".to_owned(),
        HashMap::from([(m, 900_000.0), (m - 1, 900_000.0)]),
    )]);
    let full = status(|s| {
        entry(s, "claudeAgent_work").projection = None;
        entry(s, "claudeAgent").five_hour_pct = Some(99);
    });
    assert!(!make(&full, &spike).unwrap().is_urgent());
    // the week fills in 15 min, but its reset comes first
    let after_reset = status(|s| {
        entry(s, "claudeAgent_work").projection = None;
        let e = entry(s, "claudeAgent");
        e.seven_day_resets_at = Some("29/10 15:35".to_owned());
        e.week_projection = projection("1 h", "15:40");
    });
    let s = make(&after_reset, &none()).unwrap();
    assert_eq!(s.accounts[1].week_full_at, None);
    assert!(!s.is_urgent());
    // the 5 h window fills in 15 min, before its reset: red
    let soon = status(|s| entry(s, "claudeAgent_work").projection = projection("5 min", "15:40"));
    assert!(make(&soon, &none()).unwrap().is_urgent());
}

mod sqlite {
    use std::ffi::{CString, c_char, c_int, c_void};

    #[repr(C)]
    pub struct Sqlite3 {
        _p: [u8; 0],
    }

    #[link(name = "sqlite3")]
    unsafe extern "C" {
        fn sqlite3_open(name: *const c_char, db: *mut *mut Sqlite3) -> c_int;
        fn sqlite3_exec(
            db: *mut Sqlite3,
            sql: *const c_char,
            cb: *const c_void,
            arg: *mut c_void,
            err: *mut *mut c_char,
        ) -> c_int;
        fn sqlite3_close(db: *mut Sqlite3) -> c_int;
    }

    /// Runs `sql` on a new database at `path`; true when both worked.
    pub fn create(path: &str, sql: &str) -> bool {
        let (Ok(name), Ok(sql)) = (CString::new(path), CString::new(sql)) else {
            return false;
        };
        let mut db = std::ptr::null_mut();
        // SAFETY: valid C strings for the calls; the handle is closed below.
        unsafe {
            let open = sqlite3_open(name.as_ptr(), &mut db) == 0;
            let exec = open
                && sqlite3_exec(
                    db,
                    sql.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                ) == 0;
            sqlite3_close(db);
            exec
        }
    }
}

/// A database with the two tables the query reads, and the thread titles.
fn make_database(turns: &[(&str, &str, &str, i64, i64, i64)]) -> String {
    let path = std::env::temp_dir()
        .join(format!("tokenlog-{}.sqlite", uuid()))
        .to_string_lossy()
        .into_owned();
    let mut sql = "CREATE TABLE orchestration_v2_projection_provider_turns (provider_turn_id TEXT PRIMARY KEY, thread_id TEXT,
          provider_thread_id TEXT, started_at TEXT, completed_at TEXT, payload_json TEXT);
        CREATE TABLE orchestration_v2_projection_provider_threads (provider_thread_id TEXT PRIMARY KEY, provider TEXT,
          provider_instance_id TEXT, thread_id TEXT, payload_json TEXT);
        CREATE TABLE orchestration_v2_projection_threads (thread_id TEXT PRIMARY KEY, title TEXT, project_id TEXT);
        CREATE TABLE projection_projects (project_id TEXT PRIMARY KEY, title TEXT);
        INSERT INTO projection_projects VALUES ('p-brain', 'garden-notes'), ('p-none', 'No project');"
        .to_owned();
    for (i, (instance, thread, completed, input, cached, output)) in turns.iter().enumerate() {
        let project = if *thread == "c" { "p-none" } else { "p-brain" };
        sql += &format!(
            r#"
            INSERT OR IGNORE INTO orchestration_v2_projection_provider_threads VALUES ('pt-{thread}', 'claudeAgent', '{instance}',
              '{thread}', '{{"nativeThreadRef":{{"nativeId":"session-{thread}"}}}}');
            INSERT OR IGNORE INTO orchestration_v2_projection_threads VALUES ('{thread}', 'Thread {thread}', '{project}');
            INSERT INTO orchestration_v2_projection_provider_turns VALUES ('turn-{i}', '{thread}', 'pt-{thread}',
              NULL, '{completed}',
              '{{"turnTokenUsage":{{"inputTokens":{input},"cachedInputTokens":{cached},"outputTokens":{output}}}}}');"#
        );
    }
    assert!(sqlite::create(&path, &sql));
    path
}

#[test]
fn session_threads() {
    let path = make_database(&[("claudeAgent", "c", "2026-10-29T14:22:00.000Z", 300, 0, 0)]);
    let threads = TokenLog::new(path)
        .threads(&[
            "session-c".to_owned(),
            "session-x".to_owned(),
            "it's".to_owned(),
        ])
        .unwrap();
    assert_eq!(
        threads,
        HashMap::from([(
            "session-c".to_owned(),
            Thread {
                id: "c".to_owned(),
                title: "Thread c".to_owned(),
                project: None
            }
        )])
    );
}

#[test]
fn missing_database_throws() {
    assert!(
        TokenLog::new("/nonexistent/statev2.sqlite")
            .threads(&["s".to_owned()])
            .is_err()
    );
}

fn transcript_now() -> f64 {
    iso_date("2026-10-29T14:25:51.000Z").unwrap()
}

fn line(id: &str, at: &str, input: i64, write: i64, read: i64, output: i64) -> String {
    format!(
        r#"{{"type":"assistant","timestamp":"{at}","message":{{"id":"{id}","usage":{{"input_tokens":{input},"cache_creation_input_tokens":{write},"cache_read_input_tokens":{read},"output_tokens":{output}}}}}}}"#
    )
}

fn make_home() -> (String, PathBuf) {
    let home = temp_dir("home").to_string_lossy().into_owned();
    let dir =
        PathBuf::from(TranscriptLog::dir("claudeAgent_work", &home) + "/projects/p/s/subagents");
    std::fs::create_dir_all(&dir).unwrap();
    (home, dir.join("agent-1.jsonl"))
}

#[test]
fn config_dirs() {
    assert_eq!(TranscriptLog::dir("claudeAgent", "/h"), "/h/.claude");
    assert_eq!(
        TranscriptLog::dir("claudeAgent_team", "/h"),
        "/h/.claude-team"
    );
    assert_eq!(
        TranscriptLog::dir("claudeAgent_side", "/h"),
        "/h/.claude-side"
    );
}

#[test]
fn counts_new_tokens_per_minute_once_and_reads_only_new_lines() {
    let (home, file) = make_home();
    let minute = transcript_now() as i64 / 60;
    let text = [
        line("m1", "2026-10-29T14:24:10.000Z", 2, 1000, 90000, 100), // 1102
        line("m1", "2026-10-29T14:24:11.000Z", 2, 1000, 90000, 100), // same message, second block
        r#"{"type":"user","timestamp":"2026-10-29T14:24:12.000Z","message":{"content":"tool output"}}"#.to_owned(),
        line("m2", "2026-10-29T14:25:05.000Z", 5, 0, 91000, 400), // 405
        line("old", "2026-10-29T13:10:00.000Z", 9999, 0, 0, 0),   // over an hour ago
    ]
    .join("\n")
        + "\n";
    std::fs::write(&file, text).unwrap();
    let mut log = TranscriptLog::new(
        HashMap::from([
            (
                "claudeAgent_work".to_owned(),
                TranscriptLog::dir("claudeAgent_work", &home),
            ),
            (
                "claudeAgent".to_owned(),
                TranscriptLog::dir("claudeAgent", &home),
            ),
        ]),
        None,
    );
    let mut read = || {
        log.read(transcript_now(), TranscriptLog::DEFAULT_MINUTES)
            .minutes
    };
    let first = read();
    assert_eq!(
        first.get("claudeAgent_work"),
        Some(&HashMap::from([(minute - 1, 1102.0), (minute, 405.0)]))
    );
    assert_eq!(first.get("claudeAgent"), None); // no directory: the database stays the source
    // a half-written line waits for its newline
    let more = line("m3", "2026-10-29T14:25:30.000Z", 0, 0, 0, 50);
    append_raw(&file, &more[..40]);
    assert_eq!(
        read().get("claudeAgent_work"),
        Some(&HashMap::from([(minute - 1, 1102.0), (minute, 405.0)]))
    );
    append_raw(&file, &(more[40..].to_owned() + "\n"));
    assert_eq!(
        read().get("claudeAgent_work"),
        Some(&HashMap::from([(minute - 1, 1102.0), (minute, 455.0)]))
    );
}

#[test]
fn first_read_starts_near_the_window() {
    let (_, file) = make_home();
    // 300 lines, one per minute from 10:00; each about 200 bytes
    let start = iso_date("2026-10-29T10:00:00.000Z").unwrap();
    let lines: Vec<String> = (0..300)
        .map(|i| {
            line(
                &format!("m{i}"),
                &iso_string(start + i as f64 * 60.0, true),
                1,
                0,
                0,
                0,
            )
        })
        .collect();
    std::fs::write(&file, lines.join("\n") + "\n").unwrap();
    let since = start + 240.0 * 60.0;
    let offset = TranscriptLog::start(&file, since, 4096);
    let text = std::fs::read_to_string(&file).unwrap();
    let tail = &text[offset as usize..];
    assert!(offset > 0);
    assert!(tail.starts_with(r#"{"type""#)); // a line start
    let first_line = tail.split('\n').next().unwrap();
    let first_minute: i64 = first_line.split('"').nth(13).unwrap()[1..].parse().unwrap(); // "m<n>"
    assert!(first_minute <= 240 && first_minute > 200); // before the window, but not far before
    assert_eq!(TranscriptLog::start(&file, start - 60.0, 4096), 0);
}

/// A session's own file and its subagents' files make one session; the cwd comes from the session's own calls.
#[test]
fn groups_subagents_under_their_session() {
    let (home, sub) = make_home(); // projects/p/s/subagents/agent-1.jsonl
    let main = sub
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("s.jsonl");
    let call = |id: &str, at: &str, tokens: i64, cwd: &str| {
        format!(
            r#"{{"type":"assistant","timestamp":"{at}","cwd":"{cwd}","message":{{"id":"{id}","usage":{{"input_tokens":{tokens},"output_tokens":0}}}}}}"#
        ) + "\n"
    };
    std::fs::write(
        &main,
        call("m1", "2026-10-29T14:24:00.000Z", 3000, "/r/cooked"),
    )
    .unwrap();
    std::fs::write(
        &sub,
        call("s1", "2026-10-29T14:25:00.000Z", 1500, "/tmp/wt"),
    )
    .unwrap();
    let other = sub.parent().unwrap().join("agent-2.jsonl");
    std::fs::write(
        &other,
        call("s2", "2026-10-29T14:15:00.000Z", 1500, "/tmp/wt"),
    )
    .unwrap();
    let r = TranscriptLog::new(
        HashMap::from([(
            "claudeAgent_work".to_owned(),
            TranscriptLog::dir("claudeAgent_work", &home),
        )]),
        None,
    )
    .read(transcript_now(), TranscriptLog::DEFAULT_MINUTES);
    let mut expected = Session::new(
        "claudeAgent_work",
        "s",
        Some("/r/cooked".to_owned()),
        6000.0,
        true,
        1,
    );
    expected.last_minute = Some(iso_date("2026-10-29T14:25:00.000Z").unwrap() as i64 / 60);
    expected.last_five = 4500.0; // the call at 14:15 is out of the last 5 minutes
    assert_eq!(r.sessions, [expected]);
}

#[test]
fn reads_the_limits_from_the_provider_logs() {
    let now = common::now();
    let dir = log_dir(&[claude_line(
        now - 60.0,
        Some("claudeAgent_work"),
        0.48,
        now + 3600.0,
    )]);
    let reading = source(dir).read(now, &FixedOffset(0));
    assert_eq!(shorts(reading.snapshot.accounts.iter().collect()), ["work"]);
    assert_eq!(reading.snapshot.accounts[0].five_hour_percent, 48);
    assert_eq!(reading.snapshot.series, vec![0.0; 1440]); // no database: empty histogram
}

/// No percent and no tokens: an empty snapshot, never the demo, and the notice says why.
#[test]
fn with_no_data_the_notice_says_why() {
    let reading = source(log_dir(&[])).read(common::now(), &FixedOffset(0));
    assert!(reading.snapshot.accounts.is_empty());
    let first = &test_accounts()[0];
    assert_eq!(
        reading.notices.first(),
        Some(&Notice::NoPercent {
            id: first.id.clone(),
            account: first.name.clone()
        })
    );
}

#[test]
fn reports_no_accounts() {
    let mut s = source(log_dir(&[]));
    s.accounts = Vec::new();
    assert_eq!(
        s.read(common::now(), &FixedOffset(0)).notices,
        [Notice::NoAccounts]
    );
}

#[test]
fn shows_the_values_as_they_are() {
    let mut h = LiveHistogram::new(vec![32000.0; 60], 40000.0, 100, 6000.0);
    let history = h.history(10.0, false);
    assert!(history.values.iter().all(|v| *v == 32000.0));
    assert!(history.top == 64000.0 && history.threshold == 40000.0);
}

#[test]
fn shifts_when_the_wall_clock_minute_closes() {
    let values: Vec<f64> = (0..60).map(|i| 1000.0 * (i as f64 + 1.0)).collect();
    let mut h = LiveHistogram::new(values, 40000.0, 100, 6000.0);
    assert_eq!(h.history(59.0, false).values[10], 11000.0);
    let after = h.history(60.0, false);
    assert_eq!(after.values[10], 12000.0);
    assert_eq!(after.values[59], 0.0);
}

#[test]
fn new_data_replaces_the_history() {
    let mut h = LiveHistogram::new(vec![16000.0; 60], 40000.0, 100, 6000.0);
    h.history(10.0, false);
    h.update(vec![32000.0; 60], 30000.0, 100);
    let history = h.history(10.0, false);
    assert!(history.values.iter().all(|v| *v == 32000.0));
    assert_eq!(history.threshold, 30000.0);
}

#[test]
fn catches_up_after_several_minutes_unseen() {
    let mut h = LiveHistogram::new(vec![32000.0; 60], 40000.0, 100, 6000.0);
    let after = h.history(3.0 * 60.0 + 5.0, false);
    assert!(after.values[57..].iter().all(|v| *v == 0.0));
    assert!(after.values[..57].iter().all(|v| *v == 32000.0));
}

/// A session as busy in its last 5 minutes as in its last 15: the same rate on both windows.
fn steady(mut s: Session) -> Session {
    s.last_five = s.tokens / 3.0;
    s
}

fn session(
    account: &str,
    id: &str,
    cwd: Option<&str>,
    tokens: f64,
    main: bool,
    subs: i64,
) -> Session {
    Session::new(account, id, cwd.map(str::to_owned), tokens, main, subs)
}

fn session_at(
    account: &str,
    id: &str,
    cwd: Option<&str>,
    tokens: f64,
    main: bool,
    subs: i64,
    last_minute: i64,
) -> Session {
    let mut s = session(account, id, cwd, tokens, main, subs);
    s.last_minute = Some(last_minute);
    s
}

fn thread() -> Thread {
    Thread {
        id: "t1".to_owned(),
        title: "popover data".to_owned(),
        project: Some("cooked".to_owned()),
    }
}

#[test]
fn names_sessions_by_their_t3_thread_in_rate_order() {
    let sessions = [
        session(
            "claudeAgent_work",
            "s1",
            Some("/r/cooked"),
            1_695_000.0,
            true,
            3,
        ),
        session(
            "claudeAgent_team",
            "s2",
            Some("/Users/m/.t3/scratch/2026-x"),
            60_000.0,
            false,
            0,
        ),
        session("claudeAgent", "s3", None, 10.0, true, 0), // under 1/min
        session("claudeAgent", "s4", Some("/r/atlas"), 15_000.0, false, 2),
    ];
    // where a click goes: OpenTargetTests
    let steady_sessions: Vec<Session> = sessions.into_iter().map(steady).collect();
    let out: Vec<_> = token_sources::make(
        &steady_sessions,
        &HashMap::from([("s1".to_owned(), thread())]),
        &test_accounts(),
        None,
    )
    .into_iter()
    .map(|mut s| {
        s.session = None;
        s
    })
    .collect();
    assert_eq!(
        out,
        [
            token_source(
                "t1",
                Some("t1"),
                "cooked",
                "popover data",
                "work",
                113_000.0,
                Some("1 agent + 3 sub")
            ),
            token_source(
                "s2",
                None,
                "t3 scratch",
                "Claude Code session",
                "team",
                4000.0,
                None
            ),
            token_source(
                "s4",
                None,
                "atlas",
                "Claude Code session",
                "personal",
                1000.0,
                Some("2 sub")
            ),
        ]
    );
    assert!(token_sources::make(&[], &HashMap::new(), &test_accounts(), None).is_empty());
}

/// The card shows 3 and "N more"; the list behind it stops at 8 (2026-10-30-sources-expand).
#[test]
fn keeps_up_to_eight_sources() {
    let sessions: Vec<Session> = (1..=10)
        .map(|i| {
            steady(session(
                "claudeAgent",
                &format!("s{i}"),
                None,
                i as f64 * 1500.0,
                true,
                0,
            ))
        })
        .collect();
    let out = token_sources::make(&sessions, &HashMap::new(), &test_accounts(), None);
    assert_eq!(
        out.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        ["s10", "s9", "s8", "s7", "s6", "s5", "s4", "s3"]
    );
}

#[test]
fn merges_sessions_of_one_thread() {
    let sessions = [
        steady(session("claudeAgent_work", "a", None, 15_000.0, false, 0)),
        steady(session("claudeAgent_work", "b", None, 30_000.0, true, 0)),
    ];
    let threads = HashMap::from([("a".to_owned(), thread()), ("b".to_owned(), thread())]);
    let out = token_sources::make(&sessions, &threads, &test_accounts(), None);
    assert_eq!(out.iter().map(|s| s.rate).collect::<Vec<_>>(), [3000.0]);
    assert_eq!(
        out.first().and_then(|s| s.agents.as_deref()),
        Some("1 agent")
    );
}

/// A live line's rate is its last 5 minutes on the popover's "now" divisor, so the lines add up to "now"; an ended
/// line keeps its last 15.
#[test]
fn live_lines_use_the_last_five_minutes() {
    let now = 1_000.0 * 60.0 + 30.0; // half of this minute gone: 4.5 minutes
    let mut live = session_at("claudeAgent", "live", None, 90_000.0, true, 0, 1_000);
    live.last_five = 45_000.0;
    let ended = session_at("claudeAgent", "ended", None, 15_000.0, false, 0, 993);
    let out = token_sources::make(&[live, ended], &HashMap::new(), &test_accounts(), Some(now));
    assert_eq!(
        out.iter().map(|s| s.rate).collect::<Vec<_>>(),
        [10_000.0, 1000.0]
    );
}

/// A thread with no call in the last 5 minutes goes under "Ended", newest first, after the live ones.
#[test]
fn ended_threads_come_last_with_their_minutes() {
    let now = 1_000.0 * 60.0;
    let sessions = [
        session_at("claudeAgent", "big", Some("/r/a"), 90_000.0, false, 0, 991),
        session_at(
            "claudeAgent",
            "live",
            Some("/r/b"),
            15_000.0,
            true,
            0,
            1_000,
        ),
        session_at(
            "claudeAgent",
            "recent",
            Some("/r/c"),
            30_000.0,
            false,
            0,
            994,
        ),
        session_at("claudeAgent", "subs", Some("/r/d"), 3_000.0, false, 1, 999),
    ];
    let steady_sessions: Vec<Session> = sessions.iter().cloned().map(steady).collect();
    let out = token_sources::make(
        &steady_sessions,
        &HashMap::new(),
        &test_accounts(),
        Some(now),
    );
    assert_eq!(
        out.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        ["live", "subs", "recent", "big"]
    );
    assert_eq!(
        out.iter().map(|s| s.ended_ago).collect::<Vec<_>>(),
        [None, None, Some(6), Some(9)]
    );
    // two sessions of one thread: ended only when both are, at the newer one's minute
    let merged = token_sources::make(
        &[sessions[0].clone(), sessions[2].clone()],
        &HashMap::from([
            ("big".to_owned(), thread()),
            ("recent".to_owned(), thread()),
        ]),
        &test_accounts(),
        Some(now),
    );
    assert_eq!(
        merged.iter().map(|s| s.ended_ago).collect::<Vec<_>>(),
        [Some(6)]
    );
    let one = token_sources::make(
        &[sessions[0].clone(), sessions[1].clone()],
        &HashMap::from([("big".to_owned(), thread()), ("live".to_owned(), thread())]),
        &test_accounts(),
        Some(now),
    );
    assert_eq!(one.iter().map(|s| s.ended_ago).collect::<Vec<_>>(), [None]);
}

#[test]
fn agent_labels() {
    assert_eq!(token_sources::agents(true, 0).as_deref(), Some("1 agent"));
    assert_eq!(
        token_sources::agents(true, 3).as_deref(),
        Some("1 agent + 3 sub")
    );
    assert_eq!(token_sources::agents(false, 2).as_deref(), Some("2 sub"));
    assert_eq!(token_sources::agents(false, 0), None);
    assert_eq!(
        token_sources::project_of_cwd(Some("/Users/m/Desktop/GitHub/garden-notes")),
        "garden-notes"
    );
    assert_eq!(token_sources::project_of_cwd(None), "unknown");
}

#[test]
fn thread_link() {
    let file = temp_dir("t3").join("environment-id");
    std::fs::write(&file, "6ee1c3a6-fb6a\n").unwrap();
    assert_eq!(
        t3_code::link("0c24a363", &file).as_deref(),
        Some("t3code://threads/6ee1c3a6-fb6a/0c24a363")
    );
    assert_eq!(t3_code::link("x", Path::new("/nonexistent")), None);
    // a delegated thread: its ":" and "%3A" reach T3 Code as written
    assert_eq!(
        t3_code::link("thread:delegated-task:command%3Amcp%3A37e7", &file).as_deref(),
        Some("t3code://threads/6ee1c3a6-fb6a/thread%3Adelegated-task%3Acommand%253Amcp%253A37e7")
    );
}
