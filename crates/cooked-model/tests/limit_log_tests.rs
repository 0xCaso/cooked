//! The limits read from T3 Code's provider logs, in place of the watcher script.
mod common;

use common::{append_raw, claude_line, claude_line_full, codex_line, log_dir, set_modified, temp_dir};
use cooked_model::clock::{FixedOffset, days_from_civil};
use cooked_model::limit_log::{LimitInput, LimitLog, Sample};
use cooked_model::status_file::Projection;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Europe/Rome on 30/10/2026, after the switch back from summer time.
const ROME: FixedOffset = FixedOffset(3600);

/// 30/10/2026 14:00 in Rome.
fn now() -> f64 {
    (days_from_civil(2026, 10, 30) * 86_400 + 13 * 3600) as f64
}

fn sample_full(minutes_ago: f64, five: f64, reset: f64, week: f64, week_reset: f64) -> Sample {
    let t = now();
    Sample {
        at: t - minutes_ago * 60.0,
        five,
        five_reset: Some(t + reset),
        week,
        week_reset: Some(t + week_reset),
        status: "allowed".to_owned(),
    }
}

fn sample(minutes_ago: f64, five: f64) -> Sample {
    sample_full(minutes_ago, five, 3600.0, 0.5, 86400.0)
}

fn sample_reset(minutes_ago: f64, five: f64, reset: f64) -> Sample {
    sample_full(minutes_ago, five, reset, 0.5, 86400.0)
}

fn sample_week(minutes_ago: f64, five: f64, week: f64) -> Sample {
    sample_full(minutes_ago, five, 3600.0, week, 86400.0)
}

fn read(log: &mut LimitLog) -> cooked_model::status_file::StatusFile {
    log.read(now(), LimitInput::default(), &ROME)
}

fn with_samples(
    log: &mut LimitLog,
    samples: &[(&str, Sample)],
) -> cooked_model::status_file::StatusFile {
    let samples = samples
        .iter()
        .map(|(id, s)| ((*id).to_owned(), s.clone()))
        .collect();
    log.read(
        now(),
        LimitInput {
            samples,
            ..LimitInput::default()
        },
        &ROME,
    )
}

fn events(dir: &Path) -> PathBuf {
    dir.join("events.t1.log")
}

#[test]
fn parses_claude_and_codex_events() {
    let reset = now() + 3600.0;
    let (account, s) =
        LimitLog::parse(claude_line(now(), Some("claudeAgent_team"), 0.69, reset).as_bytes())
            .expect("claude event");
    assert_eq!(account.as_deref(), Some("claudeAgent_team"));
    assert_eq!(
        s,
        Sample {
            at: now(),
            five: 0.69,
            five_reset: Some(reset),
            week: 0.38,
            week_reset: Some(1_793_714_400.0),
            status: "allowed".to_owned(),
        }
    );
    let (codex, c) =
        LimitLog::parse(codex_line(now(), 42.0, reset, true).as_bytes()).expect("codex event");
    assert_eq!(codex.as_deref(), Some("codex"));
    assert_eq!(c.five, 0.42);
    assert_eq!(c.week, 0.12);
    assert_eq!(c.status, "rejected");
    // a tool result that only quotes the event name is not an event
    let quoted = r#"[2026-10-30T12:36:46.727Z] NTIVE: {"event":{"payload":{"type":"user","content":"\"rate_limit_event\""}}}"#;
    assert!(LimitLog::parse(quoted.as_bytes()).is_none());
    assert_eq!(LimitLog::instance("provider-session:x"), None);
}

#[test]
fn reads_only_new_complete_lines() {
    let reset = now() + 3600.0;
    let dir = log_dir(&[claude_line(
        now() - 120.0,
        Some("claudeAgent_work"),
        0.30,
        reset,
    )]);
    let mut log = LimitLog::new(Some(dir.clone()), None);
    let path = events(&dir);
    set_modified(&path, now());
    assert_eq!(
        read(&mut log).accounts["claudeAgent_work"].five_hour_pct,
        Some(30)
    );
    // a new event and half a line: the event counts, the half line waits
    let next = claude_line(now() - 60.0, Some("claudeAgent_work"), 0.35, reset);
    append_raw(&path, &format!("{next}\n{}", &next[..40]));
    set_modified(&path, now());
    let status = read(&mut log);
    let work = &status.accounts["claudeAgent_work"];
    assert_eq!(work.five_hour_pct, Some(35));
    assert_eq!(work.five_hour_resets_at.as_deref(), Some("15:00"));
    assert_eq!(work.last_event_at.as_deref(), Some("13:59"));
    assert_eq!(work.live, Some(true));
    assert_eq!(
        work.spike_threshold_15m,
        Some(LimitLog::CLAUDE_FALLBACK_15M)
    );
}

