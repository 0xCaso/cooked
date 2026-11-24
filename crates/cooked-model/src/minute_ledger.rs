//! Port of `Model/MinuteLedger.swift`. The minute cache: new tokens per account and minute for the last 14 days, and
//! where each session file was read up to, in `~/Library/Application Support/Cooked/minutes.json`. The readers
//! (`TranscriptLog`, `CodexLog`) add to it.
//! - The first launch reads 14 days of files, `budget` bytes per reading, newest files first, so the island keeps
//!   updating while the history fills (`history_progress`).
//! - At start, each file continues from its saved position: lines written while Cooked was closed are counted.
//!
//! Shared between the readers behind an `Arc`; every method takes `&self` and locks inside, like the Swift class.

use crate::files;
use crate::limit_log::Buckets;
use crate::transcript_log::TranscriptLog;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// Where a file was read up to.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Mark {
    #[serde(default)]
    pub offset: u64,
    /// Minute → new tokens of this file, the last hour: the sessions of the last 15 minutes.
    #[serde(default)]
    pub recent: HashMap<i64, f64>,
    /// Calls already counted (Claude message ids) → their minute, the last hour: a message is written once per
    /// content block.
    #[serde(default)]
    pub keys: HashMap<String, i64>,
    /// Codex: the totals of the last event, to skip a repeated one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<String>,
    /// Claude: the minute of the newest call counted in this file. After `/compact`, Claude Code writes copies of
    /// the latest calls again, with the same id and time: a call older than this is such a copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newest: Option<i64>,
}

/// The file's form; `version` has no default, so a file without it is dropped like in Swift.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    version: u32,
    #[serde(default)]
    minutes: HashMap<String, HashMap<i64, f64>>,
    #[serde(default)]
    marks: HashMap<String, Mark>,
    #[serde(default)]
    history_done: bool,
}

impl Default for Saved {
    fn default() -> Self {
        Self {
            version: MinuteLedger::VERSION,
            minutes: HashMap::new(),
            marks: HashMap::new(),
            history_done: false,
        }
    }
}

#[derive(Debug, Default)]
struct State {
    saved: Saved,
    last_save: Option<f64>,
    dirty: bool,
    /// This reading: bytes left to spend, and bytes still unread at the end of each file.
    left: i64,
    pending: HashMap<String, u64>,
    /// The files the readers walked in this reading; the marks of the others are dropped.
    seen: HashSet<String>,
    /// The history fill so far: bytes read, for the progress.
    filled: u64,
}

#[derive(Debug)]
pub struct MinuteLedger {
    pub url: Option<PathBuf>,
    pub budget: u64,
    state: Mutex<State>,
}

impl MinuteLedger {
    pub const DAYS: i64 = 14;
    pub const KEEP_MINUTES: i64 = Self::DAYS * 24 * 60;
    /// Bytes read per reading while the history fills.
    pub const DEFAULT_BUDGET: u64 = 256 << 20;
    /// Saved at most this often (seconds), and when the history is complete.
    pub const SAVE_EVERY: f64 = 60.0;
    /// 2: the 14 days read again without the `/compact` copies that version 1 counted twice.
    pub const VERSION: u32 = 2;

    /// `url` `None` keeps everything in memory (tests).
    pub fn new(url: Option<PathBuf>, budget: u64) -> Self {
        let saved = url
            .as_deref()
            .and_then(|u| std::fs::read(u).ok())
            .and_then(|data| serde_json::from_slice::<Saved>(&data).ok())
            .filter(|s| s.version == Self::VERSION)
            .unwrap_or_default();
        Self {
            url,
            budget,
            state: Mutex::new(State {
                saved,
                ..State::default()
            }),
        }
    }

    /// In memory, with the default budget.
    pub fn memory() -> Self {
        Self::new(None, Self::DEFAULT_BUDGET)
    }

    /// Saved at `url`, with the default budget.
    pub fn at(url: PathBuf) -> Self {
        Self::new(Some(url), Self::DEFAULT_BUDGET)
    }

