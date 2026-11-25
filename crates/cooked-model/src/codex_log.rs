//! New tokens per minute, the 5 h and week percent, and the sessions of a Codex
//! account, read from its rollouts (`<codex home>/sessions/<year>/<month>/<day>/rollout-<time>-<id>.jsonl`). Codex
//! writes a `token_count` event after each call, with that call's usage (`last_token_usage`) and the account's limits
//! (`rate_limits`). Rollouts from the Codex CLI, the Codex app and T3 Code are all written here. "New" counts like the
//! T3 Code database: input − cached input + output.
//! Keeps each file's read position between readings (in the `MinuteLedger`), so a reading only parses the lines
//! added since the last one.

use crate::accounts::{ConnectedAccount, Kind};
use crate::clock::iso_date;
use crate::files::{self, contains};
use crate::limit_log::Sample;
use crate::minute_ledger::MinuteLedger;
use crate::session_format::SessionFormat;
use crate::transcript_log::{Session, TranscriptLog, changed_since, recent, sort_sessions};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// One limit window.
#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    /// 0...100.
    pub used_percent: f64,
    /// Unix seconds.
    pub resets_at: Option<f64>,
}

/// The limits of the newest `token_count` that has them.
#[derive(Debug, Clone, PartialEq)]
pub struct Limits {
    pub five_hour: Option<Window>,
    pub week: Option<Window>,
    /// When Codex wrote them, Unix seconds.
    pub at: f64,
}

impl Limits {
    /// Has a window the island shows: a free plan's 30-day window alone does not count.
    pub fn shown(&self) -> bool {
        self.five_hour.is_some() || self.week.is_some()
    }