/// The first reading looks back a week; later readings only at files written in the last 6 hours.
#[test]
fn old_files_are_skipped() {
    let dir = log_dir(&[claude_line(
        now() - 60.0,
        Some("claudeAgent"),
        0.3,
        now() + 3600.0,
    )]);
    set_modified(&events(&dir), now() - 8.0 * 86400.0);
    assert!(
        read(&mut LimitLog::new(Some(dir.clone()), None))
            .accounts
            .is_empty()
    );
    set_modified(&events(&dir), now() - 7.0 * 3600.0);
    let mut log = LimitLog::new(Some(dir), None);
    assert_eq!(
        read(&mut log).accounts["claudeAgent"].five_hour_pct,
        Some(30)
    );
    // a second reading does not read the old file again, and keeps the account
    assert_eq!(
        read(&mut log).accounts["claudeAgent"].five_hour_pct,
        Some(30)
    );
}

/// An account with no new event keeps its last percent across launches: ~/.claude's 23% week, used 3 days ago.
#[test]
fn last_known_percent_survives_a_restart() {
    let store = temp_dir("limits-store").join("limits.json");
    let used = now() - 3.0 * 86400.0;
    let week_reset = now() + 4.0 * 86400.0;
    let dir = log_dir(&[claude_line_full(
        used,
        Some("claudeAgent"),
        0.4,
        used + 3600.0,
        0.23,
        week_reset,
        "allowed",
    )]);
    set_modified(&events(&dir), used);
    let first = read(&mut LimitLog::new(Some(dir), Some(store.clone())))
        .accounts
        .remove("claudeAgent")
        .expect("claudeAgent");
    assert!(first.seven_day_pct == Some(23) && first.live == Some(false));
    assert!(first.note.is_some()); // its 5 h window has reset since
    // relaunch with no log at all: the store keeps the account
    let empty = log_dir(&[]);
    let again = read(&mut LimitLog::new(Some(empty.clone()), Some(store.clone())))
        .accounts
        .remove("claudeAgent")
        .expect("claudeAgent");
    assert_eq!(again, first);
    // a newer sample replaces it
    let newer = Sample {
        at: now(),
        five: 0.1,
        five_reset: Some(now() + 3600.0),
        week: 0.25,
        week_reset: Some(week_reset),
        status: "allowed".to_owned(),
    };
    with_samples(
        &mut LimitLog::new(Some(empty), Some(store.clone())),
        &[("claudeAgent", newer)],
    );
    assert_eq!(
        read(&mut LimitLog::new(None, Some(store))).accounts["claudeAgent"].seven_day_pct,
        Some(25)
    );
}

/// An account in the terminal (bridge) and in T3 Code (logs): the newest sample wins, whatever its source.
#[test]
fn newest_sample_wins_across_bridge_and_logs() {
    let reset = now() + 3600.0;
    let dir = log_dir(&[claude_line(now() - 60.0, Some("claudeAgent"), 0.40, reset)]);
    set_modified(&events(&dir), now());
    let mut log = LimitLog::new(Some(dir), None);
    // the bridge sample is from 30 minutes ago, the T3 Code event from 1 minute ago
    let bridge = sample_reset(30.0, 0.20, 3600.0);
    let status = with_samples(&mut log, &[("claudeAgent", bridge)]);
    let entry = &status.accounts["claudeAgent"];
    assert_eq!(entry.five_hour_pct, Some(40));
    assert_eq!(entry.last_event_at.as_deref(), Some("13:59"));
    // a newer bridge sample wins again
    let newer = sample_reset(0.0, 0.45, 3600.0);
    assert_eq!(
        with_samples(&mut log, &[("claudeAgent", newer)]).accounts["claudeAgent"].five_hour_pct,
        Some(45)
    );
}

/// Only the current 5 h window and the last hour.
#[test]
fn prune_keeps_the_current_window() {
    let old = sample_reset(30.0, 0.9, -60.0);
    let s = LimitLog::prune(&[
        sample(5.0, 0.5),
        old,
        sample(90.0, 0.1),
        sample(5.0, 0.5),
        sample(1.0, 0.6),
    ]);
    assert_eq!(s, vec![sample(5.0, 0.5), sample(1.0, 0.6)]);
}

/// 8 minutes of samples are no "30 min" pace: on 30/10 the script saw work full at 13:57 from them.
#[test]
fn pace_needs_most_of_its_window() {
    let five = |s: &Sample| s.five;
    let eight = [sample(9.0, 0.30), sample(5.0, 0.33), sample(1.0, 0.36)];
    assert_eq!(LimitLog::slope(&eight, 1800.0, 2, five), None);
    assert_eq!(LimitLog::slope(&eight, 300.0, 3, five), None); // only 2 samples in the last 5 min
    let long = [sample(25.0, 0.20), sample(1.0, 0.36)];
    let pace = LimitLog::slope(&long, 1800.0, 2, five).expect("a pace");
    assert!((pace - 0.16 / (24.0 * 60.0)).abs() < 1e-9);
}

