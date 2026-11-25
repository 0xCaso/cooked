//! The lines of "What burns tokens", and where a click on one goes.

use crate::accounts::{self, CODEX_ID, ConnectedAccount};
use crate::live_snapshot;
use crate::token_log::Thread;
use crate::transcript_log::Session;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::Path;

/// One line of "What burns tokens": a project and the T3 Code thread working in it, over the last 5 minutes.
#[derive(Debug, Clone, PartialEq)]
pub struct TokenSource {
    /// The T3 thread id when known, else the Claude session id.
    pub id: String,
    /// The T3 Code thread, when the source runs in one: a click opens it.
    pub thread_id: Option<String>,
    pub project: String,
    /// The thread's title: "Draft the release notes".
    pub detail: String,
    /// The account's short name: "team".
    pub account: String,
    /// New tokens per minute over the last 5 minutes, like the popover's "now"; an ended line, over the last 15.
    pub rate: f64,
    /// "1 agent + 3 sub", "3 sub"; `None` when nothing ran in the last 5 minutes.
    pub agents: Option<String>,
    /// Minutes since the last call, when nothing ran in the last 5 minutes: the line goes under "Ended".
    pub ended_ago: Option<i64>,
    /// Where a click goes when the source has no T3 Code thread.
    pub session: Option<OpenTarget>,
}

impl TokenSource {
    /// The T3 Code thread first, else the session itself.
    pub fn target(&self) -> Option<OpenTarget> {
        self.thread_id
            .as_ref()
            .map(|t| OpenTarget::t3_code(t))
            .or_else(|| self.session.clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    T3Code,
    Codex,
    Claude,
}

/// Where a click on a row goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenTarget {
    pub place: Place,
    /// The T3 Code thread, or the session id.
    pub id: String,
    pub cwd: Option<String>,
    /// A Claude account's folder; `None` for `~/.claude`, the only account the Claude app can open.
    pub config_dir: Option<String>,
}

impl OpenTarget {
    pub fn t3_code(thread: &str) -> Self {
        Self {
            place: Place::T3Code,
            id: thread.to_owned(),
            cwd: None,
            config_dir: None,
        }
    }

    pub fn codex(session: &str, cwd: Option<String>) -> Self {
        Self {
            place: Place::Codex,
            id: session.to_owned(),
            cwd,
            config_dir: None,
        }
    }

    pub fn claude(session: &str, cwd: Option<String>, config_dir: Option<String>) -> Self {
        Self {
            place: Place::Claude,
            id: session.to_owned(),
            cwd,
            config_dir,
        }
    }

    /// The session behind a transcript: Codex, `~/.claude`, or another Claude account with its folder.
    pub fn session(s: &Session, accounts: &[ConnectedAccount]) -> Self {
        if s.account == CODEX_ID {
            return Self::codex(&s.id, s.cwd.clone());
        }
        let dir = if s.account == accounts::id(".claude") {
            None
        } else {
            accounts
                .iter()
                .find(|a| a.id == s.account)
                .map(|a| a.path.clone())
        };
        Self::claude(&s.id, s.cwd.clone(), dir)
    }

    /// The command that resumes the session in a Terminal.
    pub fn command(&self) -> String {
        match self.place {
            Place::T3Code => format!(
                "open {}",
                Self::quote(&format!("t3code://threads/{}", self.id))
            ),
            Place::Codex => format!("codex resume {}", Self::quote(&self.id)),
            Place::Claude => format!(
                "{}claude --resume {}",
                self.config_dir
                    .as_ref()
                    .map(|d| format!("CLAUDE_CONFIG_DIR={} ", Self::quote(d)))
                    .unwrap_or_default(),
                Self::quote(&self.id)
            ),
        }
    }

    /// The `.command` file: into the project folder, then the command.
    pub fn script(&self) -> String {
        let cd = self
            .cwd
            .as_ref()
            .map(|c| format!("cd {} || exit 1\n", Self::quote(c)))
            .unwrap_or_default();
        format!("#!/bin/sh\n{cd}{}\n", self.command())
    }

