//! What the popover's notice card says when a number is missing, and its one fix. One
//! at a time, in the order of the cases.

use crate::accounts::{ConnectedAccount, Kind};
use crate::status_line_setup::State;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// No connected account: the island says "Set up Cooked".
    NoAccounts,
    /// A connected account's newest session file has lines Cooked does not know: its tokens are not counted.
    FormatChanged { account: String, file: String },
    /// Cooked connected the status line, but it changed since: no Claude percent until it connects again.
    Paused,
    /// A Claude account with no percent, its status line not connected.
    NoPercent { id: String, account: String },
    /// The status line is connected but Claude sends no percent: an API key, which has no 5-hour limit.
    ApiKey { id: String, account: String },
}

impl Notice {
    pub fn title(&self) -> String {
        match self {
            Notice::NoAccounts => "No accounts connected".to_owned(),
            Notice::FormatChanged { .. } => {
                "Cooked cannot read Claude Code's new session files".to_owned()
            }
            Notice::Paused => "Claude percent paused".to_owned(),
            Notice::NoPercent { account, .. } => format!("No 5-hour percent for {account}"),
            Notice::ApiKey { account, .. } => format!("{account} uses an API key"),
        }
    }

    pub fn body(&self) -> &'static str {
        match self {
            Notice::NoAccounts => {
                "Cooked reads the session files of your Claude and Codex accounts on this Mac."
            }
            Notice::FormatChanged { .. } => {
                "Tokens per minute stop for this account until you update Cooked. The percents still work."
            }
            Notice::Paused => "Your status line changed, so Cooked gets no percent from Claude.",
            Notice::NoPercent { .. } => {
                "Claude sends it only to the status line. Cooked can read it there."
            }
            Notice::ApiKey { .. } => {
                "API keys have no 5-hour limit, so Cooked shows only tokens per minute."
            }
        }
    }

    /// The key that hides it: only the notices you can put off have one.
    pub fn dismiss_key(&self) -> Option<String> {
        match self {
            Notice::NoPercent { id, .. } => Some(format!("noPercent:{id}")),
            Notice::ApiKey { id, .. } => Some(format!("apiKey:{id}")),
            _ => None,
        }
    }

    /// "Copy details" for a changed format: what a bug report needs.
    pub fn details(&self) -> Option<String> {
        match self {
            Notice::FormatChanged { account, file } => Some(format!(
                "Cooked cannot read the session format of {account}.\nFile: {file}"
            )),
            _ => None,
        }
    }

    /// Every notice that applies, first the one to show. `status_line`: each Claude account's status line;
    /// `bridged`: the accounts whose status line ran (the bridge wrote their file), with or without a percent.
    pub fn all(
        accounts: &[ConnectedAccount],
        format_changed: &HashMap<String, String>,
        status_line: &HashMap<String, State>,
        with_percent: &HashSet<String>,
        bridged: &HashSet<String>,
    ) -> Vec<Notice> {
        if accounts.is_empty() {
            return vec![Notice::NoAccounts];
        }
        let mut out: Vec<Notice> = accounts
            .iter()
            .filter_map(|a| {
                format_changed.get(&a.id).map(|f| Notice::FormatChanged {
                    account: a.name.clone(),
                    file: f.clone(),
                })
            })
            .collect();
        if accounts
            .iter()
            .any(|a| status_line.get(&a.id) == Some(&State::Changed))
        {
            out.push(Notice::Paused);
        }
        let missing: Vec<&ConnectedAccount> = accounts
            .iter()
            .filter(|a| a.kind() == Kind::Claude && !with_percent.contains(&a.id))
            .collect();
        // a status line not connected comes before an API key; connected with no call yet says nothing
        out.extend(
            missing
                .iter()
                .filter(|a| {
                    status_line
                        .get(&a.id)
                        .copied()
                        .unwrap_or(State::NotConnected)
                        == State::NotConnected
                })
                .map(|a| Notice::NoPercent {
                    id: a.id.clone(),
                    account: a.name.clone(),
                }),
        );
        out.extend(
            missing
                .iter()
                .filter(|a| {
                    status_line.get(&a.id) == Some(&State::Connected) && bridged.contains(&a.id)
                })
                .map(|a| Notice::ApiKey {
                    id: a.id.clone(),
                    account: a.name.clone(),
                }),
        );
        out
    }

    /// The one to show: the first not put off.
    pub fn first<'a>(notices: &'a [Notice], dismissed: &HashSet<String>) -> Option<&'a Notice> {
        notices
            .iter()
            .find(|n| n.dismiss_key().is_none_or(|k| !dismissed.contains(&k)))
    }
}
