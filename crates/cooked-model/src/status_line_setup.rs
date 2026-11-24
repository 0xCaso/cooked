//! Port of `Model/StatusLineSetup.swift`: connects Claude Code's status line to Cooked's bridge, and gives it back
//! (release spec, section 2). Cooked changes only the `statusLine` key of the real `settings.json`, after a click:
//! every other byte of the file stays as it was. The bridge saves the JSON Claude Code sends for the Claude percent
//! (`status_line_bridge` reads it), then runs the status line the user had, so it looks the same.

use crate::files;
use serde::Deserialize;
use serde_json::Value;
use std::fmt;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum State {
    /// Cooked never connected this file, or gave its status line back.
    NotConnected,
    Connected,
    /// Cooked connected it, but the key changed since: the Claude percent is paused until the user connects again.
    Changed,
}

#[derive(Debug)]
pub enum Failure {
    /// `settings.json` is not a JSON object: Cooked does not write it.
    NotAnObject,
    Io(std::io::Error),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::NotAnObject => write!(f, "settings.json is not a JSON object"),
            Failure::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Failure {}

impl From<std::io::Error> for Failure {
    fn from(e: std::io::Error) -> Self {
        Failure::Io(e)
    }
}

/// What Cooked keeps for a settings file: its real path and the `statusLine` value it had, as text, exactly.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Saved {
    pub settings: String,
    /// `None`: the file had no status line.
    #[serde(default)]
    pub previous: Option<String>,
}