    /// One shell word, in single quotes.
    pub fn quote(s: &str) -> String {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// The full list, when the card is open; it shows `SourcesCard.collapsed` lines until then.
pub const LIMIT: usize = 8;
/// An ended line's rate: it made no call in the last 5 minutes, so its last 15.
pub const WINDOW: f64 = 15.0;
/// Lines under this many tokens per minute are left out: a source of 0/min says nothing.
pub const MINIMUM_RATE: f64 = 1.0;

/// The top sources: the sessions of the session files, named by their T3 Code thread when they have one. `now` in
/// Unix seconds.
pub fn make(
    sessions: &[Session],
    threads: &HashMap<String, Thread>,
    accounts: &[ConnectedAccount],
    now: Option<f64>,
) -> Vec<TokenSource> {
    let minute_now = now.map(|n| n as i64 / 60);
    // the same divisor as `LiveSnapshot`'s rate: 4 whole minutes and the part of this one gone by
    let gone = now.map_or(5.0, |n| 4.0 + (n % 60.0) / 60.0);
    let ended = |s: &Session| -> Option<i64> {
        if s.main_active || s.active_subagents != 0 {
            return None;
        }
        Some((minute_now? - s.last_minute?).max(0))
    };
    let out: Vec<TokenSource> = sessions
        .iter()
        .map(|s| {
            let thread = threads.get(&s.id);
            let ended_ago = ended(s);
            let fallback = if s.account == CODEX_ID {
                "Codex session"
            } else {
                "Claude Code session"
            };
            TokenSource {
                id: thread.map_or_else(|| s.id.clone(), |t| t.id.clone()),
                thread_id: thread.map(|t| t.id.clone()),
                project: thread.map_or_else(|| project_of_cwd(s.cwd.as_deref()), project),
                detail: thread
                    .map(|t| t.title.clone())
                    .or_else(|| s.name.clone())
                    .unwrap_or_else(|| fallback.to_owned()),
                account: live_snapshot::short(&s.account, accounts),
                rate: if ended_ago.is_none() {
                    s.last_five / gone
                } else {
                    s.tokens / WINDOW
                },
                agents: agents(s.main_active, s.active_subagents),
                ended_ago,
                session: Some(OpenTarget::session(s, accounts)),
            }
        })
        .collect();
    // one T3 thread can hold several sessions (a resumed or handed-off conversation): one line
    let mut merged: HashMap<String, TokenSource> = HashMap::new();
    for s in out {
        match merged.get_mut(&s.id) {
            Some(m) => {
                m.rate += s.rate;
                if m.agents.is_none() {
                    m.agents = s.agents;
                }
                m.ended_ago = match (m.ended_ago, s.ended_ago) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    _ => None,
                };
            }
            None => {
                merged.insert(s.id.clone(), s);
            }
        }
    }
    let mut list: Vec<TokenSource> = merged
        .into_values()
        .filter(|s| s.rate >= MINIMUM_RATE)
        .collect();
    // live lines by rate, then the ended ones, the newest first
    list.sort_by(|a, b| match (a.ended_ago, b.ended_ago) {
        (None, None) => b.rate.total_cmp(&a.rate).then_with(|| b.id.cmp(&a.id)),
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(x), Some(y)) => x
            .cmp(&y)
            .then_with(|| b.rate.total_cmp(&a.rate))
            .then_with(|| a.id.cmp(&b.id)),
    });
    list.truncate(LIMIT);
    list
}

pub fn agents(main: bool, subagents: i64) -> Option<String> {
    match (main, subagents) {
        (false, 0) => None,
        (true, 0) => Some("1 agent".to_owned()),
        (true, n) => Some(format!("1 agent + {n} sub")),
        (false, n) => Some(format!("{n} sub")),
    }
}

pub fn project(thread: &Thread) -> String {
    thread
        .project
        .clone()
        .unwrap_or_else(|| "t3 scratch".to_owned())
}

/// The folder's name; T3 Code's scratch threads, one folder each, are all "t3 scratch".
pub fn project_of_cwd(cwd: Option<&str>) -> String {
    let Some(cwd) = cwd.filter(|c| !c.is_empty()) else {
        return "unknown".to_owned();
    };
    if cwd.contains("/.t3/scratch") {
        return "t3 scratch".to_owned();
    }
    Path::new(cwd)
        .file_name()
        .map_or_else(|| cwd.to_owned(), |n| n.to_string_lossy().into_owned())
}
