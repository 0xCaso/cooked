//! Shared test helpers: the fixtures (real Claude Code and Codex lines, anonymized) and temporary folders.
#![allow(dead_code)]

use cooked_model::clock::iso_string;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// The file's non-empty lines, like Swift's `Data.split(separator:)`.
pub fn lines(name: &str) -> Vec<Vec<u8>> {
    let data = std::fs::read(fixture(name)).expect("fixture");
    data.split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .map(<[u8]>::to_vec)
        .collect()
}

/// The `token_count` payloads of a Codex rollout.
pub fn token_counts(name: &str) -> Vec<serde_json::Value> {
    lines(name)
        .iter()
        .filter_map(|line| {
            let json: serde_json::Value = serde_json::from_slice(line).expect("json line");
            let payload = json.get("payload")?;
            (payload.get("type")?.as_str() == Some("token_count")).then(|| payload.clone())
        })
        .collect()
}

/// A new, empty folder under the system temp dir.
pub fn temp_dir(prefix: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "{prefix}-{}-{nanos}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// Now, in Unix seconds.
pub fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs_f64()
}

/// A lowercase id in the shape of `UUID().uuidString.lowercased()`: 36 characters.
pub fn uuid() -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos() as u64;
    let n = N.fetch_add(1, Ordering::Relaxed);
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        nanos as u32,
        (nanos >> 32) as u16,
        std::process::id() as u16,
        n as u16,
        (nanos ^ (n << 20)) & 0xffff_ffff_ffff
    )
}

/// Sets a file's change time to Unix seconds `at`.
pub fn set_modified(path: &Path, at: f64) {
    let time = if at >= 0.0 {
        UNIX_EPOCH + std::time::Duration::from_secs_f64(at)
    } else {
        UNIX_EPOCH - std::time::Duration::from_secs_f64(-at)
    };
    std::fs::File::options()
        .write(true)
        .open(path)
        .expect("open")
        .set_modified(time)
        .expect("set modified");
}

/// Appends raw text to a file, creating it.
pub fn append_raw(path: &Path, text: &str) {
    use std::io::Write;
    let mut f = std::fs::File::options()
        .create(true)
        .append(true)
        .open(path)
        .expect("open");
    f.write_all(text.as_bytes()).expect("write");
}

/// Appends lines, each ended by a newline.
pub fn append(path: &Path, lines: &[String]) {
    append_raw(path, &(lines.join("\n") + "\n"));
}

// T3 Code's provider log lines (`LimitLogTests.claudeLine`, `codexLine`, `logDir`).

/// A Claude `rate_limit_event` line as T3 Code logs it (30/10 11:59).
pub fn claude_line_full(
    at: f64,
    account: Option<&str>,
    five: f64,
    five_reset: f64,
    week: f64,
    week_reset: f64,
    status: &str,
) -> String {
    let session = account
        .map(|a| format!("provider-session:provider-instance:{a}:thread:t1:x"))
        .unwrap_or_else(|| "provider-session:x".to_owned());
    format!(
        r#"[{}] NTIVE: {{"provider":"claudeAgent","providerSessionId":"{session}","event":{{"direction":"incoming","stage":"decoded","payload":{{"type":"rate_limit_event","rate_limit_info":{{"status":"{status}","unifiedWindows":{{"five_hour":{{"utilization":{five},"resetsAt":{},"status":"allowed"}},"seven_day":{{"utilization":{week},"resetsAt":{},"status":"allowed"}}}}}}}}}}}}"#,
        iso_string(at, true),
        five_reset as i64,
        week_reset as i64
    )
}

pub fn claude_line(at: f64, account: Option<&str>, five: f64, five_reset: f64) -> String {
    claude_line_full(
        at,
        account,
        five,
        five_reset,
        0.38,
        1_793_714_400.0,
        "allowed",
    )
}

/// A Codex `account/rateLimits/updated` line.
pub fn codex_line(at: f64, used: f64, reset: f64, hit: bool) -> String {
    format!(
        r#"[{}] NTIVE: {{"provider":"codex","providerSessionId":"provider-session:provider-instance:codex:thread:t2:y","event":{{"payload":{{"method":"account/rateLimits/updated","params":{{"rateLimits":{{"primary":{{"usedPercent":{used},"windowDurationMins":300,"resetsAt":{}}},"secondary":{{"usedPercent":12,"windowDurationMins":10080,"resetsAt":{}}},"rateLimitReachedType":{}}}}}}}}}}}"#,
        iso_string(at, true),
        reset as i64,
        reset as i64 + 86400,
        if hit {
            r#""rate_limit_reached""#
        } else {
            "null"
        }
    )
}

pub fn log_dir(lines: &[String]) -> PathBuf {
    let dir = temp_dir("limits");
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(dir.join("events.t1.log"), text).expect("write");
    dir
}

// `LiveDataTests`' shared fixtures.

/// status.json of 29/10 15:25, as the watcher wrote it.
pub const STATUS_FIXTURE: &str = r#"
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
"#;

pub fn status_fixture() -> cooked_model::status_file::StatusFile {
    cooked_model::status_file::StatusFile::decode(STATUS_FIXTURE.as_bytes()).expect("fixture")
}

/// The connected accounts of the tests, with the names and order the island had before the account store.
pub fn test_accounts() -> Vec<cooked_model::accounts::ConnectedAccount> {
    [
        ("claudeAgent_work", "work"),
        ("claudeAgent_team", "team"),
        ("claudeAgent", "personal"),
        ("claudeAgent_side", "side"),
        ("codex", "codex"),
    ]
    .iter()
    .map(|(id, name)| cooked_model::accounts::ConnectedAccount::new(*id, *name, format!("/h/{id}")))
    .collect()
}

/// A source reading only the T3 Code limit logs in `dir`.
pub fn source(dir: PathBuf) -> cooked_model::usage_source::UsageSource {
    use cooked_model::limit_log::LimitLog;
    use cooked_model::minute_ledger::MinuteLedger;
    use cooked_model::status_line_setup::StatusLineSetup;
    cooked_model::usage_source::UsageSource {
        limits: LimitLog::new(Some(dir), None),
        bridge_dir: Some(PathBuf::from("/nonexistent")),
        status_line: StatusLineSetup::new("/nonexistent"),
        t3_database: Some(PathBuf::from("/nonexistent")),
        accounts: test_accounts(),
        transcripts: None,
        codex: None,
        ledger: std::sync::Arc::new(MinuteLedger::memory()),
    }
}

/// A source line, as Swift's `TokenSource(id:threadID:project:detail:account:rate:agents:)`.
pub fn token_source(
    id: &str,
    thread: Option<&str>,
    project: &str,
    detail: &str,
    account: &str,
    rate: f64,
    agents: Option<&str>,
) -> cooked_model::token_sources::TokenSource {
    cooked_model::token_sources::TokenSource {
        id: id.to_owned(),
        thread_id: thread.map(str::to_owned),
        project: project.to_owned(),
        detail: detail.to_owned(),
        account: account.to_owned(),
        rate,
        agents: agents.map(str::to_owned),
        ended_ago: None,
        session: None,
    }
}

/// Europe/Rome from 25/10/2026 (after the switch back from summer time) to the end of the year.
pub const ROME: cooked_model::clock::FixedOffset = cooked_model::clock::FixedOffset(3600);

/// A Rome wall-clock time in 2026, in Unix seconds.
pub fn rome(day: i64, month: i64, h: i64, m: i64, s: i64) -> f64 {
    (cooked_model::clock::days_from_civil(2026, month, day) * 86_400 + h * 3600 + m * 60 + s - 3600)
        as f64
}
