//! Builds what the island shows from the accounts' limits (`LimitLog`) and the
//! per-minute tokens (the minute cache).

use crate::accounts::ConnectedAccount;
use crate::clock::{self, TimeZone};
use crate::status_file::{Entry, StatusFile, WatcherTime};
use crate::token_format;
use crate::token_sources::TokenSource;
use crate::usage_snapshot::{Account, Urgency, UsageSnapshot};
use std::collections::HashMap;

/// The watcher's fallback p99 for 15 minutes, when status.json has none.
pub const FALLBACK_THRESHOLD_15M: i64 = 500_000;
/// The chart keeps a day, for its longest range (`ChartRange.day`).
pub const HISTORY_MINUTES: usize = 24 * 60;
/// The island turns urgent when a window fills this soon. Red means "fills before the reset", never a spike or a
/// percent alone (2026-11-16-red-account-rule).
pub const URGENT_ETA: f64 = 20.0 * 60.0;
pub const OK_STATES: [&str; 3] = ["allowed", "allowed_warning", "?"];
pub const FIVE_HOURS: f64 = 5.0 * 3600.0;
pub const WEEK: f64 = 7.0 * 86400.0;
/// An account with no event for this long is hidden from the popover.
pub const UNUSED_AFTER: f64 = 5.0 * 3600.0;

/// Account id → wall-clock minute (Unix seconds / 60) → new tokens.
pub type Minutes = HashMap<String, HashMap<i64, f64>>;

/// The account's short name: "team"; its id when it is not connected.
pub fn short(id: &str, accounts: &[ConnectedAccount]) -> String {
    accounts
        .iter()
        .find(|a| a.id == id)
        .map_or_else(|| id.to_owned(), |a| a.name.clone())
}

/// `None` when there is no account to show. `sources` give the popover's hot thread in urgency. `accounts` are the
/// connected ones, in display order: the others are left out, and a connected account with tokens but no 5h percent
/// stays, with "—". Empty keeps every account with a percent, by id. `now`: Unix seconds.
pub fn make(
    status: &StatusFile,
    minutes: &Minutes,
    now: f64,
    tz: &dyn TimeZone,
    sources: &[TokenSource],
    accounts: &[ConnectedAccount],
) -> Option<UsageSnapshot> {
    let c = Ctx {
        status,
        minutes,
        now,
        tz,
        minute_now: now as i64 / 60,
        empty: Entry::default(),
    };
    let ids: Vec<String> = if accounts.is_empty() {
        status
            .accounts
            .iter()
            .filter(|(_, e)| e.five_hour_pct.is_some())
            .map(|(id, _)| id.clone())
            .collect()
    } else {
        accounts
            .iter()
            .map(|a| a.id.clone())
            .filter(|id| c.has_percent(id) || minutes.contains_key(id))
            .collect()
    };
    let first = ids.first()?.clone();

    let mut rows: Vec<Account> = ids
        .iter()
        .map(|id| {
            let e = c.entry(id);
            let week_percent = c.week_percent(id);
            Account {
                full_at: c.full_at(id),
                week_percent,
                week_full_at: c.week_eta(id).map(|t| self::clock(t, tz)),
                is_blocked: c.blocked(id),
                is_unused: c.unused(id),
                five_hour_elapsed: if c.stale(id) {
                    None
                } else {
                    c.reset_date(id).map(|r| elapsed(r, FIVE_HOURS, now))
                },
                week_elapsed: week_percent
                    .and_then(|_| c.week_reset_date(id))
                    .map(|r| elapsed(r, WEEK, now)),
                week_reset: c.week_reset(id),
                has_percent: c.has_percent(id),
                ..Account::new(
                    id,
                    &short(id, accounts),
                    c.percent(id),
                    &if c.stale(id) {
                        "—".to_owned()
                    } else {
                        clock_part(e.five_hour_resets_at.as_deref())
                            .unwrap_or_else(|| "—".to_owned())
                    },
                    e.live.unwrap_or(false),
                )
            }
        })
        .collect();

    let urgent_id = first_max(ids.iter().filter(|id| c.is_urgent(id)), |id| {
        c.percent(id) as f64
    });
    let live_ids: Vec<&String> = ids
        .iter()
        .filter(|id| c.entry(id).live == Some(true))
        .collect();
    let candidates: Vec<&String> = if live_ids.is_empty() {
        ids.iter().collect()
    } else {
        live_ids
    };
    let active_id = urgent_id
        .or_else(|| first_max(candidates.into_iter(), |id| c.recent(id)))
        .cloned()
        .unwrap_or(first);
    // the active account always keeps its row
    if let Some(a) = rows.iter_mut().find(|a| a.id == active_id) {
        a.is_unused = false;
    }

    let urgency = urgent_id.map(|id| c.urgency(id, sources, accounts));

    // The popover's chart sums every account; the island keeps the active account, since each account has its own
    // limit.
    let mut total = vec![0.0; HISTORY_MINUTES];
    for id in minutes.keys() {
        for (sum, v) in total.iter_mut().zip(c.series(id)) {
            *sum += v;
        }
    }
    let total_threshold = status
        .total_threshold_15m
        .or_else(|| ids.iter().map(|id| c.threshold(id) * 15).max())
        .unwrap_or(FALLBACK_THRESHOLD_15M)
        / 15;
    Some(UsageSnapshot {
        clock: self::clock(now, tz),
        accounts: rows,
        rate_now: c.rate(&active_id) as i64,
        threshold: c.threshold(&active_id),
        total_rate: Some(c.rate_of(&total) as i64),
        total_threshold: Some(total_threshold),
        series: total,
        urgency,
        sources: sources.to_vec(),
        active_id,
    })
}