/// The week fills at the last hour's pace before its reset and within 5 h: red on the week pill (2026-11-16-red-account-rule).
#[test]
fn week_projection_within_five_hours_only() {
    let entry = |samples: &[Sample]| LimitLog::entry("claudeAgent", samples, None, now(), &ROME);
    // 0.04 in 50 min: the last 0.04 in 50 min, at 13:59 + 50, before the week reset in 24 h
    let soon = entry(&[sample_week(51.0, 0.3, 0.92), sample_week(1.0, 0.4, 0.96)]);
    assert_eq!(
        soon.week_projection,
        Some(Projection {
            window: "1 h".to_owned(),
            eta: "14:49".to_owned()
        })
    );
    // 0.01 in 50 min with 0.22 left: 18 h away, more than 5 h
    let slow = entry(&[sample_week(51.0, 0.3, 0.77), sample_week(1.0, 0.4, 0.78)]);
    assert_eq!(slow.week_projection, None);
    // after the week reset
    let late = entry(&[
        sample_full(51.0, 0.3, 3600.0, 0.92, 1800.0),
        sample_full(1.0, 0.4, 3600.0, 0.96, 1800.0),
    ]);
    assert_eq!(late.week_projection, None);
    // 20 minutes of samples are no 1 h pace
    let short = entry(&[sample_week(21.0, 0.3, 0.92), sample_week(1.0, 0.4, 0.96)]);
    assert_eq!(short.week_projection, None);
}

#[test]
fn projection_before_the_reset_only() {
    let entry =
        |samples: &[Sample]| LimitLog::entry("claudeAgent_work", samples, None, now(), &ROME);
    // 0.16 in 24 min: 100% in 105 min, after the reset in 60 min
    let late = entry(&[sample(25.0, 0.20), sample(1.0, 0.36)]);
    assert_eq!(late.projection, None);
    let soon = entry(&[sample(25.0, 0.20), sample(1.0, 0.80)]);
    // 0.6 in 24 min: the last 0.2 in 8 min, at 13:59 + 8
    assert_eq!(
        soon.projection,
        Some(Projection {
            window: "30 min".to_owned(),
            eta: "14:07".to_owned()
        })
    );
    let low = entry(&[sample(25.0, 0.01), sample(1.0, 0.19)]);
    assert_eq!(low.projection, None); // under 20%
    let stale = entry(&[sample_reset(1.0, 0.5, -30.0)]);
    assert!(stale.note.is_some());
    let idle = entry(&[sample(25.0, 0.2), sample(11.0, 0.8)]);
    assert_eq!(idle.live, Some(false));
    assert_eq!(idle.projection, None);
}

/// team had 10 buckets on 30/10 and sat at the fixed 600k: it takes the other Claude accounts' p99.
#[test]
fn threshold_with_little_history() {
    let work: Vec<f64> = (1..=100).map(f64::from).collect(); // p99 = 100
    let buckets = HashMap::from([
        ("claudeAgent_work".to_owned(), work),
        ("claudeAgent_team".to_owned(), vec![5.0; 10]),
        ("codex".to_owned(), vec![1.0, 2.0]),
    ]);
    let t = LimitLog::thresholds(&buckets, &HashSet::from(["claudeAgent".to_owned()]));
    assert_eq!(t.get("claudeAgent_work"), Some(&100));
    assert_eq!(t.get("claudeAgent_team"), Some(&100));
    assert_eq!(t.get("claudeAgent"), Some(&100));
    assert_eq!(t.get("codex"), Some(&350_000));
}

/// The popover's chart sums every account: its line is the p99 of the summed buckets.
#[test]
fn total_threshold_on_the_sum() {
    let sum: Vec<f64> = (1..=200).map(|n| f64::from(n * 10)).collect();
    assert_eq!(LimitLog::total_threshold(&sum, 7), 1990);
    assert_eq!(LimitLog::total_threshold(&[1.0, 2.0, 3.0], 7), 7);
}

#[test]
fn clock_on_other_days() {
    // 31/10/2026 09:05 in Rome
    let tomorrow = (days_from_civil(2026, 10, 31) * 86_400 + 8 * 3600 + 5 * 60) as f64;
    assert_eq!(LimitLog::clock(tomorrow, now(), &ROME), "31/10 09:05");
    assert_eq!(LimitLog::clock(now(), now(), &ROME), "14:00");
}
