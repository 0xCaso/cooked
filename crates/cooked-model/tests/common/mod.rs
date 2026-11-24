//! Shared test helpers: the fixtures (real Claude Code and Codex lines, anonymized) and temporary folders.
#![allow(dead_code)]

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