/// The first element with the largest key.
fn first_max<'a, I>(items: I, key: impl Fn(&str) -> f64) -> Option<&'a String>
where
    I: Iterator<Item = &'a String>,
{
    let mut best: Option<(&String, f64)> = None;
    for item in items {
        let k = key(item);
        if best.is_none_or(|(_, b)| b < k) {
            best = Some((item, k));
        }
    }
    best.map(|(item, _)| item)
}

struct Ctx<'a> {
    status: &'a StatusFile,
    minutes: &'a Minutes,
    now: f64,
    tz: &'a dyn TimeZone,
    minute_now: i64,
    empty: Entry,
}

impl Ctx<'_> {
    fn entry(&self, id: &str) -> &Entry {
        self.status.accounts.get(id).unwrap_or(&self.empty)
    }

    fn parse(&self, s: &str) -> Option<f64> {
        WatcherTime::parse(s, self.now, self.tz)
    }

    fn has_percent(&self, id: &str) -> bool {
        self.status
            .accounts
            .get(id)
            .is_some_and(|e| e.five_hour_pct.is_some())
    }

    fn stale(&self, id: &str) -> bool {
        self.entry(id).note.is_some()
    }

    fn percent(&self, id: &str) -> i64 {
        if self.stale(id) {
            0
        } else {
            self.entry(id).five_hour_pct.unwrap_or(0)
        }
    }

    fn threshold(&self, id: &str) -> i64 {
        self.entry(id)
            .spike_threshold_15m
            .unwrap_or(FALLBACK_THRESHOLD_15M)
            / 15
    }

    fn blocked(&self, id: &str) -> bool {
        !self.stale(id) && self.entry(id).limit_status.as_deref() == Some("rejected")
    }

    /// `None` once it has gone by, or at the week reset and after.
    fn week_eta(&self, id: &str) -> Option<f64> {
        let eta = self
            .entry(id)
            .week_projection
            .as_ref()
            .and_then(|p| self.parse(&p.eta))?;
        if eta <= self.now || self.stale(id) || self.blocked(id) {
            return None;
        }
        if self.week_reset_date(id).is_some_and(|reset| eta >= reset) {
            return None;
        }
        Some(eta)
    }

    fn series(&self, id: &str) -> Vec<f64> {
        let first = self.minute_now - HISTORY_MINUTES as i64 + 1;
        match self.minutes.get(id) {
            Some(m) => (0..HISTORY_MINUTES as i64)
                .map(|i| m.get(&(first + i)).copied().unwrap_or(0.0))
                .collect(),
            None => vec![0.0; HISTORY_MINUTES],
        }
    }

    /// Tokens per minute over the last 5 minutes: the 4 whole minutes and the part of this one gone by, so the rate
    /// does not drop when a minute starts.
    fn rate_of(&self, series: &[f64]) -> f64 {
        let gone = 4.0 + (self.now % 60.0) / 60.0;
        series.iter().rev().take(5).sum::<f64>() / gone
    }

    /// From the same minutes as the chart.
    fn rate(&self, id: &str) -> f64 {
        self.rate_of(&self.series(id))
    }

    /// Tokens in the last 15 minutes, from the chart's minutes (the transcripts for Claude): the island follows the
    /// account that burns most, the same one the chart and the sources show.
    fn recent(&self, id: &str) -> f64 {
        self.series(id).iter().rev().take(15).sum()
    }

    /// `None` once it has gone by: the watcher projects from its last event, and an account that stopped never fills.
    fn eta(&self, id: &str) -> Option<f64> {
        self.entry(id)
            .projection
            .as_ref()
            .and_then(|p| self.parse(&p.eta))
            .filter(|&t| t > self.now)
    }

    fn reset_date(&self, id: &str) -> Option<f64> {
        self.entry(id)
            .five_hour_resets_at
            .as_deref()
            .and_then(|s| self.parse(s))
    }

    /// Shown only when it lands before the reset: the watcher means it so, and a later time says nothing.
    fn full_at(&self, id: &str) -> Option<String> {
        if self.stale(id) || self.blocked(id) {
            return None;
        }
        if self.percent(id) >= 100 {
            return Some("now".to_owned());
        }
        let eta = self.eta(id)?;
        if self.reset_date(id).is_some_and(|reset| eta >= reset) {
            return None;
        }
        Some(clock(eta, self.tz))
    }

    fn week_reset_date(&self, id: &str) -> Option<f64> {
        self.entry(id)
            .seven_day_resets_at
            .as_deref()
            .and_then(|s| self.parse(s))
    }

    /// `None` once the week has reset with no new data.
    fn week_percent(&self, id: &str) -> Option<i64> {
        if self
            .week_reset_date(id)
            .is_some_and(|reset| reset <= self.now)
        {
            return None;
        }
        self.entry(id).seven_day_pct
    }

    fn week_reset(&self, id: &str) -> Option<String> {
        self.week_percent(id)?;
        let reset = self.week_reset_date(id)?;
        // today the day says nothing: the time does
        Some(if same_day(reset, self.now, self.tz) {
            clock(reset, self.tz)
        } else {
            weekday(reset, self.tz)
        })
    }

    /// No event in the last 5 hours; without a last event, not live and no tokens in the last 15 minutes.
    fn unused(&self, id: &str) -> bool {
        if self.blocked(id) {
            return false;
        }
        let e = self.entry(id);
        if let Some(last) = e.last_event_at.as_deref().and_then(|s| self.parse(s)) {
            return self.now - last > UNUSED_AFTER;
        }
        e.live != Some(true)
            && self.recent(id) == 0.0
            && e.tokens_fresh
                .as_ref()
                .and_then(|t| t.get("15m"))
                .copied()
                .unwrap_or(0)
                == 0
    }

    fn is_urgent(&self, id: &str) -> bool {
        let e = self.entry(id);
        if self.stale(id) || !self.has_percent(id) || e.live != Some(true) {
            return false;
        }
        if self.percent(id) >= 100 {
            return true;
        }
        if self.eta(id).is_some_and(|eta| eta - self.now <= URGENT_ETA) {
            return true;
        }
        if self
            .week_eta(id)
            .is_some_and(|eta| eta - self.now <= URGENT_ETA)
        {
            return true;
        }
        e.limit_status
            .as_deref()
            .is_some_and(|state| !OK_STATES.contains(&state))
    }

    fn urgency(&self, id: &str, sources: &[TokenSource], accounts: &[ConnectedAccount]) -> Urgency {
        let name = short(id, accounts);
        let reset = clock_part(self.entry(id).five_hour_resets_at.as_deref())
            .unwrap_or_else(|| "—".to_owned());
        let minutes_to = |eta: f64| 0.max(((eta - self.now) / 60.0) as i64);
        // Without an ETA the headline says when the window frees up, never "full at —". Short: the card cuts at ~17
        // characters.
        let (lead, time, when): (&str, String, Option<String>) = if self.blocked(id) {
            ("free at ", reset.clone(), Some("is cooked".to_owned()))
        } else if let Some(eta) = self.eta(id) {
            (
                "full at ",
                clock(eta, self.tz),
                Some(format!("in {} min", minutes_to(eta))),
            )
        } else if let Some(eta) = self.week_eta(id) {
            (
                "full at ",
                clock(eta, self.tz),
                Some(format!("week in {} min", minutes_to(eta))),
            )
        } else if self.percent(id) >= 100 {
            ("full ", "now".to_owned(), Some(format!("resets {reset}")))
        } else {
            ("resets ", reset.clone(), None)
        };
        let rate = self.rate(id);
        // the thread "Open thread ↗" opens
        let source = UsageSnapshot::peak_source(sources, Some(&name))
            .filter(|s| s.rate >= 1.0)
            .map(|s| format!("{}/min from “{}”", token_format::short(s.rate), s.detail))
            .unwrap_or_else(|| {
                if rate >= 1.0 {
                    format!("{}/min now", token_format::short(rate))
                } else {
                    "Idle for 5 min".to_owned()
                }
            });
        Urgency {
            percent: self.percent(id),
            lead: lead.to_owned(),
            full_at: time,
            detail: when.map_or_else(|| name.clone(), |w| format!("{name} {w}")),
            source,
        }
    }
}

