//! Port of `Model/SessionFormat.swift`. Claude Code and Codex do not document their files, so a reader first checks
//! that the lines are ones it knows: a file with lines but none known means the format changed, which the app shows
//! as "format changed", never as 0 tokens.

use serde::Deserialize;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionFormat {
    Claude,
    Codex,
    Empty,
    FormatChanged,
}

/// Claude transcript line kinds that every session has; other kinds (attachment, mode, ...) come and go.
pub const CLAUDE_KINDS: [&str; 6] = [
    "user",
    "assistant",
    "ai-title",
    "custom-title",
    "last-prompt",
    "summary",
];

#[derive(Deserialize)]
struct Payload {
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    payload: Option<Payload>,
}

impl SessionFormat {
    pub fn check<I>(lines: I) -> SessionFormat
    where
        I: IntoIterator,
        I::Item: AsRef<[u8]>,
    {
        let mut any = false;
        for raw in lines {
            let data = raw.as_ref();
            if data.iter().all(|b| *b == b' ') {
                continue;
            }
            any = true;
            let Ok(line) = serde_json::from_slice::<Line>(data) else {
                continue;
            };
            let Some(kind) = line.kind.as_deref() else {
                continue;
            };
            if kind == "session_meta"
                || (kind == "event_msg"
                    && line.payload.and_then(|p| p.kind).as_deref() == Some("token_count"))
            {
                return SessionFormat::Codex;
            }
            if CLAUDE_KINDS.contains(&kind) && line.session_id.is_some() {
                return SessionFormat::Claude;
            }
        }
        if any {
            SessionFormat::FormatChanged
        } else {
            SessionFormat::Empty
        }
    }

    pub const DEFAULT_LIMIT: usize = 256 * 1024;

    /// Checks the first 256 KB of a file, so a long transcript costs one read.
    pub fn check_file(file: &Path) -> SessionFormat {
        Self::check_file_limit(file, Self::DEFAULT_LIMIT)
    }

    pub fn check_file_limit(file: &Path, limit: usize) -> SessionFormat {
        let Ok(handle) = std::fs::File::open(file) else {
            return SessionFormat::Empty;
        };
        let mut data = Vec::new();
        if handle.take(limit as u64).read_to_end(&mut data).is_err() || data.is_empty() {
            return SessionFormat::Empty;
        }
        let mut lines: Vec<&[u8]> = data
            .split(|b| *b == b'\n')
            .filter(|l| !l.is_empty())
            .collect();
        if data.len() == limit && data.last() != Some(&b'\n') && lines.len() > 1 {
            lines.pop(); // cut line
        }
        Self::check(lines)
    }

    /// The newest file's path when its format changed, else `None`. `cache` keeps each file's answer, except
    /// "empty": a new file has no lines yet.
    pub fn newest_changed(
        urls: &[(PathBuf, SystemTime)],
        cache: &mut HashMap<String, SessionFormat>,
    ) -> Option<String> {
        // the first of equal times, like Swift's `max(by:)`
        let newest = urls
            .iter()
            .fold(None::<&(PathBuf, SystemTime)>, |best, u| match best {
                Some(b) if b.1 >= u.1 => Some(b),
                _ => Some(u),
            })?;
        let path = newest.0.to_string_lossy().into_owned();
        let format = cache
            .get(&path)
            .copied()
            .unwrap_or_else(|| Self::check_file(&newest.0));
        cache.clear();
        if format != SessionFormat::Empty {
            cache.insert(path.clone(), format);
        }
        (format == SessionFormat::FormatChanged).then_some(path)
    }
}
