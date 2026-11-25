//! The accounts' 5 h and week percent, and the paces and alert thresholds built from
//! them, in the form the watcher script's status.json had. The samples come from:
//! - Claude: the status line bridge files (`status_line_bridge`, terminal `claude` sessions) and T3 Code's provider
//!   logs (`~/.t3/userdata/logs/provider/events.<thread>.log`, read-only: the Agent SDK runs no status line, and logs
//!   each `rate_limit_event` when the percent changes). The newest sample of the two wins.
//! - Codex: the `rate_limits` of its rollouts (`CodexLog`).
//!
//! The alert thresholds are the p99 of the 15-minute buckets in the minute cache.
//! Keeps each log file's read position and the samples of the last hour across readings.

use crate::clock::{self, TimeZone, iso_date, iso_string, unix};
use crate::files::{self, contains};
use crate::status_file::{Entry, Projection, StatusFile};
use crate::token_log::TokenLog;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    /// Unix seconds.
    pub at: f64,
    /// Share used, 0...1 (over 1 when blocked).
    pub five: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_reset: Option<f64>,
    pub week: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub week_reset: Option<f64>,
    pub status: String,
}

/// 15-minute buckets of new tokens: each account's, and every account summed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Buckets {
    pub by_account: HashMap<String, Vec<f64>>,
    pub total: Vec<f64>,
}

/// A pace: its window in seconds, its label, and the fewest samples it needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pace {
    pub window: f64,
    pub label: &'static str,
    pub minimum: usize,
}

/// What a reading takes besides the logs.
#[derive(Default)]
pub struct LimitInput<'a> {
    /// This reading's samples: the bridge's and the rollouts'.
    pub samples: HashMap<String, Sample>,
    /// Called when the thresholds are due (hourly, or every reading while `fresh`); `None` gives empty buckets.
    pub buckets: Option<&'a dyn Fn() -> Buckets>,
    /// The minute cache is still filling: the thresholds are built every reading.
    pub fresh: bool,
    /// Gives the account of a T3 Code thread whose events do not name it.
    pub database: Option<&'a TokenLog>,
}

struct Thresholds {
    at: f64,
    values: HashMap<String, i64>,
    total: i64,
}

pub struct LimitLog {
    /// T3 Code's provider logs; `None` reads none.
    pub dir: Option<PathBuf>,
    /// Where the newest sample of each account is kept across launches; `None` keeps it in memory only (tests).
    pub store: Option<PathBuf>,
    /// Log path → read offset.
    files: HashMap<String, u64>,
    /// From the bridge and the rollouts.
    samples: HashMap<String, Vec<Sample>>,
    /// From T3 Code's logs, joined with the samples above for a Claude account.
    logged: HashMap<String, Vec<Sample>>,
    thresholds: Option<Thresholds>,
    /// The newest sample of each account, kept across launches in `store`: an account with no new event keeps its
    /// last known percent (its 5 h reset or week reset already gone by shows as such, see `entry`).
    last: BTreeMap<String, Sample>,
    first_read: bool,
}

impl LimitLog {
    pub const CLAUDE_FALLBACK_15M: i64 = 500_000;
    pub const CODEX_FALLBACK_15M: i64 = 350_000;
    /// Files not written for this long hold no new events.
    pub const FILE_MAX_AGE: f64 = 6.0 * 3600.0;
    /// The first reading after a launch looks back a week, the longest window: an account used days ago still has a
    /// week percent.
    pub const FIRST_READ_MAX_AGE: f64 = 7.0 * 86400.0;
    /// A file seen for the first time is read from this far before its end.
    pub const TAIL: u64 = 2 << 20;
    /// Samples older than this before the last one are dropped: the paces look back 30 minutes at most.
    pub const SAMPLE_KEEP: f64 = 3600.0;
    /// An account with an event this recent is live.
    pub const LIVE_MAX_AGE: f64 = 600.0;
    /// No projection under this share used: right after a reset the pace says nothing.
    pub const PROJECTION_MINIMUM: f64 = 0.20;
    /// The paces. A pace needs samples over `COVERAGE` of its window.
    pub const PACES: [Pace; 2] = [
        Pace {
            window: 300.0,
            label: "5 min",
            minimum: 3,
        },
        Pace {
            window: 1800.0,
            label: "30 min",
            minimum: 2,
        },
    ];
    pub const COVERAGE: f64 = 0.75;
    /// The week's pace: its percent moves about 1% at a time, so it takes the whole hour of samples kept.
    pub const WEEK_PACE: Pace = Pace {
        window: 3600.0,
        label: "1 h",
        minimum: 2,
    };
    /// A week projection counts only this close: further out, the pace of one hour says nothing about the days ahead.
    pub const WEEK_HORIZON: f64 = 5.0 * 3600.0;
    /// An account needs this many 15-min buckets of history for its own p99.
    pub const MINIMUM_BUCKETS: usize = 30;
    pub const THRESHOLDS_MAX_AGE: f64 = 3600.0;

