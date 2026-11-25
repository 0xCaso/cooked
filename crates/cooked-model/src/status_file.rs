//! Every account's limits, as the watcher script's status.json had them:
//! `LimitLog` builds it now, and the tests still decode fixtures in that form. Only the fields the island uses.

use crate::clock::{self, Civil, TimeZone};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct StatusFile {
    pub updated_at: String,
    pub accounts: BTreeMap<String, Entry>,
    /// p99 of every account's 15-min buckets summed, for the popover's chart. Not in the watcher's file.
    #[serde(skip)]
    pub total_threshold_15m: Option<i64>,
}

/// Keys are spelled out, not renamed from snake case: that would also rename account ids like "claudeAgent_work".
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct Entry {
    pub name: String,
    #[serde(default)]
    pub tokens_fresh: Option<BTreeMap<String, i64>>,
    #[serde(default)]
    pub spike_threshold_15m: Option<i64>,
    #[serde(default)]
    pub five_hour_pct: Option<i64>,
    /// "HH:MM" today, or "dd/mm HH:MM".
    #[serde(default)]
    pub five_hour_resets_at: Option<String>,
    #[serde(default)]
    pub seven_day_pct: Option<i64>,
    #[serde(default)]
    pub seven_day_resets_at: Option<String>,
    /// "allowed", "allowed_warning", or a state that means the limit is hit.
    #[serde(default)]
    pub limit_status: Option<String>,
    /// "HH:MM" today, or "dd/mm HH:MM": the account's last event.
    #[serde(default)]
    pub last_event_at: Option<String>,
    #[serde(default)]
    pub live: Option<bool>,
    #[serde(default)]
    pub projection: Option<Projection>,
    /// When the week fills at the last hour's pace, before its reset and within `LimitLog.weekHorizon`.
    #[serde(default)]
    pub week_projection: Option<Projection>,
    /// Set when the 5h window has already reset and no new data came in.
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Projection {
    pub window: String,
    pub eta: String,
}

impl StatusFile {
    pub fn decode(data: &[u8]) -> serde_json::Result<StatusFile> {
        serde_json::from_slice(data)
    }
}

/// The watcher's times: "HH:MM" (today) or "dd/mm HH:MM", local time.
pub struct WatcherTime;

impl WatcherTime {
    /// `now` and the result: Unix seconds.
    pub fn parse(s: &str, now: f64, tz: &dyn TimeZone) -> Option<f64> {
        let parts: Vec<&str> = s.split(' ').filter(|p| !p.is_empty()).collect();
        let (h, m) = pair(parts.last()?, ':')?;
        let today = clock::local(now, tz);
        let mut day = Civil {
            hour: h,
            minute: m,
            second: 0,
            ..today
        };
        if parts.len() == 2 {
            let (d, mo) = pair(parts[0], '/')?;
            day.day = d;
            day.month = mo;
        }
        let mut date = clock::from_local(day, tz);
        // "02/01 10:00" read on 30/12 is next year's January.
        if parts.len() == 2 && date < now - 180.0 * 86_400.0 {
            let mut next = clock::local(date, tz);
            next.year += 1;
            if next.month == 2 && next.day == 29 && !clock::is_leap(next.year) {
                next.day = 28;
            }
            date = clock::from_local(next, tz);
        }
        Some(date)
    }
}

fn pair(s: &str, sep: char) -> Option<(i64, i64)> {
    let p: Vec<i64> = s
        .split(sep)
        .filter(|x| !x.is_empty())
        .filter_map(|x| x.parse().ok())
        .collect();
    (p.len() == 2).then(|| (p[0], p[1]))
}
