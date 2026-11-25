//! New tokens per minute of the Claude accounts, read from their transcripts
//! (`<config dir>/projects/**/*.jsonl`). T3 Code's database writes a turn's tokens only when the turn ends, and never
//! the subagents that keep running after it; the transcripts have every call, with its time. "New" counts like the
//! watcher: input + cache writes + output, without cache reads.
//! Keeps each file's read position between readings (in the `MinuteLedger`), so a reading only parses the lines
//! added since the last one.

use crate::accounts::{ConnectedAccount, Kind};
use crate::clock::{iso_date, unix};
use crate::files::{self, contains, find};
use crate::minute_ledger::MinuteLedger;
use crate::session_format::SessionFormat;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

/// What a file's lines say besides tokens, since Cooked started.
#[derive(Debug, Clone, Default)]
struct File {
    /// The working directory of the last call: the source's project.
    cwd: Option<String>,
    /// The last titles in the lines read so far.
    titles: Titles,
}

pub struct TranscriptLog {
    /// Account id → config dir: `claudeAgent` is `~/.claude`, `claudeAgent_x_y` is `~/.claude-x-y`.
    pub dirs: HashMap<String, String>,
    files: HashMap<String, File>,
    /// Account id → the format of its newest transcript, checked once per file.
    formats: HashMap<String, HashMap<String, SessionFormat>>,
    /// Main transcript path → the titles in its first `HEAD_BYTES`, read once: the first prompt, and titles the tail
    /// lacks.
    heads: HashMap<String, Titles>,
    /// The minute cache, where the tokens go. A ledger the reader made itself is begun and ended by `read`.
    ledger: Arc<MinuteLedger>,
    owns_ledger: bool,
}

/// One Claude session in the last 15 minutes: its own calls and its subagents' calls, which run in files of their own.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub account: String,
    /// The session id, the transcript's file name; T3 Code keeps it as the thread's `nativeThreadRef.nativeId`.
    pub id: String,
    pub cwd: Option<String>,
    /// New tokens in the last 15 minutes, subagents included.
    pub tokens: f64,
    /// The session itself made a call in the last 5 minutes.
    pub main_active: bool,
    /// Subagents with a call in the last 5 minutes.
    pub active_subagents: i64,
    /// The session's name: its title in Claude Code, else its first prompt.
    pub name: Option<String>,
    /// The newest minute with a call, subagents included: an ended session shows how long ago it stopped.
    pub last_minute: Option<i64>,
    /// New tokens in the last 5 minutes (this one included), subagents included: the line's rate, on the same
    /// minutes as the popover's "now".
    pub last_five: f64,
}

impl Session {
    pub fn new(
        account: &str,
        id: &str,
        cwd: Option<String>,
        tokens: f64,
        main_active: bool,
        active_subagents: i64,
    ) -> Self {
        Self {
            account: account.to_owned(),
            id: id.to_owned(),
            cwd,
            tokens,
            main_active,
            active_subagents,
            name: None,
            last_minute: None,
            last_five: 0.0,
        }
    }
}

