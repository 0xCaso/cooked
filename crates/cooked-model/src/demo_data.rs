//! Port of `Model/DemoData.swift`: the mockup's demo data (src/lib.js). `calm` is a real status.json of 28/10 19:28,
//! `peak` (23:05) is invented. The week % of every account but the work account is invented. `Feed` is
//! `IslandModel.Feed`: what the island shows.

use crate::token_sources::TokenSource;
use crate::usage_snapshot::{Account, Urgency, UsageSnapshot};

pub const THRESHOLD: i64 = 40000;
pub const CALM_RATES: [i64; 4] = [9000, 11000, 8000, 10000];

/// Live data or one of the demos. Not saved: every launch starts live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Feed {
    Live,
    DemoCalm,
    DemoPeak,
}

impl Feed {
    pub const ALL: [Feed; 3] = [Feed::Live, Feed::DemoCalm, Feed::DemoPeak];

    /// The Swift raw value, as `--feed` takes it.
    pub fn raw(self) -> &'static str {
        match self {
            Feed::Live => "live",
            Feed::DemoCalm => "demoCalm",
            Feed::DemoPeak => "demoPeak",
        }
    }

    pub fn from_raw(raw: &str) -> Option<Feed> {
        Self::ALL.into_iter().find(|f| f.raw() == raw)
    }

    pub fn title(self) -> &'static str {
        match self {
            Feed::Live => "Live",
            Feed::DemoCalm => "Demo: calm",
            Feed::DemoPeak => "Demo: peak",
        }
    }

    /// The demo's snapshot; `None` for live data.
    pub fn demo(self) -> Option<UsageSnapshot> {
        match self {
            Feed::Live => None,
            Feed::DemoCalm => Some(calm()),
            Feed::DemoPeak => Some(peak()),
        }
    }
}

/// Tokens per minute, the same seeded series as the mockup, so the charts match its screenshots.
pub fn series(n: usize, peak: bool) -> Vec<f64> {
    let mut seed: i64 = 7;
    let mut rnd = move || {
        seed = (seed * 16807) % 2_147_483_647;
        seed as f64 / 2_147_483_647.0
    };
    (0..n)
        .map(|i| {
            let k = i as f64 / (n as f64 - 1.0);
            let mut v = 6000.0 + rnd() * 9000.0 + (i as f64 / 3.0).sin() * 2500.0;
            if peak {
                v = 12000.0 + rnd() * 14000.0 + k.powf(6.0) * 150_000.0;
                v += if i + 9 > n {
                    40000.0 + rnd() * 50000.0
                } else {
                    0.0
                };
            } else if (k * 59.0).round() as i64 == 41 {
                v = 47000.0;
            }
            if rnd() < 0.3 {
                v += rnd() * 5000.0;
            }
            v
        })
        .collect()
}

fn account(
    id: &str,
    short: &str,
    percent: i64,
    reset: &str,
    live: bool,
    week: Option<(i64, f64, f64, &str)>,
) -> Account {
    let mut a = Account::new(id, short, percent, reset, live);
    if let Some((week_percent, five_elapsed, week_elapsed, week_reset)) = week {
        a.week_percent = Some(week_percent);
        a.five_hour_elapsed = Some(five_elapsed);
        a.week_elapsed = Some(week_elapsed);
        a.week_reset = Some(week_reset.to_owned());
    }
    a
}

pub fn calm() -> UsageSnapshot {
    let mut personal = account("per", "personal", 0, "—", false, None);
    personal.is_unused = true;
    UsageSnapshot {
        clock: "19:28".to_owned(),
        accounts: vec![
            account("work", "work", 4, "00:20", true, Some((74, 0.38, 0.71, "Mon"))),
            account("team", "team", 11, "04:40", false, Some((22, 0.04, 0.3, "Wed"))),
            personal,
            account("side", "side", 12, "01:55", false, Some((41, 0.11, 0.5, "Tue"))),
            account("cdx", "codex", 6, "03:10", false, Some((9, 0.2, 0.14, "Sun"))),
        ],
        active_id: "work".to_owned(),
        rate_now: 9000,
        threshold: THRESHOLD,
        series: series(60, false),
        total_rate: None,
        total_threshold: None,
        urgency: None,
        sources: Vec::new(),
    }
}

fn source(
    id: &str,
    thread: Option<&str>,
    project: &str,
    detail: &str,
    account: &str,
    rate: f64,
    agents: Option<&str>,
) -> TokenSource {
    TokenSource {
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

pub fn peak() -> UsageSnapshot {
    let mut work = account("work", "work", 82, "00:20", true, Some((79, 0.55, 0.71, "Mon")));
    work.full_at = Some("23:14".to_owned());
    let mut personal = account(
        "per",
        "personal",
        31,
        "02:40",
        true,
        Some((64, 0.15, 0.42, "Thu")),
    );
    personal.full_at = Some("01:50".to_owned());
    UsageSnapshot {
        clock: "23:05".to_owned(),
        accounts: vec![
            work,
            account("team", "team", 12, "04:40", false, Some((23, 0.07, 0.3, "Wed"))),
            personal,
            account("side", "side", 12, "01:55", false, Some((41, 0.11, 0.5, "Tue"))),
            account("cdx", "codex", 9, "03:10", false, Some((10, 0.2, 0.14, "Sun"))),
        ],
        active_id: "work".to_owned(),
        rate_now: 184_000,
        threshold: THRESHOLD,
        series: series(60, true),
        total_rate: None,
        total_threshold: None,
        urgency: Some(Urgency::new(
            82,
            "23:14",
            "claude-work in 9 min",
            "184k/min from “Refactor usage page”",
        )),
        sources: vec![
            source(
                "t1",
                Some("t1"),
                "cooked",
                "Refactor usage page",
                "work",
                113_000.0,
                Some("1 agent + 3 sub"),
            ),
            source(
                "t2",
                None,
                "garden-notes",
                "Draft the release notes",
                "work",
                18_000.0,
                Some("1 agent"),
            ),
            source(
                "t3",
                None,
                "t3 scratch",
                "Onboarding copy pass",
                "personal",
                4000.0,
                None,
            ),
        ],
    }
}
