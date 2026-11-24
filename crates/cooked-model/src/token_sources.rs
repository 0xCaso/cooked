//! The data types of `Model/TokenSources.swift`. Task 5 adds `make`.

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
}