    pub fn new(dir: Option<PathBuf>, store: Option<PathBuf>) -> Self {
        let last = store
            .as_deref()
            .and_then(|s| std::fs::read(s).ok())
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default();
        Self {
            dir,
            store,
            files: HashMap::new(),
            samples: HashMap::new(),
            logged: HashMap::new(),
            thresholds: None,
            last,
            first_read: true,
        }
    }

    /// `~/.t3/userdata/logs/provider`.
    pub fn default_dir() -> Option<PathBuf> {
        Some(files::home()?.join(".t3/userdata/logs/provider"))
    }

    /// `~/Library/Application Support/Cooked/limits.json`.
    pub fn default_store() -> Option<PathBuf> {
        files::support("limits.json")
    }

    /// Adds this reading's samples, reads the new log events and builds every account's limits.
    pub fn read(&mut self, now: f64, input: LimitInput<'_>, tz: &dyn TimeZone) -> StatusFile {
        for (id, sample) in input.samples {
            self.samples.entry(id).or_default().push(sample);
        }
        if let Some(dir) = self.dir.clone() {
            self.read_logs(&dir, now, input.database);
        }
        for s in self.samples.values_mut() {
            *s = Self::prune(s);
        }
        for s in self.logged.values_mut() {
            *s = Self::prune(s);
        }

        // A Claude account in the terminal and in T3 Code has both: one list, so the newest sample wins.
        let mut all: HashMap<String, Vec<Sample>> = self
            .logged
            .iter()
            .filter(|(id, _)| Self::is_claude(id))
            .map(|(id, s)| (id.clone(), s.clone()))
            .collect();
        for (id, s) in &self.samples {
            if s.is_empty() {
                continue;
            }
            let mut joined = all.remove(id).unwrap_or_default();
            joined.extend(s.iter().cloned());
            all.insert(id.clone(), Self::prune(&joined));
        }
        let mut changed = false;
        for (id, s) in &all {
            let Some(newest) = s.last() else {
                continue;
            };
            if newest.at > self.last.get(id).map_or(f64::NEG_INFINITY, |l| l.at) {
                self.last.insert(id.clone(), newest.clone());
                changed = true;
            }
        }
        for (id, sample) in &self.last {
            all.entry(id.clone())
                .or_insert_with(|| vec![sample.clone()]);
        }
        if changed
            && let Some(store) = &self.store
            && let Ok(data) = serde_json::to_vec(&self.last)
        {
            // a failed save is tried again at the next change
            let _ = files::write_atomic(store, &data);
        }
        if input.fresh
            || self
                .thresholds
                .as_ref()
                .is_none_or(|t| now - t.at >= Self::THRESHOLDS_MAX_AGE)
        {
            let b = input.buckets.map(|f| f()).unwrap_or_default();
            let ids: HashSet<String> = all.keys().cloned().collect();
            let values = Self::thresholds(&b.by_account, &ids);
            let fallback = values
                .values()
                .copied()
                .max()
                .unwrap_or(Self::CLAUDE_FALLBACK_15M);
            let total = Self::total_threshold(&b.total, fallback);
            self.thresholds = Some(Thresholds {
                at: now,
                values,
                total,
            });
        }
        let mut accounts = BTreeMap::new();
        for (id, s) in &all {
            if s.is_empty() {
                continue;
            }
            let threshold = self
                .thresholds
                .as_ref()
                .and_then(|t| t.values.get(id).copied());
            accounts.insert(id.clone(), Self::entry(id, s, threshold, now, tz));
        }
        StatusFile {
            updated_at: iso_string(now, false),
            accounts,
            total_threshold_15m: self.thresholds.as_ref().map(|t| t.total),
        }
    }

