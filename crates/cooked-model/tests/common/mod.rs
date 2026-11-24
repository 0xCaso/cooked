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
        nanos ^ (n << 20)
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
