mod common;

use common::{fixture, lines, now, set_modified, temp_dir, uuid};
use cooked_model::clock::{FixedOffset, iso_date, iso_string};
use cooked_model::codex_log::{CodexLog, Count, Window};
use cooked_model::limit_log::{LimitInput, LimitLog};
use cooked_model::minute_ledger::MinuteLedger;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

fn iso(s: &str) -> f64 {
    iso_date(s).expect("iso date")
}

/// A Codex home with the sample rollouts in `sessions/`.
fn make_home(samples: &[&str]) -> PathBuf {
    let home = temp_dir("codex");
    let day = home.join("sessions/2026/11/01");
    std::fs::create_dir_all(&day).expect("day");
    for sample in samples {
        let id = uuid();
        std::fs::copy(
            fixture(sample),
            day.join(format!("rollout-2026-11-01T10-00-00-{id}.jsonl")),
        )
        .expect("copy");
    }
    home
}

fn counts(sample: &str) -> Vec<Count> {
    lines(sample)
        .iter()
        .filter_map(|l| CodexLog::parse(l))
        .collect()
}

#[test]
fn sample_calls_count_input_minus_cached_plus_output() {
    let c = counts("codex-rollout.jsonl");
    let first = 16898.0 - 8960.0 + 294.0;
    let second = 17946.0 - 16128.0 + 209.0;
    assert_eq!(
        c.iter().map(|c| c.tokens).collect::<Vec<_>>(),
        vec![first, second, second]
    );
}

#[test]
fn sample_limits_are_read_by_window() {
    let c = counts("codex-rollout.jsonl");
    let c = c.first().expect("a count");
    // a free plan: one 30-day window, neither 5 h nor week
    let l = c.limits.as_ref();
    assert!(
        l.and_then(|l| l.five_hour.as_ref()).is_none() && l.and_then(|l| l.week.as_ref()).is_none()
    );
    let plus = r#"{"timestamp":"2026-10-31T01:03:19.553Z","type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{"primary":{"used_percent":12.0,"window_minutes":300,"resets_at":1793420421},"secondary":{"used_percent":40.0,"window_minutes":10080,"resets_at":1793805472}}}}"#;
    let l = CodexLog::parse(plus.as_bytes())
        .and_then(|c| c.limits)
        .expect("limits");
    assert_eq!(
        l.five_hour,
        Some(Window {
            used_percent: 12.0,
            resets_at: Some(1793420421.0)
        })
    );
    assert_eq!(
        l.week,
        Some(Window {
            used_percent: 40.0,
            resets_at: Some(1793805472.0)
        })
    );
}

#[test]
fn imported_rollout_adds_no_tokens() {
    assert!(
        counts("codex-imported-rollout.jsonl")
            .iter()
            .all(|c| c.tokens == 0.0 && c.limits.is_none())
    );
}

#[test]
fn reads_minutes_once_each_and_skips_the_repeated_event() {
    let home = make_home(&["codex-rollout.jsonl", "codex-imported-rollout.jsonl"]);
    let mut log = CodexLog::new("codex", home, None);
    let now = iso("2026-08-20T18:52:00.000Z");
    let m = log.read(now, 120 * 24 * 60).minutes; // the samples are from August and November
    let minute = |s: &str| iso(s) as i64 / 60;
    // 18:51:04 and 18:51:09 are two calls; 18:51:15 repeats the 18:51:09 totals
    let both = (16898.0 - 8960.0 + 294.0) + (17946.0 - 16128.0 + 209.0);
    assert_eq!(
        m,
        HashMap::from([(minute("2026-08-20T18:51:04.062Z"), both)])
    );
    assert_eq!(log.read(now, 120 * 24 * 60).minutes, m); // a second reading adds nothing
}