    fn read_logs(&mut self, dir: &Path, now: f64, database: Option<&TokenLog>) {
        let mut thread_accounts: Option<HashMap<String, String>> = None;
        let names: Vec<String> = std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok()?.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default();
        let mut seen = HashSet::new();
        let max_age = if self.first_read {
            Self::FIRST_READ_MAX_AGE
        } else {
            Self::FILE_MAX_AGE
        };
        self.first_read = false;
        for name in names
            .iter()
            .filter(|n| n.starts_with("events.") && n.ends_with(".log"))
        {
            let path = dir.join(name);
            let Some(modified) = files::modified(&path) else {
                continue;
            };
            if now - unix(modified) >= max_age {
                continue;
            }
            let key = path.to_string_lossy().into_owned();
            seen.insert(key.clone());
            let thread = name
                .get("events.".len()..name.len().saturating_sub(".log".len()))
                .unwrap_or("");
            let mut offset = match self.files.get(&key) {
                Some(o) => *o,
                None => Self::start(&path),
            };
            for (account, sample) in Self::read_new(&path, &mut offset) {
                let id = account.or_else(|| {
                    thread_accounts
                        .get_or_insert_with(|| {
                            database
                                .and_then(|d| d.thread_accounts().ok())
                                .unwrap_or_default()
                        })
                        .get(thread)
                        .cloned()
                });
                if let Some(id) = id {
                    self.logged.entry(id).or_default().push(sample);
                }
            }
            self.files.insert(key, offset);
        }
        self.files.retain(|k, _| seen.contains(k));
    }

    pub fn is_claude(id: &str) -> bool {
        id.starts_with("claudeAgent")
    }

    /// Sorted, one per time, only the current 5 h window (the last sample's reset), the last hour before the last
    /// one.
    pub fn prune(s: &[Sample]) -> Vec<Sample> {
        let mut sorted: Vec<Sample> = s.to_vec();
        sorted.sort_by(|a, b| a.at.total_cmp(&b.at));
        // one per time: the later one in `s` wins
        let mut unique: Vec<Sample> = Vec::with_capacity(sorted.len());
        for sample in sorted {
            match unique.last_mut() {
                Some(prev) if prev.at == sample.at => *prev = sample,
                _ => unique.push(sample),
            }
        }
        let Some(last) = unique.last().cloned() else {
            return Vec::new();
        };
        unique
            .into_iter()
            .filter(|x| x.five_reset == last.five_reset && x.at >= last.at - Self::SAMPLE_KEEP)
            .collect()
    }

    /// The account's limits, as the watcher wrote them.
    pub fn entry(
        id: &str,
        samples: &[Sample],
        threshold: Option<i64>,
        now: f64,
        tz: &dyn TimeZone,
    ) -> Entry {
        let Some(last) = samples.last() else {
            return Entry {
                name: id.to_owned(),
                ..Entry::default()
            };
        };
        let t = now;
        let live = t - last.at <= Self::LIVE_MAX_AGE;
        let reset = last.five_reset;
        let stale = reset.is_some_and(|r| r <= t);
        let fallback = if Self::is_claude(id) {
            Self::CLAUDE_FALLBACK_15M
        } else {
            Self::CODEX_FALLBACK_15M
        };
        let mut entry = Entry {
            name: id.to_owned(),
            tokens_fresh: None,
            spike_threshold_15m: Some(threshold.unwrap_or(fallback)),
            five_hour_pct: Some((last.five * 100.0).round() as i64),
            five_hour_resets_at: reset.map(|r| Self::clock(r, now, tz)),
            seven_day_pct: Some((last.week * 100.0).round() as i64),
            seven_day_resets_at: last.week_reset.map(|r| Self::clock(r, now, tz)),
            limit_status: Some(last.status.clone()),
            last_event_at: Some(Self::clock(last.at, now, tz)),
            live: Some(live),
            projection: None,
            week_projection: None,
            note: stale.then(|| "5h window already reset, no new data".to_owned()),
        };
        if live
            && let Some(reset) = reset
            && !stale
            && last.five >= Self::PROJECTION_MINIMUM
        {
            let mut best: Option<(f64, &str)> = None;
            for pace in Self::PACES {
                let Some(slope) = Self::slope(samples, pace.window, pace.minimum, |s| s.five)
                else {
                    continue;
                };
                if slope <= 0.0 {
                    continue;
                }
                let eta = last.at + (1.0 - last.five) / slope;
                if eta < reset && eta < best.map_or(f64::INFINITY, |b| b.0) {
                    best = Some((eta, pace.label));
                }
            }
            entry.projection = best.map(|(eta, label)| Projection {
                window: label.to_owned(),
                eta: Self::clock(eta, now, tz),
            });
        }
        if live
            && let Some(week_reset) = last.week_reset
            && week_reset > t
            && last.week >= Self::PROJECTION_MINIMUM
            && last.week < 1.0
            && let Some(slope) = Self::slope(
                samples,
                Self::WEEK_PACE.window,
                Self::WEEK_PACE.minimum,
                |s| s.week,
            )
            && slope > 0.0
        {
            let eta = last.at + (1.0 - last.week) / slope;
            if eta < week_reset && eta < t + Self::WEEK_HORIZON {
                entry.week_projection = Some(Projection {
                    window: Self::WEEK_PACE.label.to_owned(),
                    eta: Self::clock(eta, now, tz),
                });
            }
        }
        entry
    }