impl Saved {
    /// The bytes Swift's `JSONEncoder` writes with `.prettyPrinted, .sortedKeys, .withoutEscapingSlashes`.
    pub fn encode(&self) -> String {
        let mut fields = Vec::new();
        if let Some(previous) = &self.previous {
            fields.push(format!("  \"previous\" : {}", json_string(previous)));
        }
        fields.push(format!("  \"settings\" : {}", json_string(&self.settings)));
        format!("{{\n{}\n}}", fields.join(",\n"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusLineSetup {
    /// Cooked's folder: the bridge script, the saved status lines and the bridge files.
    pub support: String,
}

impl Default for StatusLineSetup {
    /// `~/Library/Application Support/Cooked`. Without `$HOME`, a folder that cannot exist: nothing is written.
    fn default() -> Self {
        Self::new(Self::default_support().unwrap_or_else(|| "/nonexistent/Cooked".to_owned()))
    }
}

impl StatusLineSetup {
    pub fn new(support: impl Into<String>) -> Self {
        Self {
            support: support.into(),
        }
    }

    pub fn default_support() -> Option<String> {
        Some(
            files::home()?
                .join("Library/Application Support/Cooked")
                .to_string_lossy()
                .into_owned(),
        )
    }

    pub fn script(&self) -> String {
        format!("{}/statusline-bridge.sh", self.support)
    }

    pub fn bridge_dir(&self) -> String {
        format!("{}/bridge", self.support)
    }

    /// Per real settings file: the status line it had (`<key>.json`) and that line's command (`<key>.command`).
    pub fn saved_dir(&self) -> String {
        format!("{}/statusline", self.support)
    }

    /// The file Claude Code reads, through the symlinks: several accounts can share one `settings.json`. A file that
    /// does not exist yet resolves through its folder.
    pub fn real(settings: &str) -> String {
        let path = Path::new(settings);
        fs::canonicalize(path)
            .ok()
            .or_else(|| {
                let name = path.file_name()?;
                Some(fs::canonicalize(path.parent()?).ok()?.join(name))
            })
            .map_or_else(|| settings.to_owned(), |p| p.to_string_lossy().into_owned())
    }

    /// A short name for a real settings file, stable across launches (FNV-1a of its path).
    pub fn key(real: &str) -> String {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in real.bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
        }
        format!("{hash:016x}")
    }

    /// The command Claude Code runs once connected: the bridge, with the key of the line to run after it.
    pub fn command(&self, key: &str) -> String {
        format!("sh {} {key}", Self::quote(&self.script()))
    }

    pub fn quote(s: &str) -> String {
        format!("'{}'", s.replace('\'', r"'\''"))
    }

    // MARK: - State

    pub fn state(&self, settings: &str) -> State {
        let real = Self::real(settings);
        let key = Self::key(&real);
        if self.saved(&key).is_none() {
            return State::NotConnected;
        }
        if self.bridged(&real, &key) {
            State::Connected
        } else {
            State::Changed
        }
    }

    fn saved_file(&self, key: &str, ext: &str) -> String {
        format!("{}/{key}.{ext}", self.saved_dir())
    }

    fn saved(&self, key: &str) -> Option<Saved> {
        serde_json::from_slice(&fs::read(self.saved_file(key, "json")).ok()?).ok()
    }

    /// The file's `statusLine` runs this bridge.
    fn bridged(&self, real: &str, key: &str) -> bool {
        status_line_command(real).is_some_and(|c| c == self.command(key))
    }

    // MARK: - Connect

    /// Points the file's status line to the bridge. Copies the file once to `settings.json.cooked-backup`, saves the
    /// old `statusLine` object as it is, keeps `padding` and `refreshInterval`, and writes through a temporary file.
    pub fn connect(&self, settings: &str) -> Result<(), Failure> {
        let real = Self::real(settings);
        let key = Self::key(&real);
        let exists = Path::new(&real).exists();
        let text = if exists {
            fs::read_to_string(&real)?
        } else {
            "{}\n".to_owned()
        };
        let mut object = JsonObjectText::new(text).ok_or(Failure::NotAnObject)?;
        if self.bridged(&real, &key) {
            return Ok(());
        }

        let backup = format!("{real}.cooked-backup");
        if exists && !Path::new(&backup).exists() {
            fs::copy(&real, &backup)?;
        }
        self.install_script()?;
        let previous = object.member("statusLine").map(|m| m.value);
        self.save(
            &Saved {
                settings: real.clone(),
                previous: previous.clone(),
            },
            &key,
        )?;

        let mut fields = vec![
            ("type".to_owned(), r#""command""#.to_owned()),
            ("command".to_owned(), json_string(&self.command(&key))),
        ];
        if let Some(old) = previous.and_then(JsonObjectText::new) {
            for name in ["padding", "refreshInterval"] {
                if let Some(m) = old.member(name) {
                    fields.push((name.to_owned(), m.value));
                }
            }
        }
        object.set_object("statusLine", &fields);
        Self::write(object.text(), &real)?;
        Ok(())
    }

    fn save(&self, saved: &Saved, key: &str) -> Result<(), Failure> {
        fs::create_dir_all(self.saved_dir())?;
        files::write_atomic(
            Path::new(&self.saved_file(key, "json")),
            saved.encode().as_bytes(),
        )?;
        let command_file = self.saved_file(key, "command");
        let command = saved
            .previous
            .as_deref()
            .and_then(|p| serde_json::from_str::<Value>(p).ok())
            .filter(|old| old.get("type").and_then(Value::as_str).unwrap_or("command") == "command")
            .and_then(|old| old.get("command")?.as_str().map(str::to_owned));
        match command {
            Some(command) => files::write_atomic(Path::new(&command_file), command.as_bytes())?,
            None => {
                let _ = fs::remove_file(&command_file);
            }
        }
        Ok(())
    }

    pub fn install_script(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.support)?;
        files::write_atomic(Path::new(&self.script()), BRIDGE_SCRIPT.as_bytes())
    }

    // MARK: - Disconnect

    /// Gives the file its old status line back, or removes the key if it had none. When the key changed since Cooked
    /// connected (the user or another tool), the file is not touched. Returns whether the file was written.
    pub fn disconnect(&self, settings: &str) -> Result<bool, Failure> {
        let real = Self::real(settings);
        let key = Self::key(&real);
        let Some(saved) = self.saved(&key) else {
            return Ok(false);
        };
        let out = self.give_back(&real, &key, &saved);
        let _ = fs::remove_file(self.saved_file(&key, "json"));
        let _ = fs::remove_file(self.saved_file(&key, "command"));
        out
    }

    fn give_back(&self, real: &str, key: &str, saved: &Saved) -> Result<bool, Failure> {
        if !self.bridged(real, key) {
            return Ok(false);
        }
        let Some(mut object) = JsonObjectText::new(fs::read_to_string(real)?) else {
            return Ok(false);
        };
        match &saved.previous {
            Some(previous) => object.set("statusLine", previous),
            None => object.remove("statusLine"),
        }
        Self::write(object.text(), real)?;
        Ok(true)
    }

    // MARK: - Files

    /// Writes a temporary file next to `path`, then renames it: Claude Code never reads a half-written file. The file
    /// keeps its permissions.
    pub fn write(text: &str, path: &str) -> std::io::Result<()> {
        let tmp = format!("{path}.cooked-tmp-{}", std::process::id());
        let permissions = fs::metadata(path).ok().map(|m| m.permissions());
        let written = fs::write(&tmp, text).and_then(|()| match permissions {
            Some(p) => fs::set_permissions(&tmp, p),
            None => Ok(()),
        });
        let out = written.and_then(|()| fs::rename(&tmp, path));
        if out.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        out
    }
}

/// The `command` of the file's `statusLine` object, if any.
fn status_line_command(real: &str) -> Option<String> {
    let text = fs::read_to_string(real).ok()?;
    let member = JsonObjectText::new(text)?.member("statusLine")?;
    let value: Value = serde_json::from_str(&member.value).ok()?;
    value.get("command")?.as_str().map(str::to_owned)
}

/// A JSON string literal, `/` not escaped (Swift's `JSONEncoder` with `.withoutEscapingSlashes`).
pub fn json_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_owned())
}

/// The bridge. `sh` only, no other tools than `tr`, `sed` and `mv`; it never waits for Cooked, so the status line
/// works when Cooked is closed. The account comes from `transcript_path`, like `accounts::id`.
pub const BRIDGE_SCRIPT: &str = r##"#!/bin/sh
# Cooked's status line bridge (https://github.com/0xCaso/cooked).
# Saves the JSON Claude Code gives the status line, for Cooked's Claude percent, then runs the status line
# you had before with the same JSON and prints its output. It never waits for Cooked.
here=$(cd "$(dirname "$0")" && pwd)
input=$(cat)
path=$(printf '%s' "$input" | tr -d '\n' | sed -n 's/.*"transcript_path"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
config=${path%/projects/*}
if [ -n "$path" ] && [ "$config" != "$path" ]; then
  folder=${config##*/}
  case "$folder" in
    .claude|.claude-*)
      id="claudeAgent$(printf '%s' "${folder#.claude}" | tr '-' '_')"
      mkdir -p "$here/bridge" 2>/dev/null
      tmp="$here/bridge/.$id.$$"
      printf '%s' "$input" > "$tmp" 2>/dev/null && mv -f "$tmp" "$here/bridge/$id.json" 2>/dev/null
      ;;
  esac
fi
previous="$here/statusline/$1.command"
if [ -n "$1" ] && [ -f "$previous" ]; then
  printf '%s' "$input" | sh -c "$(cat "$previous")"
fi
"##;

/// One top-level member of a JSON object's text. Byte offsets into the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub key: String,
    /// From the key's opening quote to the value's end.
    pub start: usize,
    pub end: usize,
    pub value_start: usize,
    pub value: String,
}

/// The text of a JSON object, edited one top-level member at a time: every other byte stays as it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonObjectText {
    text: String,
}