#[test]
fn sessions_are_named_from_the_index_else_the_first_prompt() {
    let home = make_home(&[]);
    let day = home.join("sessions/2026/11/01");
    let id = "01a0c32f-514f-7102-a63f-ddcb17038712"; // in the sample index
    let other = "01a0ffff-0000-7000-8000-000000000000";
    let at = iso_string(now() - 60.0, true);
    let rollout = |prompt: &str| {
        [
            format!(r#"{{"timestamp":"{at}","type":"session_meta","payload":{{"id":"x","cwd":"/r/demo"}}}}"#),
            format!(r#"{{"timestamp":"{at}","type":"event_msg","payload":{{"type":"user_message","message":"{prompt}"}}}}"#),
            format!(r#"{{"timestamp":"{at}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"total_tokens":1}},"last_token_usage":{{"input_tokens":100,"cached_input_tokens":40,"output_tokens":20}}}}}}}}"#),
        ]
        .join("\n")
            + "\n"
    };
    std::fs::write(
        day.join(format!("rollout-2026-11-01T10-00-00-{id}.jsonl")),
        rollout("Fix CI"),
    )
    .expect("write");
    std::fs::write(
        day.join(format!("rollout-2026-11-01T11-00-00-{other}.jsonl")),
        rollout("Plan the week"),
    )
    .expect("write");
    std::fs::copy(
        fixture("codex-session-index.jsonl"),
        home.join("session_index.jsonl"),
    )
    .expect("copy");
    let sessions = CodexLog::new("codex", home, None).read(now(), 60).sessions;
    assert_eq!(
        sessions
            .iter()
            .map(|s| s.id.as_str())
            .collect::<HashSet<_>>(),
        HashSet::from([id, other])
    );
    let name = |sid: &str| {
        sessions
            .iter()
            .find(|s| s.id == sid)
            .and_then(|s| s.name.clone())
    };
    assert_eq!(name(id).as_deref(), Some("Demo Codex thread"));
    assert_eq!(name(other).as_deref(), Some("Plan the week"));
    assert!(sessions.iter().all(|s| s.account == "codex"
        && s.cwd.as_deref() == Some("/r/demo")
        && s.tokens == 80.0
        && s.main_active));
}

#[test]
fn newest_limits_win() {
    let home = make_home(&[]);
    let day = home.join("sessions/2026/11/01");
    let line = |at: f64, pct: f64| {
        format!(
            r#"{{"timestamp":"{}","type":"event_msg","payload":{{"type":"token_count","info":null,"rate_limits":{{"primary":{{"used_percent":{pct},"window_minutes":300,"resets_at":1}}}}}}}}"#,
            iso_string(at, true)
        ) + "\n"
    };
    let now = now();
    std::fs::write(
        day.join(format!("rollout-a-{}.jsonl", uuid())),
        line(now - 120.0, 30.0),
    )
    .expect("write");
    std::fs::write(
        day.join(format!("rollout-b-{}.jsonl", uuid())),
        line(now - 60.0, 35.0),
    )
    .expect("write");
    let limits = CodexLog::new("codex", home, None).read(now, 60).limits;
    assert_eq!(
        limits.and_then(|l| l.five_hour).map(|w| w.used_percent),
        Some(35.0)
    );
}

fn token_count(at: f64, limits: &str) -> String {
    format!(
        r#"{{"timestamp":"{}","type":"event_msg","payload":{{"type":"token_count","info":null,"rate_limits":{limits}}}}}"#,
        iso_string(at, true)
    )
}

/// 2/11: the newest file was a rollout from 20/8, rewritten by a codex:// link test, with a free plan's 30-day
/// window only; today's rollout had `rate_limits: null`. The limits come from the newest event with 5 h or week.
#[test]
fn after_a_restart_the_newest_event_wins_not_the_newest_file() {
    let now = now();
    let home = make_home(&[]);
    let day = home.join("sessions/2026/11/01");
    let plus = format!(
        r#"{{"primary":{{"used_percent":12.0,"window_minutes":300,"resets_at":{}}},"secondary":{{"used_percent":40.0,"window_minutes":10080,"resets_at":{}}}}}"#,
        now as i64 - 600,
        now as i64 + 86400
    );
    let free = r#"{"primary":{"used_percent":48.0,"window_minutes":43200,"resets_at":1789813849},"secondary":null}"#;
    let files = [
        ("plus", token_count(now - 2.0 * 86400.0, &plus), -3600.0),
        ("free", token_count(now - 70.0 * 86400.0, free), -60.0), // rewritten a minute ago
        ("null", token_count(now - 120.0, "null"), -120.0),
    ];
    for (name, line, modified) in files {
        let path = day.join(format!(
            "rollout-2026-11-01T10-00-00-{name}-{}.jsonl",
            uuid()
        ));
        std::fs::write(&path, line + "\n").expect("write");
        set_modified(&path, now + modified);
    }
    let ledger = Arc::new(MinuteLedger::memory());
    let first = CodexLog::new("codex", home.clone(), Some(ledger.clone())).read(now, 60);
    assert_eq!(
        first.limits.and_then(|l| l.week).map(|w| w.used_percent),
        Some(40.0)
    );
    // a relaunch: the files continue from their marks, past those lines
    let limits = CodexLog::new("codex", home, Some(ledger))
        .read(now, 60)
        .limits
        .expect("limits");
    assert!(
        limits.five_hour.as_ref().map(|w| w.used_percent) == Some(12.0)
            && limits.week.as_ref().map(|w| w.used_percent) == Some(40.0)
    );
    // its 5 h window has reset: the week percent stays
    let sample = limits.sample().expect("sample");
    let status = LimitLog::new(None, None).read(
        now,
        LimitInput {
            samples: HashMap::from([("codex".to_owned(), sample)]),
            ..LimitInput::default()
        },
        &FixedOffset(0),
    );
    let entry = status.accounts.get("codex").expect("codex entry");
    assert!(entry.seven_day_pct == Some(40) && entry.note.is_some());
}