/// Sessions most tokens first, then by id, both descending.
pub(crate) fn sort_sessions(sessions: &mut [Session]) {
    sessions.sort_by(|a, b| b.tokens.total_cmp(&a.tokens).then_with(|| b.id.cmp(&a.id)));
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reading {
    /// Account id → minute → new tokens.
    pub minutes: HashMap<String, HashMap<i64, f64>>,
    /// Sessions with tokens in the last 15 minutes, most tokens first.
    pub sessions: Vec<Session>,
    /// Account id → its newest transcript, when Cooked does not know its lines.
    pub format_changed: HashMap<String, String>,
}

/// New tokens of a file's recent minutes, up to `minute_now`: (last 15 min, last 5 min, active in the last 5, the
/// newest minute in the last 15).
pub(crate) fn recent(recent: &HashMap<i64, f64>, minute_now: i64) -> (f64, f64, bool, Option<i64>) {
    let (mut fifteen, mut five, mut active, mut last) = (0.0, 0.0, false, None::<i64>);
    for (&minute, &tokens) in recent.iter().filter(|(m, _)| **m <= minute_now) {
        if minute > minute_now - 15 {
            fifteen += tokens;
            last = Some(last.map_or(minute, |l| l.max(minute)));
        }
        if minute > minute_now - 5 {
            active = true;
            five += tokens;
        }
    }
    (fifteen, five, active, last)
}

/// The `jsonl` files under `root` changed since `since`, with their change time.
pub(crate) fn changed_since(
    root: &Path,
    since: f64,
    keep: impl Fn(&Path) -> bool,
) -> Vec<(PathBuf, SystemTime)> {
    files::walk(root)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl") && keep(p))
        .filter_map(|p| {
            let modified = files::modified(&p)?;
            (unix(modified) >= since).then_some((p, modified))
        })
        .collect()
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

impl TranscriptLog {
    pub const HEAD_BYTES: usize = 512 * 1024;
    /// The block `start` steps back by.
    pub const START_BLOCK: u64 = 4 << 20;
    /// The minutes `read` returns by default: the island's hour.
    pub const DEFAULT_MINUTES: i64 = 60;

    /// `ledger` `None`: the reader makes its own, in memory, and begins and ends it in `read`.
    pub fn new(dirs: HashMap<String, String>, ledger: Option<Arc<MinuteLedger>>) -> Self {
        let owns_ledger = ledger.is_none();
        Self {
            dirs,
            files: HashMap::new(),
            formats: HashMap::new(),
            heads: HashMap::new(),
            ledger: ledger.unwrap_or_else(|| Arc::new(MinuteLedger::memory())),
            owns_ledger,
        }
    }

    /// The connected Claude accounts' folders.
    pub fn with_accounts(accounts: &[ConnectedAccount], ledger: Option<Arc<MinuteLedger>>) -> Self {
        let mut dirs = HashMap::new();
        for a in accounts.iter().filter(|a| a.kind() == Kind::Claude) {
            dirs.entry(a.id.clone()).or_insert_with(|| a.path.clone());
        }
        Self::new(dirs, ledger)
    }

    /// The minute cache this reader adds to.
    pub fn ledger(&self) -> Arc<MinuteLedger> {
        Arc::clone(&self.ledger)
    }

    pub fn dir(id: &str, home: &str) -> String {
        let suffix: String = id.chars().skip("claudeAgent".len()).collect();
        format!("{home}/.claude{}", suffix.replace('_', "-"))
    }

    /// `<projects>/<dir>/<session>.jsonl` → (session, false); `<projects>/<dir>/<session>/subagents/agent-x.jsonl` →
    /// (session, true).
    pub fn session_of(path: &Path) -> (String, bool) {
        let name = |p: Option<&Path>| {
            p.and_then(Path::file_name)
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let parent = path.parent();
        if parent
            .and_then(Path::file_name)
            .is_some_and(|n| n == "subagents")
        {
            return (name(parent.and_then(Path::parent)), true);
        }
        let stem = path
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        (stem, false)
    }

    /// Account id → minute → new tokens, for the `minutes` minutes up to `now` (from the minute cache), and the
    /// sessions of the last 15 minutes. Accounts without a transcript directory are left out, so the caller keeps its
    /// other source for them. Walks the files written in the last 14 days, newest first.
    pub fn read(&mut self, now: f64, minutes: i64) -> Reading {
        let ledger = Arc::clone(&self.ledger);
        if self.owns_ledger {
            ledger.begin();
        }
        let out = self.read_files(now, minutes, &ledger);
        if self.owns_ledger {
            ledger.end(now);
        }
        out
    }

    fn read_files(&mut self, now: f64, minutes: i64, ledger: &MinuteLedger) -> Reading {
        let minute_now = now as i64 / 60;
        let since = MinuteLedger::since(now);
        let mut out = Reading::default();
        let mut sessions: HashMap<String, Session> = HashMap::new();
        let mut main_paths: HashMap<String, String> = HashMap::new();
        let mut seen: HashSet<String> = HashSet::new();
        let dirs: Vec<(String, String)> = self
            .dirs
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (id, dir) in &dirs {
            let root = Path::new(dir).join("projects");
            if !root.is_dir() {
                continue;
            }
            let mut urls = changed_since(&root, since, |_| true);
            let cache = self.formats.entry(id.clone()).or_default();
            if let Some(path) = SessionFormat::newest_changed(&urls, cache) {
                out.format_changed.insert(id.clone(), path);
            }
            urls.sort_by_key(|u| std::cmp::Reverse(u.1));
            for (url, _) in &urls {
                let key = url.to_string_lossy().into_owned();
                seen.insert(key.clone());
                let mut file = self.files.remove(&key).unwrap_or_default();
                let mark = ledger.read(url, now, |line, mark| {
                    if let Some(call) = Self::parse(line) {
                        if let Some(cwd) = &call.cwd {
                            file.cwd = Some(cwd.clone());
                        }
                        // a message is written once per content block, with the same usage; `/compact` copies
                        // older calls again
                        if mark.keys.contains_key(&call.id)
                            || call.minute < mark.newest.unwrap_or(i64::MIN)
                        {
                            return;
                        }
                        mark.keys.insert(call.id.clone(), call.minute);
                        mark.newest = Some(mark.newest.map_or(call.minute, |n| n.max(call.minute)));
                        *mark.recent.entry(call.minute).or_insert(0.0) += call.tokens;
                        ledger.add(id, call.minute, call.tokens);
                    } else {
                        Self::title(line, &mut file.titles);
                    }
                });
                let file_cwd = file.cwd.clone();
                self.files.insert(key.clone(), file);
                let (fifteen, five, active, last) = recent(&mark.recent, minute_now);
                if fifteen > 0.0 {
                    let (sid, is_sub) = Self::session_of(url);
                    let main = if is_sub {
                        let session_dir = url.parent().and_then(Path::parent).unwrap_or(url);
                        format!("{}.jsonl", session_dir.to_string_lossy())
                    } else {
                        key.clone()
                    };
                    main_paths.insert(sid.clone(), main);
                    let s = sessions
                        .entry(sid.clone())
                        .or_insert_with(|| Session::new(id, &sid, None, 0.0, false, 0));
                    s.tokens += fifteen;
                    s.last_five += five;
                    if let Some(last) = last {
                        s.last_minute = Some(s.last_minute.map_or(last, |l| l.max(last)));
                    }
                    if is_sub {
                        s.active_subagents += i64::from(active);
                    } else {
                        s.main_active = active;
                    }
                    // a subagent may run in another folder: the session's own cwd wins
                    if !is_sub || s.cwd.is_none() {
                        s.cwd = file_cwd.or(s.cwd.take());
                    }
                }
            }
            out.minutes.insert(
                id.clone(),
                ledger.minutes(id, minute_now - minutes + 1, minute_now),
            );
        }
        self.files.retain(|k, _| seen.contains(k));
        for (sid, path) in &main_paths {
            let head = match self.heads.get(path) {
                Some(h) => h.clone(),
                None => Self::head_titles(Path::new(path)),
            };
            self.heads.insert(path.clone(), head.clone());
            if let Some(s) = sessions.get_mut(sid) {
                let later = self
                    .files
                    .get(path)
                    .map(|f| f.titles.clone())
                    .unwrap_or_default();
                s.name = head.name(&later);
                // the cwd of a session resumed after a restart, before its first new call
                if s.cwd.is_none() {
                    s.cwd = head.cwd.clone();
                }
            }
        }
        let mains: HashSet<&String> = main_paths.values().collect();
        self.heads.retain(|k, _| mains.contains(k));
        out.sessions = sessions.into_values().collect();
        sort_sessions(&mut out.sessions);
        out
    }

    /// Where to start reading a file seen for the first time: transcripts grow to hundreds of MB, so step back from
    /// the end in blocks until every time in a block is before `since`, and start at that block's last line.
    /// Calls are in time order, but `/compact` copies older lines after newer ones. Returns a line start.
    pub fn start(path: &Path, since: f64, block: u64) -> u64 {
        let Some(end) = files::len(path) else {
            return 0;
        };
        let key = br#""timestamp":""#;
        let mut start = end;
        while start > 0 {
            start = start.saturating_sub(block);
            let Some(chunk) = files::read_at(path, start, block) else {
                return 0;
            };
            let mut newest: Option<f64> = None;
            let mut from = 0;
            while let Some(hit) = find(&chunk, key, from) {
                let up = hit + key.len();
                from = up;
                let Some(at) = chunk
                    .get(up..up + 24)
                    .and_then(|b| std::str::from_utf8(b).ok())
                    .and_then(iso_date)
                else {
                    continue;
                };
                newest = Some(newest.map_or(at, |n| n.max(at)));
            }
            if newest.is_some_and(|n| n < since) {
                let Some(newline) = chunk.iter().rposition(|b| *b == b'\n') else {
                    return 0;
                };
                return start + newline as u64 + 1;
            }
        }
        0
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