const SPACE: [u8; 4] = *b" \t\n\r";

impl JsonObjectText {
    /// `None` when `text` is not a JSON object.
    pub fn new(text: impl Into<String>) -> Option<Self> {
        let text = text.into();
        serde_json::from_str::<Value>(&text)
            .ok()?
            .is_object()
            .then_some(Self { text })
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// The positions of the braces, and the top-level members in order.
    fn scan(&self) -> (usize, usize, Vec<Member>) {
        let b = self.text.as_bytes();
        let n = b.len();
        let at = |i: usize| b.get(i).copied().unwrap_or(0);
        let mut i = 0;
        let skip_space = |i: &mut usize| {
            while *i < n && SPACE.contains(&b[*i]) {
                *i += 1;
            }
        };
        let skip_string = |i: &mut usize| {
            *i += 1;
            while *i < n && b[*i] != b'"' {
                *i += if b[*i] == b'\\' { 2 } else { 1 };
            }
            *i += 1;
        };
        let skip_value = |i: &mut usize| match at(*i) {
            b'"' => skip_string(i),
            b'{' | b'[' => {
                let mut depth = 0i64;
                loop {
                    match at(*i) {
                        b'"' => skip_string(i),
                        c => {
                            match c {
                                b'{' | b'[' => depth += 1,
                                b'}' | b']' => depth -= 1,
                                _ => {}
                            }
                            *i += 1;
                        }
                    }
                    if !(depth > 0 && *i < n) {
                        break;
                    }
                }
            }
            _ => {
                while *i < n && !matches!(b[*i], b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r') {
                    *i += 1;
                }
            }
        };
        skip_space(&mut i);
        let open = i;
        i += 1;
        let mut members = Vec::new();
        loop {
            skip_space(&mut i);
            if i >= n || at(i) == b'}' {
                break;
            }
            let key_start = i;
            skip_string(&mut i);
            let key = b
                .get(key_start..i.min(n))
                .and_then(|k| serde_json::from_slice::<String>(k).ok())
                .unwrap_or_default();
            skip_space(&mut i);
            i += 1; // :
            skip_space(&mut i);
            let value_start = i;
            skip_value(&mut i);
            let end = i.min(n);
            members.push(Member {
                key,
                start: key_start,
                end,
                value_start,
                value: String::from_utf8_lossy(b.get(value_start..end).unwrap_or_default())
                    .into_owned(),
            });
            skip_space(&mut i);
            if at(i) == b',' {
                i += 1;
            }
        }
        (open, i.min(n), members)
    }

    pub fn member(&self, key: &str) -> Option<Member> {
        self.scan().2.into_iter().rev().find(|m| m.key == key)
    }

    fn replace(&mut self, from: usize, to: usize, new: &str) {
        let b = self.text.as_bytes();
        let mut out = Vec::with_capacity(b.len() + new.len());
        out.extend_from_slice(b.get(..from).unwrap_or_default());
        out.extend_from_slice(new.as_bytes());
        out.extend_from_slice(b.get(to..).unwrap_or_default());
        self.text = String::from_utf8_lossy(&out).into_owned();
    }

    /// The spaces before a member's key on its line: the file's own indent.
    fn indent(&self, m: &Member) -> String {
        let b = self.text.as_bytes();
        let mut i = m.start;
        while i > 0 && matches!(b.get(i - 1), Some(b' ' | b'\t')) {
            i -= 1;
        }
        String::from_utf8_lossy(b.get(i..m.start).unwrap_or_default()).into_owned()
    }

    /// Sets a member's value text; a new member goes last.
    pub fn set(&mut self, key: &str, value: &str) {
        let (open, close, members) = self.scan();
        if let Some(m) = members.iter().rev().find(|m| m.key == key) {
            self.replace(m.value_start, m.end, value);
        } else if let Some(last) = members.last() {
            let text = format!(",\n{}{}: {value}", self.indent(last), json_string(key));
            self.replace(last.end, last.end, &text);
        } else {
            self.replace(
                open + 1,
                close,
                &format!("\n  {}: {value}\n", json_string(key)),
            );
        }
    }

    /// Sets a member to an object, one field per line, indented like the file.
    pub fn set_object(&mut self, key: &str, fields: &[(String, String)]) {
        let members = self.scan().2;
        let unit = members
            .first()
            .map(|m| self.indent(m))
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| "  ".to_owned());
        let outer = members
            .iter()
            .rev()
            .find(|m| m.key == key)
            .or(members.last())
            .map_or_else(|| unit.clone(), |m| self.indent(m));
        let body: Vec<String> = fields
            .iter()
            .map(|(name, value)| format!("{outer}{unit}{}: {value}", json_string(name)))
            .collect();
        self.set(key, &format!("{{\n{}\n{outer}}}", body.join(",\n")));
    }

    /// Removes a member and the comma that joined it.
    pub fn remove(&mut self, key: &str) {
        let (open, close, members) = self.scan();
        let Some(index) = members.iter().rposition(|m| m.key == key) else {
            return;
        };
        let m = &members[index];
        if index > 0 {
            self.replace(members[index - 1].end, m.end, "");
        } else if let Some(next) = members.get(index + 1) {
            self.replace(m.start, next.start, "");
        } else {
            self.replace(open + 1, close, "");
        }
    }
}
