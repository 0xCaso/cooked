//! Part of `Model/TranscriptLog.swift`: the line parsers and session names. Task 5 adds the reader (`read`, the
//! minute ledger, sessions).

use crate::accounts::{ConnectedAccount, Kind};
use crate::clock::iso_date;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

/// New tokens per minute of the Claude accounts, read from their transcripts (`<config dir>/projects/**/*.jsonl`).
#[derive(Debug, Clone, Default)]
pub struct TranscriptLog {
    /// Account id → config dir: `claudeAgent` is `~/.claude`, `claudeAgent_x_y` is `~/.claude-x-y`.
    pub dirs: HashMap<String, String>,
}

/// A session's names, from its own lines: the user's (`/rename`), Claude Code's, and the first prompt.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Titles {
    pub custom: Option<String>,
    pub ai: Option<String>,
    pub first_prompt: Option<String>,
    /// The first working directory in the head: the folder of a session with no new call yet.
    pub cwd: Option<String>,
}

impl Titles {
    /// `custom-title`, else `ai-title`, else the first prompt; `later` (newer lines) wins over `self`.
    pub fn name(&self, later: &Titles) -> Option<String> {
        later
            .custom
            .clone()
            .or_else(|| self.custom.clone())
            .or_else(|| later.ai.clone())
            .or_else(|| self.ai.clone())
            .or_else(|| self.first_prompt.clone())
    }
}

/// One assistant call in a transcript line.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub id: String,
    /// Unix minute.
    pub minute: i64,
    pub tokens: f64,
    pub cwd: Option<String>,
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: Option<f64>,
    cache_creation_input_tokens: Option<f64>,
    output_tokens: Option<f64>,
}

#[derive(Deserialize)]
struct Message {
    id: Option<String>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type")]
    kind: Option<String>,
    timestamp: Option<String>,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    cwd: Option<String>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Block {
    #[serde(rename = "type")]
    kind: Option<String>,
    text: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Content {
    Text(String),
    Blocks(Vec<Block>),
}

#[derive(Deserialize)]
struct TitleMessage {
    content: Option<Content>,
}

#[derive(Deserialize)]
struct TitleLine {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(rename = "customTitle")]
    custom_title: Option<String>,
    #[serde(rename = "aiTitle")]
    ai_title: Option<String>,
    #[serde(rename = "isMeta")]
    is_meta: Option<bool>,
    cwd: Option<String>,
    message: Option<TitleMessage>,
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

impl TranscriptLog {
    pub const HEAD_BYTES: usize = 512 * 1024;

    pub fn new(dirs: HashMap<String, String>) -> Self {
        Self { dirs }
    }

    /// The connected Claude accounts' folders.
    pub fn with_accounts(accounts: &[ConnectedAccount]) -> Self {
        let mut dirs = HashMap::new();
        for a in accounts.iter().filter(|a| a.kind() == Kind::Claude) {
            dirs.entry(a.id.clone()).or_insert_with(|| a.path.clone());
        }
        Self { dirs }
    }

    pub fn dir(id: &str, home: &str) -> String {
        let suffix: String = id.chars().skip("claudeAgent".len()).collect();
        format!("{home}/.claude{}", suffix.replace('_', "-"))
    }

    /// Takes a title or the first prompt from a line that is not a call.
    pub fn title(line: &[u8], titles: &mut Titles) {
        let needs_prompt = titles.first_prompt.is_none();
        if !(contains(line, br#""custom-title""#)
            || contains(line, br#""ai-title""#)
            || (needs_prompt && contains(line, br#""type":"user""#)))
        {
            return;
        }
        let Ok(l) = serde_json::from_slice::<TitleLine>(line) else {
            return;
        };
        match l.kind.as_deref() {
            Some("custom-title") => {
                if let Some(t) = Self::clean(l.custom_title.as_deref()) {
                    titles.custom = Some(t);
                }
            }
            Some("ai-title") => {
                if let Some(t) = Self::clean(l.ai_title.as_deref()) {
                    titles.ai = Some(t);
                }
            }
            Some("user") if needs_prompt && l.is_meta != Some(true) => {
                if titles.cwd.is_none() {
                    titles.cwd = l.cwd;
                }
                let text = match l.message.and_then(|m| m.content) {
                    Some(Content::Text(s)) => Some(s),
                    Some(Content::Blocks(blocks)) => blocks
                        .into_iter()
                        .find(|b| b.kind.as_deref() == Some("text"))
                        .and_then(|b| b.text),
                    None => None,
                };
                // command output and injected context start with a tag; tool results have no text block
                if let Some(text) = text.filter(|t| !t.starts_with('<')) {
                    titles.first_prompt = Self::clean(Some(&text));
                }
            }
            _ => {}
        }
    }

    /// The first non-empty line, at most 80 characters.
    pub fn clean(s: Option<&str>) -> Option<String> {
        let is_newline = |c: char| {
            matches!(
                c,
                '\n' | '\r' | '\u{0B}' | '\u{0C}' | '\u{85}' | '\u{2028}' | '\u{2029}'
            )
        };
        let line = s?
            .split(is_newline)
            .map(|l| l.trim_matches(|c: char| c.is_whitespace() && !is_newline(c)))
            .find(|l| !l.is_empty())?;
        if line.chars().count() > 80 {
            Some(line.chars().take(79).chain(['…']).collect())
        } else {
            Some(line.to_owned())
        }
    }

    /// The titles in the first `HEAD_BYTES` of a transcript.
    pub fn head_titles(path: &Path) -> Titles {
        let mut titles = Titles::default();
        let Ok(file) = std::fs::File::open(path) else {
            return titles;
        };
        let mut data = Vec::new();
        if file
            .take(Self::HEAD_BYTES as u64)
            .read_to_end(&mut data)
            .is_err()
        {
            return titles;
        }
        for line in data.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
            Self::title(line, &mut titles);
        }
        titles
    }

    /// One transcript line → the call, or `None` when it is not an assistant call.
    pub fn parse(line: &[u8]) -> Option<Call> {
        // most lines are tool output: skip them before decoding
        if !contains(line, br#""usage""#) {
            return None;
        }
        let l: Line = serde_json::from_slice(line).ok()?;
        if l.kind.as_deref() != Some("assistant") {
            return None;
        }
        let message = l.message?;
        let u = message.usage?;
        let stamp = l.timestamp?;
        let at = iso_date(&stamp)?;
        let tokens = u.input_tokens.unwrap_or(0.0)
            + u.cache_creation_input_tokens.unwrap_or(0.0)
            + u.output_tokens.unwrap_or(0.0);
        let id = message.id.or(l.request_id).unwrap_or(stamp);
        Some(Call {
            id,
            minute: (at as i64) / 60,
            tokens,
            cwd: l.cwd,
        })
    }
}