    /// Share per second over the window before the last sample; `None` with too few samples or under `COVERAGE` of
    /// the window (the script took 7.5 minutes for "30 min" and saw the work account full at 13:57; it was at 37% at
    /// 13:44).
    pub fn slope(
        samples: &[Sample],
        window: f64,
        minimum: usize,
        share: impl Fn(&Sample) -> f64,
    ) -> Option<f64> {
        let last = samples.last()?;
        let inside: Vec<&Sample> = samples
            .iter()
            .filter(|s| s.at >= last.at - window)
            .collect();
        let first = inside.first()?;
        if inside.len() < minimum || last.at - first.at < window * Self::COVERAGE {
            return None;
        }
        Some((share(last) - share(first)) / (last.at - first.at))
    }

    /// p99 of the summed 15-min buckets; with little history, `fallback` (the largest account threshold).
    pub fn total_threshold(buckets: &[f64], fallback: i64) -> i64 {
        Self::p99(buckets).unwrap_or(fallback)
    }

    pub fn p99(values: &[f64]) -> Option<i64> {
        if values.len() < Self::MINIMUM_BUCKETS {
            return None;
        }
        let mut sorted = values.to_vec();
        sorted.sort_by(f64::total_cmp);
        let i = (sorted.len() - 1).min(sorted.len() * 99 / 100);
        Some(sorted[i] as i64)
    }

    /// p99 of each account's 15-min buckets. An account with little history takes the largest p99 of the other
    /// Claude accounts (the same person's pace), else a fixed fallback: the team account sat at the fallback on 30/10.
    pub fn thresholds(
        buckets: &HashMap<String, Vec<f64>>,
        ids: &HashSet<String>,
    ) -> HashMap<String, i64> {
        let own: HashMap<&String, i64> = buckets
            .iter()
            .filter_map(|(id, values)| Some((id, Self::p99(values)?)))
            .collect();
        let every: HashSet<&String> = buckets.keys().chain(ids.iter()).collect();
        let claude: Vec<&String> = every
            .iter()
            .copied()
            .filter(|id| Self::is_claude(id))
            .collect();
        let mut out = HashMap::new();
        for id in every {
            let others = if Self::is_claude(id) {
                claude
                    .iter()
                    .filter(|c| **c != id)
                    .filter_map(|c| own.get(c).copied())
                    .max()
            } else {
                None
            };
            let fallback = if Self::is_claude(id) {
                Self::CLAUDE_FALLBACK_15M
            } else {
                Self::CODEX_FALLBACK_15M
            };
            out.insert(
                id.clone(),
                own.get(id).copied().or(others).unwrap_or(fallback),
            );
        }
        out
    }

    /// "HH:MM" on `now`'s day, else "dd/mm HH:MM", local time (`WatcherTime` reads it back).
    pub fn clock(t: f64, now: f64, tz: &dyn TimeZone) -> String {
        let c = clock::local(t, tz);
        let today = clock::local(now, tz);
        let hm = format!("{:02}:{:02}", c.hour, c.minute);
        if (c.year, c.month, c.day) == (today.year, today.month, today.day) {
            hm
        } else {
            format!("{:02}/{:02} {hm}", c.day, c.month)
        }
    }

    /// A file seen for the first time: its last `TAIL` bytes, from a line start.
    pub fn start(path: &Path) -> u64 {
        let Some(end) = files::len(path) else {
            return 0;
        };
        if end <= Self::TAIL {
            return 0;
        }
        let from = end - Self::TAIL;
        let Some(chunk) = files::read_at(path, from, 1 << 20) else {
            return end;
        };
        match chunk.iter().position(|b| *b == b'\n') {
            Some(newline) => from + newline as u64 + 1,
            None => end,
        }
    }

