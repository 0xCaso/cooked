//! The Claude Code and Codex config folders Cooked reads.

use crate::defaults::DefaultsStore;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::SystemTime;

/// One Claude Code or Codex account Cooked reads: a config folder, with the short name the user gave it.
/// Ids follow T3 Code's provider instances, so the T3 Code extras still match: `~/.claude` is `claudeAgent`,
/// `~/.claude-x-y` is `claudeAgent_x_y`, `~/.codex` is `codex`.
///
/// Field order is the JSON key order earlier versions saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectedAccount {
    pub id: String,
    pub name: String,
    /// The config folder: `~/.claude-work`.
    pub path: String,
}

impl ConnectedAccount {
    pub fn new(id: impl Into<String>, name: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            path: path.into(),
        }
    }

    pub fn kind(&self) -> Kind {
        if self.id == CODEX_ID {
            Kind::Codex
        } else {
            Kind::Claude
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Claude,
    Codex,
}

pub const CODEX_ID: &str = "codex";

/// A folder found on this Mac, connected or not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub id: String,
    pub kind: Kind,
    pub path: String,
    /// The newest change in its sessions; `None` when it has none.
    pub last_used: Option<SystemTime>,
}

/// Every `~/.claude*` folder with a `projects/` folder of transcripts, and `~/.codex` with `sessions/`:
/// `~/.claude` first, the other Claude folders by name, Codex last.
pub fn scan(home: &str) -> Vec<Found> {
    let mut names = list(home);
    names.sort();
    let mut out = Vec::new();
    for name in names
        .iter()
        .filter(|n| *n == ".claude" || n.starts_with(".claude-"))
    {
        let projects = format!("{home}/{name}/projects");
        if !is_directory(&projects) {
            continue;
        }
        out.push(Found {
            id: id(name),
            kind: Kind::Claude,
            path: format!("{home}/{name}"),
            last_used: newest(&projects),
        });
    }
    let sessions = format!("{home}/.codex/sessions");
    if is_directory(&sessions) {
        out.push(Found {
            id: CODEX_ID.to_owned(),
            kind: Kind::Codex,
            path: format!("{home}/.codex"),
            last_used: newest_codex(&sessions),
        });
    }
    out
}

/// `.claude` → `claudeAgent`, `.claude-team` → `claudeAgent_team`.
pub fn id(folder: &str) -> String {
    let rest: String = folder.chars().skip(".claude".len()).collect();
    format!("claudeAgent{}", rest.replace('-', "_"))
}

/// The name an account gets until the user changes it: `.claude` → "claude", `.claude-team` → "team".
pub fn default_name(found: &Found) -> String {
    if found.kind == Kind::Codex {
        return "codex".to_owned();
    }
    let folder = Path::new(&found.path)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or_default();
    let rest: String = folder
        .chars()
        .skip(".claude".len())
        .skip_while(|c| *c == '-')
        .collect();
    if rest.is_empty() {
        "claude".to_owned()
    } else {
        rest.replace('-', " ")
    }
}

/// The names in a folder, hidden ones included; empty when it cannot be read.
fn list(dir: &str) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok()?.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default()
}

fn is_directory(path: &str) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_dir())
}

fn modified(path: &str) -> Option<SystemTime> {
    std::fs::symlink_metadata(path).ok()?.modified().ok()
}

/// A project folder changes when a session file is added in it.
fn newest(dir: &str) -> Option<SystemTime> {
    list(dir)
        .iter()
        .filter_map(|name| modified(&format!("{dir}/{name}")))
        .max()
}

/// `sessions/<year>/<month>/<day>/`: the last day folder.
fn newest_codex(dir: &str) -> Option<SystemTime> {
    let mut path = dir.to_owned();
    for _ in 0..3 {
        let Some(last) = list(&path)
            .into_iter()
            .filter(|n| !n.starts_with('.'))
            .max()
        else {
            break;
        };
        path = format!("{path}/{last}");
    }
    if path == dir {
        None
    } else {
        newest(&path).or_else(|| modified(&path))
    }
}

/// The connected accounts, in display order, in UserDefaults as JSON (the same bytes earlier versions saved).
/// The first launch connects every folder `scan` finds, with its default name.
pub struct AccountStore;

impl AccountStore {
    pub const KEY: &str = "accounts";

    pub fn load(defaults: &dyn DefaultsStore) -> Option<Vec<ConnectedAccount>> {
        Self::decode(&defaults.get_data(Self::KEY)?)
    }

    pub fn save(accounts: &[ConnectedAccount], defaults: &mut dyn DefaultsStore) {
        defaults.set_data(Self::KEY, Self::encode(accounts));
    }

    /// The saved accounts, or, the first time, every found folder.
    pub fn current(defaults: &mut dyn DefaultsStore, home: &str) -> Vec<ConnectedAccount> {
        if let Some(saved) = Self::load(defaults) {
            return saved;
        }
        let found: Vec<_> = scan(home)
            .iter()
            .map(|f| ConnectedAccount::new(f.id.clone(), default_name(f), f.path.clone()))
            .collect();
        if !found.is_empty() {
            Self::save(&found, defaults); // nothing found yet: scan again next launch
        }
        found
    }

    /// The bytes earlier versions saved: compact, keys in declaration order, `/` written as `\/`.
    pub fn encode(accounts: &[ConnectedAccount]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut ser = serde_json::Serializer::with_formatter(&mut out, SwiftFormatter);
        // Strings and a Vec into a Vec<u8>: serializing cannot fail.
        if accounts.serialize(&mut ser).is_err() {
            out.clear();
        }
        out
    }

    pub fn decode(data: &[u8]) -> Option<Vec<ConnectedAccount>> {
        serde_json::from_slice(data).ok()
    }
}

/// serde_json's compact output, plus the `\/` escape earlier versions saved.
pub struct SwiftFormatter;

impl serde_json::ser::Formatter for SwiftFormatter {
    fn write_string_fragment<W: ?Sized + std::io::Write>(
        &mut self,
        writer: &mut W,
        fragment: &str,
    ) -> std::io::Result<()> {
        for (i, part) in fragment.split('/').enumerate() {
            if i > 0 {
                writer.write_all(b"\\/")?;
            }
            writer.write_all(part.as_bytes())?;
        }
        Ok(())
    }
}
