//! `OneDataPathTests.swift`: one data path. The session files, the bridge files and the minute cache; T3 Code's logs
//! only for a Claude percent with no bridge file yet.
mod common;

use common::{claude_line, codex_line, log_dir, source, temp_dir, uuid};
use cooked_model::clock::{FixedOffset, iso_string};
use cooked_model::codex_log::CodexLog;
use cooked_model::limit_log::{LimitInput, LimitLog, Sample};
use cooked_model::minute_ledger::MinuteLedger;
use cooked_model::status_line_bridge;
use cooked_model::token_sources::OpenTarget;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

const UTC: FixedOffset = FixedOffset(0);

/// The JSON Claude Code gives its status line, with the limits.
fn status_line(five: f64, five_reset: f64, week: f64, week_reset: f64) -> String {
    format!(
        r#"{{"session_id":"s","transcript_path":"/h/.claude-work/projects/p/s.jsonl","rate_limits":{{"five_hour":{{"used_percentage":{five},"resets_at":{}}},"seven_day":{{"used_percentage":{week},"resets_at":{}}}}}}}"#,
        five_reset as i64, week_reset as i64
    )
}

#[test]
fn reads_the_bridge_files() {
    let now = common::now();
    let dir = temp_dir("bridge");
    let (reset, week_reset) = (now + 3600.0, now + 86400.0);
    std::fs::write(dir.join("claudeAgent_work.json"), status_line(42.0, reset, 12.0, week_reset)).unwrap();
    // an API key user: no rate_limits, no percent
    std::fs::write(dir.join("claudeAgent.json"), r#"{"session_id":"s"}"#).unwrap();
    let s = status_line_bridge::samples(&dir);
    assert_eq!(s.keys().collect::<Vec<_>>(), ["claudeAgent_work"]);
    let work = &s["claudeAgent_work"];
    assert!(work.five == 0.42 && work.week == 0.12);
    assert_eq!(work.five_reset, Some(reset as i64 as f64));
    assert!((work.at - now).abs() < 5.0); // the file's change time
}

#[test]
fn the_bridge_wins_over_the_t3_code_logs() {
    let now = common::now();
    let reset = now + 3600.0;
    let logs = log_dir(&[
        claude_line(now - 60.0, Some("claudeAgent_work"), 0.30, reset),
        claude_line(now - 60.0, Some("claudeAgent"), 0.70, reset),
        codex_line(now - 60.0, 55.0, reset, false),
    ]);
    let bridge = temp_dir("bridge");
    std::fs::write(
        bridge.join("claudeAgent_work.json"),
        status_line(42.0, reset, 12.0, now + 86400.0),
    )
    .unwrap();
    let mut s = source(logs);
    s.bridge_dir = Some(bridge);
    let accounts = s.read(now, &UTC).snapshot.accounts;
    assert_eq!(
        accounts.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
        ["claudeAgent_work", "claudeAgent"]
    );
    // work from the bridge, personal from the logs until it has one
    assert_eq!(
        accounts.iter().map(|a| a.five_hour_percent).collect::<Vec<_>>(),
        [42, 70]
    );
    // Codex from T3 Code's logs is not read: its percent comes from the rollouts
}

fn rollout(home: &Path, day: &str, id: &str, line: &str) {
    let dir = home.join(format!("sessions/{day}"));
    std::fs::create_dir_all(&dir).unwrap();
    let date = day.replace('/', "-");
    std::fs::write(dir.join(format!("rollout-{date}T10-00-00-{id}.jsonl")), format!("{line}\n")).unwrap();
}

#[test]
fn codex_percent_comes_from_the_rollouts() {
    let now = common::now();
    let home = temp_dir("codex");
    let at = iso_string(now - 30.0, true);
    let reset = now as i64 + 3600;
    let line = format!(
        r#"{{"timestamp":"{at}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":1000,"cached_input_tokens":0,"output_tokens":10,"total_tokens":1010}},"last_token_usage":{{"input_tokens":1000,"cached_input_tokens":0,"output_tokens":10,"total_tokens":1010}}}},"rate_limits":{{"primary":{{"used_percent":12.0,"window_minutes":300,"resets_at":{reset}}},"secondary":{{"used_percent":40.0,"window_minutes":10080,"resets_at":{}}}}}}}}}"#,
        reset + 86400
    );
    rollout(&home, "2026/11/01", &uuid(), &line);
    let mut s = source(log_dir(&[]));
    s.codex = Some(CodexLog::new("codex", &home, Some(Arc::clone(&s.ledger))));
    let snapshot = s.read(now, &UTC).snapshot;
    assert_eq!(
        snapshot.accounts.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
        ["codex"]
    );
    assert!(snapshot.accounts[0].five_hour_percent == 12 && snapshot.accounts[0].week_percent == Some(40));
    assert_eq!(snapshot.series.iter().rev().take(2).sum::<f64>(), 1010.0);
    // after a restart the rollout continues from its mark: the limits come from its last lines
    let mut restarted = CodexLog::new("codex", &home, Some(Arc::clone(&s.ledger)));
    assert_eq!(
        restarted
            .read(now, 60)
            .limits
            .and_then(|l| l.five_hour)
            .map(|w| w.used_percent),
        Some(12.0)
    );
}

/// Task 8: no T3 Code database, the rows keep the session's own name and open the session itself.
#[test]
fn without_t3_code_rows_use_the_session_name() {
    let now = common::now();
    let home = temp_dir("codex");
    let at = iso_string(now - 30.0, true);
    let id = uuid();
    let line = format!(
        r#"{{"timestamp":"{at}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":3000,"cached_input_tokens":0,"output_tokens":10,"total_tokens":3010}},"last_token_usage":{{"input_tokens":3000,"cached_input_tokens":0,"output_tokens":10,"total_tokens":3010}}}},"rate_limits":{{"primary":{{"used_percent":5.0,"window_minutes":300,"resets_at":{}}}}}}}}}"#,
        now as i64 + 3600
    );
    rollout(&home, "2026/11/05", &id, &line);
    std::fs::write(
        home.join("session_index.jsonl"),
        format!("{{\"id\":\"{id}\",\"thread_name\":\"Fix the login page\"}}\n"),
    )
    .unwrap();
    let mut s = source(log_dir(&[])); // t3_database: /nonexistent
    s.codex = Some(CodexLog::new("codex", &home, Some(Arc::clone(&s.ledger))));
    let reading = s.read(now, &UTC);
    let row = reading.snapshot.sources.first().unwrap();
    assert_eq!(row.detail, "Fix the login page");
    assert_eq!(row.thread_id, None);
    assert_eq!(row.target(), Some(OpenTarget::codex(&id, None)));
}

#[test]
fn the_alert_lines_come_from_the_cache() {
    let now = common::now();
    let ledger = MinuteLedger::memory();
    let minute_now = now as i64 / 60;
    // 40 busy quarter hours for work, the busiest at 15 × 2000; personal adds 100 to each
    for q in 0..40 {
        for m in 0..15 {
            let minute = (minute_now / 15 - 40 + q) * 15 + m;
            ledger.add("claudeAgent_work", minute, if q == 39 { 2000.0 } else { 1000.0 });
            ledger.add("claudeAgent", minute, 100.0);
        }
    }
    let b = ledger.buckets15();
    assert_eq!(b.by_account.get("claudeAgent_work").map(Vec::len), Some(40));
    assert_eq!(b.total.iter().copied().fold(f64::NEG_INFINITY, f64::max), 15.0 * 2100.0);
    let mut log = LimitLog::new(None, None);
    let sample = Sample {
        at: now,
        five: 0.5,
        five_reset: Some(now + 3600.0),
        week: 0.1,
        week_reset: None,
        status: "allowed".to_owned(),
    };
    let buckets = || ledger.buckets15();
    let status = log.read(
        now,
        LimitInput {
            samples: HashMap::from([
                ("claudeAgent_work".to_owned(), sample.clone()),
                ("claudeAgent".to_owned(), sample),
            ]),
            buckets: Some(&buckets),
            ..Default::default()
        },
        &UTC,
    );
    assert_eq!(status.accounts["claudeAgent_work"].spike_threshold_15m, Some(30_000));
    assert_eq!(status.accounts["claudeAgent"].spike_threshold_15m, Some(1_500));
    assert_eq!(status.total_threshold_15m, Some(31_500));
}