    /// `~/Library/Application Support/Cooked/minutes.json`.
    pub fn default_path() -> Option<PathBuf> {
        files::support("minutes.json")
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 0...1 while the first 14 days are read; `None` when complete.
    pub fn history_progress(&self) -> Option<f64> {
        let s = self.lock();
        if s.saved.history_done {
            return None;
        }
        let rest: u64 = s.pending.values().sum();
        Some(if s.filled + rest == 0 {
            0.0
        } else {
            s.filled as f64 / (s.filled + rest) as f64
        })
    }

    // A reading

    /// Starts a reading, before every reader reads.
    pub fn begin(&self) {
        let mut s = self.lock();
        s.left = i64::try_from(self.budget).unwrap_or(i64::MAX);
        s.seen.clear();
    }

    /// Ends a reading, after every reader: drops minutes older than 14 days and files no reader saw, then saves
    /// when due.
    pub fn end(&self, now: f64) {
        {
            let mut guard = self.lock();
            let s = &mut *guard;
            let oldest = now as i64 / 60 - Self::KEEP_MINUTES;
            for m in s.saved.minutes.values_mut() {
                m.retain(|minute, _| *minute > oldest);
            }
            let seen = &s.seen;
            s.saved.marks.retain(|k, _| seen.contains(k));
            s.pending.retain(|k, _| seen.contains(k));
            if !s.saved.history_done && s.pending.values().all(|v| *v == 0) {
                s.saved.history_done = true;
                s.last_save = None; // save the complete history now
            }
        }
        self.save(now, false);
    }

    pub fn save(&self, now: f64, force: bool) {
        let Some(url) = &self.url else {
            return;
        };
        let data = {
            let mut s = self.lock();
            let due = s
                .last_save
                .is_none_or(|last| now - last >= Self::SAVE_EVERY);
            if !s.dirty || !(force || due) {
                return;
            }
            let Ok(data) = serde_json::to_vec(&s.saved) else {
                return;
            };
            s.dirty = false;
            s.last_save = Some(now);
            data
        };
        // like `try? data.write(to:options: .atomic)`: a failed save is tried again at the next one
        let _ = files::write_atomic(url, &data);
    }

    // For the readers

    /// A file a reader walked in this reading; its mark, if it has one.
    pub fn see(&self, path: &str) -> Option<Mark> {
        let mut s = self.lock();
        s.seen.insert(path.to_owned());
        s.saved.marks.get(path).cloned()
    }

    /// How many bytes a file may read now: all of them once the history is complete, else what is left of the
    /// budget.
    pub fn allowance(&self, _path: &str, unread: u64) -> u64 {
        let mut s = self.lock();
        if s.saved.history_done {
            return unread;
        }
        let n = unread.min(u64::try_from(s.left.max(0)).unwrap_or(0));
        s.left -= i64::try_from(n).unwrap_or(i64::MAX);
        n
    }

    /// After a file's lines are parsed: its new mark and the bytes it still has unread.
    pub fn update(&self, path: &str, mark: &Mark, unread: u64, read: u64, now: f64) {
        let mut s = self.lock();
        let hour = now as i64 / 60 - 60;
        let mut m = mark.clone();
        m.recent.retain(|minute, _| *minute > hour);
        // the newest minute's keys stay: its copies are not older than `newest`
        let newest = m.newest.unwrap_or(i64::MAX);
        m.keys
            .retain(|_, minute| *minute > hour || *minute >= newest);
        if s.saved.marks.get(path) != Some(&m) {
            s.saved.marks.insert(path.to_owned(), m);
            s.dirty = true;
        }
        s.pending.insert(path.to_owned(), unread);
        if !s.saved.history_done {
            s.filled += read;
        }
    }

    pub fn add(&self, account: &str, minute: i64, tokens: f64) {
        if tokens <= 0.0 {
            return;
        }
        let mut s = self.lock();
        *s.saved
            .minutes
            .entry(account.to_owned())
            .or_default()
            .entry(minute)
            .or_insert(0.0) += tokens;
        s.dirty = true;
    }

    /// Minute → new tokens of an account, from `first` to `last`.
    pub fn minutes(&self, account: &str, first: i64, last: i64) -> HashMap<i64, f64> {
        let s = self.lock();
        s.saved
            .minutes
            .get(account)
            .map(|m| {
                m.iter()
                    .filter(|(k, _)| **k >= first && **k <= last)
                    .map(|(k, v)| (*k, *v))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The new tokens of each 15-minute bucket with tokens, per account and every account summed: the alert
    /// thresholds' 14 days.
    pub fn buckets15(&self) -> Buckets {
        let s = self.lock();
        let mut out = Buckets::default();
        let mut total: BTreeMap<i64, f64> = BTreeMap::new();
        for (id, minutes) in &s.saved.minutes {
            let mut own: BTreeMap<i64, f64> = BTreeMap::new();
            for (minute, tokens) in minutes {
                *own.entry(minute.div_euclid(15)).or_insert(0.0) += tokens;
                *total.entry(minute.div_euclid(15)).or_insert(0.0) += tokens;
            }
            out.by_account
                .insert(id.clone(), own.into_values().collect());
        }
        out.total = total.into_values().collect();
        out
    }

    /// Where a reader starts a file it has no mark for: 14 days back.
    pub fn since(now: f64) -> f64 {
        now - (Self::KEEP_MINUTES * 60) as f64
    }

    /// Reads a file's complete new lines, within the allowance, and moves its mark. A file with no mark starts
    /// 14 days back. `line` parses one line and adds its call (`add`, and the mark's `recent` and `keys`).
    /// A line still being written (no newline yet) stays unread until it is complete.
    pub fn read(&self, path: &Path, now: f64, mut line: impl FnMut(&[u8], &mut Mark)) -> Mark {
        let key = path.to_string_lossy().into_owned();
        let mut mark = self.see(&key).unwrap_or_else(|| Mark {
            offset: TranscriptLog::start(path, Self::since(now), TranscriptLog::START_BLOCK),
            ..Mark::default()
        });
        let Some(end) = files::len(path) else {
            return mark;
        };
        if end < mark.offset {
            mark = Mark::default(); // rewritten
        }
        let n = self.allowance(&key, end - mark.offset);
        let mut consumed = 0;
        if n > 0
            && let Some(data) = files::read_at(path, mark.offset, n)
            && let Some(last) = data.iter().rposition(|b| *b == b'\n')
        {
            for l in files::lines(&data[..=last]) {
                line(l, &mut mark);
            }
            consumed = last as u64 + 1;
            mark.offset += consumed;
        }
        self.update(&key, &mark, end - mark.offset, consumed, now);
        mark
    }
}