/// "30/10 19:00" → "19:00".
pub fn clock_part(s: Option<&str>) -> Option<String> {
    s?.split(' ').rfind(|p| !p.is_empty()).map(str::to_owned)
}

/// Even pace: how much of a window that ends at `reset` has gone by.
pub fn elapsed(reset: f64, window: f64, now: f64) -> f64 {
    (1.0 - (reset - now) / window).clamp(0.0, 1.0)
}

/// The local day of a moment, in days since 1970-01-01.
fn local_day(t: f64, tz: &dyn TimeZone) -> i64 {
    let c = clock::local(t, tz);
    clock::days_from_civil(c.year, c.month, c.day)
}

pub fn same_day(a: f64, b: f64, tz: &dyn TimeZone) -> bool {
    local_day(a, tz) == local_day(b, tz)
}

/// "Mon": English, as every other word in the island.
pub fn weekday(t: f64, tz: &dyn TimeZone) -> String {
    const NAMES: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    // 1970-01-01 was a Thursday
    NAMES[(local_day(t, tz) + 4).rem_euclid(7) as usize].to_owned()
}

/// "HH:MM", local time.
pub fn clock(t: f64, tz: &dyn TimeZone) -> String {
    let c = clock::local(t, tz);
    format!("{:02}:{:02}", c.hour, c.minute)
}
