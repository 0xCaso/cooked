//! Port of `Model/UsageSnapshot.swift`: what the island shows at one moment, live data from the watcher, or a demo.

use crate::live_snapshot::{FALLBACK_THRESHOLD_15M, HISTORY_MINUTES};
use crate::token_sources::TokenSource;

#[derive(Debug, Clone, PartialEq)]
pub struct Account {
    pub id: String,
    pub short: String,
    pub five_hour_percent: i64,
    pub five_hour_reset: String,
    pub is_live: bool,
    /// When the watcher expects the 5h window to reach 100%, before it resets: "HH:MM", or "now" at 100% and over.
    /// `None` when it is not expected to fill up before the reset, or when the account is blocked.
    pub full_at: Option<String>,
    /// The week's %, `None` when the week has already reset and no new data came in.
    pub week_percent: Option<i64>,
    /// When the week fills at the last hour's pace, within 5 h and before its reset: "HH:MM". `None` otherwise.
    pub week_full_at: Option<String>,
    /// The limit is hit (`limit_status` "rejected"): blocked until `five_hour_reset`.
    pub is_blocked: bool,
    /// No event in the last 5 hours: hidden from the popover.
    pub is_unused: bool,
    /// Even pace: the share of the 5h window that has gone by, 0...1. `None` when the reset is unknown.
    pub five_hour_elapsed: Option<f64>,
    /// The share of the week that has gone by, 0...1.
    pub week_elapsed: Option<f64>,
    /// The day the week resets: "Mon".
    pub week_reset: Option<String>,
    /// Cooked has a 5-hour percent for it: without one the popover shows "—" and the island no "%".
    pub has_percent: bool,
}

impl Account {
    /// The required fields; the rest take Swift's defaults.
    pub fn new(
        id: &str,
        short: &str,
        five_hour_percent: i64,
        five_hour_reset: &str,
        is_live: bool,
    ) -> Self {
        Self {
            id: id.to_owned(),
            short: short.to_owned(),
            five_hour_percent,
            five_hour_reset: five_hour_reset.to_owned(),
            is_live,
            full_at: None,
            week_percent: None,
            week_full_at: None,
            is_blocked: false,
            is_unused: false,
            five_hour_elapsed: None,
            week_elapsed: None,
            week_reset: None,
            has_percent: true,
        }
    }

    /// What is left of a window, counted like T3 Code's Usage page: 100 − used, within 0...100.
    pub fn left(used: i64) -> i64 {
        100 - used.clamp(0, 100)
    }

    /// The 5 h window's % left; 0 when blocked.
    pub fn five_hour_left(&self) -> i64 {
        if self.is_blocked {
            0
        } else {
            Self::left(self.five_hour_percent)
        }
    }

    /// `None` without a percent.
    pub fn five_hour_left_if_known(&self) -> Option<i64> {
        self.has_percent.then(|| self.five_hour_left())
    }
}

/// The spike line of the urgent island: when the 5h window fills up and what to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Urgency {
    /// % used of the 5 h window.
    pub percent: i64,
    /// The words before the time in the headline: "full at ", "free at ", "resets ".
    pub lead: String,
    pub full_at: String,
    pub detail: String,
    pub source: String,
}

impl Urgency {
    /// `lead` takes Swift's default, "full at ".
    pub fn new(percent: i64, full_at: &str, detail: &str, source: &str) -> Self {
        Self {
            percent,
            lead: "full at ".to_owned(),
            full_at: full_at.to_owned(),
            detail: detail.to_owned(),
            source: source.to_owned(),
        }
    }

    /// % left, what the card shows.
    pub fn left(&self) -> i64 {
        Account::left(self.percent)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsageSnapshot {
    pub clock: String,
    pub accounts: Vec<Account>,
    pub active_id: String,
    pub rate_now: i64,
    /// p99 threshold of the active account, tokens per minute: the island's.
    pub threshold: i64,
    /// The popover chart's tokens per minute, oldest first: live, every account summed; the demos, one account.
    pub series: Vec<f64>,
    /// The popover's "now" and alert line, every account summed; `None` in the demos, which show one account.
    pub total_rate: Option<i64>,
    pub total_threshold: Option<i64>,
    pub urgency: Option<Urgency>,
    /// "What burns tokens": the top sources of the last 15 minutes, empty when idle.
    pub sources: Vec<TokenSource>,
}

impl UsageSnapshot {
    /// No connected account: a flat wave and "Set up Cooked".
    pub fn empty(clock: &str) -> Self {
        Self {
            clock: clock.to_owned(),
            accounts: Vec::new(),
            active_id: String::new(),
            rate_now: 0,
            threshold: FALLBACK_THRESHOLD_15M / 15,
            series: vec![0.0; HISTORY_MINUTES],
            total_rate: Some(0),
            total_threshold: None,
            urgency: None,
            sources: Vec::new(),
        }
    }

    /// The popover's alert line, tokens per minute.
    pub fn chart_threshold(&self) -> i64 {
        self.total_threshold.unwrap_or(self.threshold)
    }

    pub fn is_urgent(&self) -> bool {
        self.urgency.is_some()
    }

    pub fn active(&self) -> Option<&Account> {
        self.accounts.iter().find(|a| a.id == self.active_id)
    }

    /// The accounts with a full row, in display order.
    pub fn rows(&self) -> Vec<&Account> {
        self.accounts.iter().filter(|a| !a.is_unused).collect()
    }

    /// The accounts not used in the last 5 hours, in display order.
    pub fn unused(&self) -> Vec<&Account> {
        self.accounts.iter().filter(|a| a.is_unused).collect()
    }

    /// The top source of `account`, else the top source: the popover marks its rate in red in urgency.
    pub fn peak_source<'a>(
        sources: &'a [TokenSource],
        account: Option<&str>,
    ) -> Option<&'a TokenSource> {
        sources
            .iter()
            .find(|s| Some(s.account.as_str()) == account)
            .or_else(|| sources.first())
    }
}
