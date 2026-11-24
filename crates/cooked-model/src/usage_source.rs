//! Port of `Model/UsageSource.swift`: where the live island reads from. The session files of the connected accounts
//! (tokens per minute, sessions and the Codex percent), the status line bridge files (the Claude percent) and the
//! minute cache (the alert thresholds). T3 Code adds thread titles when its database exists; until the bridge is
//! connected, its logs give the Claude percent.

use crate::accounts::{ConnectedAccount, Kind};
use crate::clock::TimeZone;
use crate::codex_log::CodexLog;
use crate::limit_log::{LimitInput, LimitLog};
use crate::live_snapshot::{self, HISTORY_MINUTES};
use crate::minute_ledger::MinuteLedger;
use crate::notice::Notice;
use crate::status_line_bridge;
use crate::status_line_setup::{State, StatusLineSetup};
use crate::token_log::TokenLog;
use crate::token_sources;
use crate::transcript_log::TranscriptLog;
use crate::usage_snapshot::UsageSnapshot;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

pub struct UsageSource {
    /// The percents; keeps their samples across readings.
    pub limits: LimitLog,
    /// `None` reads no bridge file.
    pub bridge_dir: Option<PathBuf>,
    /// Whether each Claude account's status line still runs the bridge.
    pub status_line: StatusLineSetup,
    /// T3 Code's database: thread titles only. No number depends on it. `None` or a missing file: no titles.
    pub t3_database: Option<PathBuf>,
    /// The connected accounts, in display order, with their names.
    pub accounts: Vec<ConnectedAccount>,
    /// The Claude accounts' minutes and sessions.
    pub transcripts: Option<TranscriptLog>,
    /// The Codex account's minutes, sessions and percent.
    pub codex: Option<CodexLog>,
    /// The minute cache both readers fill.
    pub ledger: Arc<MinuteLedger>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub snapshot: UsageSnapshot,
    /// Wall-clock minute (Unix seconds / 60) of the last value in `snapshot.series`.
    pub minute: i64,
    /// 0...1 while the first launch reads 14 days of history; `None` when complete.
    pub history: Option<f64>,
    /// What the popover's notice card can say, first the one to show.
    pub notices: Vec<Notice>,
}

impl UsageSource {
    /// The app's source: T3 Code's logs and database, Cooked's bridge folder and limits store in their usual places.
    pub fn new(accounts: Vec<ConnectedAccount>, ledger: Arc<MinuteLedger>) -> Self {
        Self {
            limits: LimitLog::new(LimitLog::default_dir(), LimitLog::default_store()),
            bridge_dir: status_line_bridge::default_dir(),
            status_line: StatusLineSetup::default(),
            t3_database: TokenLog::default_path(),
            transcripts: Some(TranscriptLog::with_accounts(
                &accounts,
                Some(Arc::clone(&ledger)),
            )),
            codex: CodexLog::with_accounts(&accounts, Some(Arc::clone(&ledger))),
            accounts,
            ledger,
        }
    }

    /// One reading at `now` (Unix seconds).
    pub fn read(&mut self, now: f64, tz: &dyn TimeZone) -> Reading {
        let minutes = HISTORY_MINUTES as i64;
        self.ledger.begin();
        let mut calls = self
            .transcripts
            .as_mut()
            .map(|t| t.read(now, minutes))
            .unwrap_or_default();
        let mut samples = self
            .bridge_dir
            .as_deref()
            .map(status_line_bridge::samples)
            .unwrap_or_default();
        let mut format_changed = std::mem::take(&mut calls.format_changed);
        if let Some(codex) = self.codex.as_mut()
            && codex.home.join("sessions").exists()
        {
            let rollouts = codex.read(now, minutes);
            match rollouts.format_changed {
                Some(file) => format_changed.insert(codex.id.clone(), file),
                None => format_changed.remove(&codex.id),
            };
            calls.minutes.insert(codex.id.clone(), rollouts.minutes);
            calls.sessions.extend(rollouts.sessions);
            if let Some(sample) = rollouts.limits.and_then(|l| l.sample()) {
                samples.insert(codex.id.clone(), sample);
            }
        }
        self.ledger.end(now);
        let history = self.ledger.history_progress();
        let database = self
            .t3_database
            .as_ref()
            .filter(|p| p.exists())
            .map(TokenLog::new);
        let ledger = Arc::clone(&self.ledger);
        let buckets = move || ledger.buckets15();
        let status = self.limits.read(
            now,
            LimitInput {
                samples,
                buckets: Some(&buckets),
                fresh: history.is_some(),
                database: database.as_ref(),
            },
            tz,
        );
        let ids: Vec<String> = calls.sessions.iter().map(|s| s.id.clone()).collect();
        let threads = database
            .as_ref()
            .and_then(|d| d.threads(&ids).ok())
            .unwrap_or_default();
        let sources = token_sources::make(&calls.sessions, &threads, &self.accounts, Some(now));
        let lines: HashMap<String, State> = self
            .accounts
            .iter()
            .filter(|a| a.kind() == Kind::Claude)
            .map(|a| {
                let state = self.status_line.state(&format!("{}/settings.json", a.path));
                (a.id.clone(), state)
            })
            .collect();
        let with_percent: HashSet<String> = status
            .accounts
            .iter()
            .filter(|(_, e)| e.five_hour_pct.is_some())
            .map(|(id, _)| id.clone())
            .collect();
        let bridged = self
            .bridge_dir
            .as_deref()
            .map(status_line_bridge::accounts)
            .unwrap_or_default();
        let notices = Notice::all(
            &self.accounts,
            &format_changed,
            &lines,
            &with_percent,
            &bridged,
        );
        let snapshot =
            live_snapshot::make(&status, &calls.minutes, now, tz, &sources, &self.accounts)
                .unwrap_or_else(|| UsageSnapshot::empty(&live_snapshot::clock(now, tz)));
        Reading {
            snapshot,
            minute: now as i64 / 60,
            history,
            notices,
        }
    }
}