    /// The events in the complete lines written since `offset`; moves `offset` past them.
    fn read_new(path: &Path, offset: &mut u64) -> Vec<(Option<String>, Sample)> {
        let Some(end) = files::len(path) else {
            return Vec::new();
        };
        if end < *offset {
            *offset = 0; // rewritten
        }
        if end <= *offset {
            return Vec::new();
        }
        let Some(data) = files::read_at(path, *offset, u64::MAX) else {
            return Vec::new();
        };
        let Some(last) = data.iter().rposition(|b| *b == b'\n') else {
            return Vec::new();
        };
        *offset += last as u64 + 1;
        files::lines(&data[..=last])
            .filter_map(Self::parse)
            .collect()
    }

    /// `[2026-10-30T11:59:13.253Z] NTIVE: {…}` → the account named in its session id (`None`: ask the thread) and the
    /// sample. Claude: `rate_limit_event`, five_hour and seven_day utilization 0...1. Codex:
    /// `account/rateLimits/updated`, primary (300 min) and secondary, percents 0...100.
    pub fn parse(line: &[u8]) -> Option<(Option<String>, Sample)> {
        let claude = contains(line, br#""rate_limit_event""#);
        if !(claude || contains(line, br#""account/rateLimits/updated""#))
            || line.first() != Some(&b'[')
        {
            return None;
        }
        let close = line.iter().position(|b| *b == b']')?;
        let at = iso_date(std::str::from_utf8(line.get(1..close)?).ok()?)?;
        let brace = close + line.get(close..)?.iter().position(|b| *b == b'{')?;
        let obj: Value = serde_json::from_slice(line.get(brace..)?).ok()?;
        let obj = obj.as_object()?;
        let payload = obj.get("event")?.as_object()?.get("payload")?.as_object()?;
        let session = obj
            .get("providerSessionId")
            .and_then(Value::as_str)
            .unwrap_or("");
        let account = Self::instance(session);
        let empty = serde_json::Map::new();
        if let Some(info) = payload.get("rate_limit_info") {
            let info = info.as_object()?;
            let windows = info.get("unifiedWindows")?.as_object()?;
            let five = windows.get("five_hour")?.as_object()?;
            let week = windows
                .get("seven_day")
                .and_then(Value::as_object)
                .unwrap_or(&empty);
            return Some((
                account,
                Sample {
                    at,
                    five: number(five.get("utilization")),
                    five_reset: optional_number(five.get("resetsAt")),
                    week: number(week.get("utilization")),
                    week_reset: optional_number(week.get("resetsAt")),
                    status: info
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("?")
                        .to_owned(),
                },
            ));
        }
        let limits = payload
            .get("params")?
            .as_object()?
            .get("rateLimits")?
            .as_object()?;
        let primary = limits.get("primary")?.as_object()?;
        if number(primary.get("windowDurationMins")) != 300.0 {
            return None;
        }
        let secondary = limits
            .get("secondary")
            .and_then(Value::as_object)
            .unwrap_or(&empty);
        let hit = limits
            .get("rateLimitReachedType")
            .is_some_and(|v| !v.is_null());
        Some((
            account.or_else(|| Some("codex".to_owned())),
            Sample {
                at,
                five: number(primary.get("usedPercent")) / 100.0,
                five_reset: optional_number(primary.get("resetsAt")),
                week: number(secondary.get("usedPercent")) / 100.0,
                week_reset: optional_number(secondary.get("resetsAt")),
                status: if hit { "rejected" } else { "allowed" }.to_owned(),
            },
        ))
    }

    /// `provider-session:provider-instance:claudeAgent_team:thread:…` → "claudeAgent_team".
    pub fn instance(session: &str) -> Option<String> {
        let key = "provider-instance:";
        let rest = &session[session.find(key)? + key.len()..];
        let id: String = rest
            .chars()
            .take_while(|c| c.is_alphabetic() || c.is_numeric() || *c == '_' || *c == '-')
            .collect();
        (!id.is_empty() && rest[id.len()..].starts_with(':')).then_some(id)
    }
}

fn optional_number(v: Option<&Value>) -> Option<f64> {
    v?.as_f64()
}

fn number(v: Option<&Value>) -> f64 {
    optional_number(v).unwrap_or(0.0)
}