    /// As a limit sample; `None` without a 5 h window (a free plan).
    pub fn sample(&self) -> Option<Sample> {
        let five = self.five_hour.as_ref()?;
        Some(Sample {
            at: self.at,
            five: five.used_percent / 100.0,
            five_reset: five.resets_at,
            week: self.week.as_ref().map_or(0.0, |w| w.used_percent) / 100.0,
            week_reset: self.week.as_ref().and_then(|w| w.resets_at),
            status: if five.used_percent >= 100.0 {
                "rejected"
            } else {
                "allowed"
            }
            .to_owned(),
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reading {
    /// Minute → new tokens.
    pub minutes: HashMap<i64, f64>,
    /// Sessions with tokens in the last 15 minutes, most tokens first.
    pub sessions: Vec<Session>,
    pub limits: Option<Limits>,
    /// The newest rollout, when Cooked does not know its lines.
    pub format_changed: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Usage {
    pub input_tokens: Option<f64>,
    pub cached_input_tokens: Option<f64>,
    pub output_tokens: Option<f64>,
    pub total_tokens: Option<f64>,
}

impl Usage {
    /// input − cached + output.
    pub fn new_tokens(&self) -> f64 {
        (self.input_tokens.unwrap_or(0.0) - self.cached_input_tokens.unwrap_or(0.0)).max(0.0)
            + self.output_tokens.unwrap_or(0.0)
    }
}

#[derive(Deserialize)]
struct Info {
    total_token_usage: Option<Usage>,
    last_token_usage: Option<Usage>,
}

#[derive(Deserialize)]
struct RateWindow {
    used_percent: Option<f64>,
    window_minutes: Option<i64>,
    resets_at: Option<f64>,
}

#[derive(Deserialize)]
struct RateLimits {
    primary: Option<RateWindow>,
    secondary: Option<RateWindow>,
}

#[derive(Deserialize)]
struct Payload {
    #[serde(rename = "type")]
    kind: Option<String>,
    info: Option<Info>,
    rate_limits: Option<RateLimits>,
    // session_meta and user_message
    cwd: Option<String>,
    message: Option<String>,
}

#[derive(Deserialize)]
struct Line {
    timestamp: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    payload: Option<Payload>,
}

/// One `token_count` line → its time, new tokens, totals and limits.
#[derive(Debug, Clone, PartialEq)]
pub struct Count {
    /// Unix seconds.
    pub at: f64,
    pub stamp: String,
    pub tokens: f64,
    pub total: Option<Usage>,
    pub limits: Option<Limits>,
}

#[derive(Deserialize)]
struct IndexLine {
    id: Option<String>,
    thread_name: Option<String>,
}

pub struct CodexLog {
    /// The account id (`codex`) and its folder (`~/.codex`).
    pub id: String,
    pub home: PathBuf,
    /// The format of the newest rollout, checked once per file.
    formats: HashMap<String, SessionFormat>,
    /// Rollout path → (cwd, first prompt), from its first lines, read once.
    heads: HashMap<String, (Option<String>, Option<String>)>,
    /// Thread names from `session_index.jsonl`, and the index's size when read.
    names: (u64, HashMap<String, String>),
    limits: Option<Limits>,
    /// After a launch the rollouts' last lines were searched for the newest limits.
    searched: bool,
    /// The minute cache, where the tokens go. A ledger the reader made itself is begun and ended by `read`.
    ledger: Arc<MinuteLedger>,
    owns_ledger: bool,
}

impl CodexLog {
    /// The bytes `head` and `last_limits` read.
    const SCAN_BYTES: u64 = 256 * 1024;

    /// `ledger` `None`: the reader makes its own, in memory, and begins and ends it in `read`.
    pub fn new(
        id: impl Into<String>,
        home: impl Into<PathBuf>,
        ledger: Option<Arc<MinuteLedger>>,
    ) -> Self {
        let owns_ledger = ledger.is_none();
        Self {
            id: id.into(),
            home: home.into(),
            formats: HashMap::new(),
            heads: HashMap::new(),
            names: (0, HashMap::new()),
            limits: None,
            searched: false,
            ledger: ledger.unwrap_or_else(|| Arc::new(MinuteLedger::memory())),
            owns_ledger,
        }
    }

    /// The connected Codex account, if any.
    pub fn with_accounts(
        accounts: &[ConnectedAccount],
        ledger: Option<Arc<MinuteLedger>>,
    ) -> Option<Self> {
        let codex = accounts.iter().find(|a| a.kind() == Kind::Codex)?;
        Some(Self::new(codex.id.clone(), codex.path.clone(), ledger))
    }

    /// The minute cache this reader adds to.
    pub fn ledger(&self) -> Arc<MinuteLedger> {
        Arc::clone(&self.ledger)
    }

    /// Minute → new tokens for the `minutes` minutes up to `now` (from the minute cache), the sessions of the last
    /// 15 minutes and the newest limits. Walks the rollouts written in the last 14 days, newest first.
    pub fn read(&mut self, now: f64, minutes: i64) -> Reading {
        let ledger = Arc::clone(&self.ledger);
        if self.owns_ledger {
            ledger.begin();
        }
        let out = self.read_files(now, minutes, &ledger);
        if self.owns_ledger {
            ledger.end(now);
        }
        out
    }

    fn read_files(&mut self, now: f64, minutes: i64, ledger: &MinuteLedger) -> Reading {
        let minute_now = now as i64 / 60;
        let since = MinuteLedger::since(now);
        let mut out = Reading::default();
        let mut seen: HashSet<String> = HashSet::new();
        let mut sessions: Vec<Session> = Vec::new();
        let mut urls = changed_since(&self.home.join("sessions"), since, |p| {
            p.file_name()
                .is_some_and(|n| n.as_encoded_bytes().starts_with(b"rollout-"))
        });
        out.format_changed = SessionFormat::newest_changed(&urls, &mut self.formats);
        urls.sort_by_key(|u| std::cmp::Reverse(u.1));
        let id = self.id.clone();
        for (url, _) in &urls {
            let key = url.to_string_lossy().into_owned();
            seen.insert(key.clone());
            let limits = &mut self.limits;
            let mark = ledger.read(url, now, |line, mark| {
                let Some(count) = Self::parse(line) else {
                    return;
                };
                if let Some(l) = count.limits
                    && l.shown()
                    && l.at > limits.as_ref().map_or(f64::NEG_INFINITY, |x| x.at)
                {
                    *limits = Some(l);
                }
                // the same totals again: a repeated event, not a new call
                if let Some(total) = count.total.as_ref().map(Self::key) {
                    if mark.last.as_ref() == Some(&total) {
                        return;
                    }
                    mark.last = Some(total);
                }
                let minute = count.at as i64 / 60;
                if count.tokens <= 0.0 {
                    return;
                }
                *mark.recent.entry(minute).or_insert(0.0) += count.tokens;
                ledger.add(&id, minute, count.tokens);
            });
            let (fifteen, five, active, last) = recent(&mark.recent, minute_now);
            if fifteen > 0.0 {
                let head = match self.heads.get(&key) {
                    Some(h) => h.clone(),
                    None => Self::head(url),
                };
                self.heads.insert(key.clone(), head.clone());
                let sid = Self::session_of(url);
                let mut s = Session::new(&id, &sid, head.0, fifteen, active, 0);
                s.name = self.thread_name(&sid).or(head.1);
                s.last_minute = last;
                s.last_five = five;
                sessions.push(s);
            }
        }
        // after a launch the files continue from their marks: the newest limits in their last lines, by the event's
        // time (a file's change time says nothing: an old rollout can be rewritten)
        if !self.searched {
            self.searched = true;
            for (url, _) in &urls {
                if let Some(l) = Self::last_limits(url)
                    && l.at > self.limits.as_ref().map_or(f64::NEG_INFINITY, |x| x.at)
                {
                    self.limits = Some(l);
                }
            }
        }
        self.heads.retain(|k, _| seen.contains(k));
        out.minutes = ledger.minutes(&id, minute_now - minutes + 1, minute_now);
        sort_sessions(&mut sessions);
        out.sessions = sessions;
        out.limits = self.limits.clone();
        out
    }

    /// The newest limits with a shown window in the last 256 KB of a rollout.
    pub fn last_limits(path: &Path) -> Option<Limits> {
        let end = files::len(path)?;
        let from = end.saturating_sub(Self::SCAN_BYTES);
        let data = files::read_at(path, from, u64::MAX)?;
        files::lines(&data)
            .rev()
            .filter_map(|l| Self::parse(l)?.limits)
            .find(Limits::shown)
    }

    /// A repeated event has the same totals. Numbers print with a decimal point ("16898.0"): the key
    /// is saved in the minute cache, which earlier versions wrote too.
    pub fn key(u: &Usage) -> String {
        let n = |v: Option<f64>| format!("{:?}", v.unwrap_or(0.0));
        format!(
            "{}/{}/{}/{}",
            n(u.input_tokens),
            n(u.cached_input_tokens),
            n(u.output_tokens),
            n(u.total_tokens)
        )
    }

    /// `rollout-2026-10-20T09-05-37-01a15821-434f-....jsonl` → the thread id, the last 36 characters.
    pub fn session_of(path: &Path) -> String {
        let stem = path
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let count = stem.chars().count();
        stem.chars().skip(count.saturating_sub(36)).collect()
    }

    /// The thread's name in `session_index.jsonl`, the newest line for that id; re-read when the index grows.
    fn thread_name(&mut self, sid: &str) -> Option<String> {
        let path = self.home.join("session_index.jsonl");
        let size = files::len(&path).unwrap_or(0);
        if size != self.names.0 {
            let data = std::fs::read(&path).unwrap_or_default();
            self.names = (size, Self::thread_names(&data));
        }
        self.names.1.get(sid).cloned()
    }

    pub fn thread_names(index: &[u8]) -> HashMap<String, String> {
        let mut out = HashMap::new();
        for line in files::lines(index) {
            let Ok(l) = serde_json::from_slice::<IndexLine>(line) else {
                continue;
            };
            let (Some(id), Some(name)) = (l.id, TranscriptLog::clean(l.thread_name.as_deref()))
            else {
                continue;
            };
            out.insert(id, name); // lines are in time order: the last one wins
        }
        out
    }

    pub fn parse(line: &[u8]) -> Option<Count> {
        if !contains(line, br#""token_count""#) {
            return None;
        }
        let l: Line = serde_json::from_slice(line).ok()?;
        let p = l.payload?;
        if p.kind.as_deref() != Some("token_count") {
            return None;
        }
        let stamp = l.timestamp?;
        let at = iso_date(&stamp)?;
        let limits = p.rate_limits.map(|r| {
            let mut limits = Limits {
                five_hour: None,
                week: None,
                at,
            };
            for w in [r.primary, r.secondary].into_iter().flatten() {
                let window = Window {
                    used_percent: w.used_percent.unwrap_or(0.0),
                    resets_at: w.resets_at,
                };
                match w.window_minutes {
                    Some(300) => limits.five_hour = Some(window),
                    Some(10080) => limits.week = Some(window),
                    _ => {} // other plans' windows (a free plan has 30 days) are not shown
                }
            }
            limits
        });
        let (tokens, total) = match p.info {
            Some(info) => (
                info.last_token_usage
                    .as_ref()
                    .map_or(0.0, Usage::new_tokens),
                info.total_token_usage,
            ),
            None => (0.0, None),
        };
        Some(Count {
            at,
            stamp,
            tokens,
            total,
            limits,
        })
    }

    /// The working folder (`session_meta`) and the first prompt (`user_message`), from the first 256 KB.
    pub fn head(path: &Path) -> (Option<String>, Option<String>) {
        let Some(data) = files::read_at(path, 0, Self::SCAN_BYTES) else {
            return (None, None);
        };
        let (mut cwd, mut prompt): (Option<String>, Option<String>) = (None, None);
        for line in files::lines(&data) {
            if !((cwd.is_none() && contains(line, br#""session_meta""#))
                || (prompt.is_none() && contains(line, br#""user_message""#)))
            {
                continue;
            }
            let Ok(l) = serde_json::from_slice::<Line>(line) else {
                continue;
            };
            if l.kind.as_deref() == Some("session_meta") {
                cwd = l.payload.as_ref().and_then(|p| p.cwd.clone());
            }
            if let Some(p) = &l.payload
                && p.kind.as_deref() == Some("user_message")
                && let Some(m) = &p.message
                && !m.starts_with('<')
            {
                prompt = TranscriptLog::clean(Some(m));
            }
            if cwd.is_some() && prompt.is_some() {
                break;
            }
        }
        (cwd, prompt)
    }
}
