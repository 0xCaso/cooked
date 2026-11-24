//! `StatusLineSetupTests.swift`: connecting the status line bridge changes only the `statusLine` key, and
//! disconnecting gives back the exact old text. Every file is under a temporary folder, never the real `~/.claude`.
mod common;

use cooked_model::status_line_bridge;
use cooked_model::status_line_setup::{Failure, JsonObjectText, State, StatusLineSetup};
use serde_json::Value;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

struct Fixture {
    root: String,
    setup: StatusLineSetup,
}

impl Fixture {
    fn new() -> Self {
        let root = common::temp_dir("statusline")
            .to_string_lossy()
            .into_owned();
        std::fs::create_dir_all(format!("{root}/.claude")).unwrap();
        let setup = StatusLineSetup::new(format!("{root}/Cooked"));
        Self { root, setup }
    }

    fn settings(&self) -> String {
        format!("{}/.claude/settings.json", self.root)
    }
}

fn text(path: &str) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn write(path: &str, s: &str) {
    std::fs::write(path, s).unwrap();
}

fn status_line(path: &str) -> Value {
    let object: Value = serde_json::from_str(&text(path)).unwrap();
    object["statusLine"].clone()
}

#[test]
fn no_status_line() {
    let f = Fixture::new();
    let settings = f.settings();
    let old = "{\n  \"model\": \"opus\",\n  \"env\": {\n    \"A\": \"1\"\n  }\n}\n";
    write(&settings, old);
    f.setup.connect(&settings).unwrap();
    let key = StatusLineSetup::key(&StatusLineSetup::real(&settings));
    assert_eq!(
        text(&settings),
        format!(
            "{{\n  \"model\": \"opus\",\n  \"env\": {{\n    \"A\": \"1\"\n  }},\n  \"statusLine\": {{\n    \"type\": \"command\",\n    \"command\": \"sh '{}/Cooked/statusline-bridge.sh' {key}\"\n  }}\n}}\n",
            f.root
        )
    );
    assert_eq!(text(&format!("{settings}.cooked-backup")), old);
    assert_eq!(f.setup.state(&settings), State::Connected);
    assert!(Path::new("/bin/sh").exists() && Path::new(&f.setup.script()).exists());

    assert!(f.setup.disconnect(&settings).unwrap());
    assert_eq!(text(&settings), old);
    assert_eq!(f.setup.state(&settings), State::NotConnected);
}

#[test]
fn an_old_command_is_kept_and_given_back() {
    let f = Fixture::new();
    let settings = f.settings();
    let old = r#"{"model":"opus","statusLine":{"type":"command","command":"\"/n/node\" \"/p/statusline.js\"","padding":0,"refreshInterval":5},"theme":"dark"}"#;
    write(&settings, old);
    f.setup.connect(&settings).unwrap();
    let now = text(&settings);
    assert!(
        now.starts_with(r#"{"model":"opus","statusLine":{"#)
            && now.ends_with(r#"},"theme":"dark"}"#)
    );
    let line = status_line(&settings);
    let key = StatusLineSetup::key(&StatusLineSetup::real(&settings));
    assert_eq!(
        line["command"].as_str(),
        Some(f.setup.command(&key).as_str())
    );
    assert!(line["padding"].as_i64() == Some(0) && line["refreshInterval"].as_i64() == Some(5));
    assert_eq!(
        text(&format!("{}/{key}.command", f.setup.saved_dir())),
        r#""/n/node" "/p/statusline.js""#
    );

    f.setup.disconnect(&settings).unwrap();
    assert_eq!(text(&settings), old);
}

#[test]
fn a_symlink_is_followed_and_kept() {
    let f = Fixture::new();
    let shared = format!("{}/.claude/settings.json", f.root);
    write(&shared, "{\n  \"a\": 1\n}\n");
    std::fs::create_dir_all(format!("{}/.claude-work", f.root)).unwrap();
    let link = format!("{}/.claude-work/settings.json", f.root);
    std::os::unix::fs::symlink(&shared, &link).unwrap();

    f.setup.connect(&link).unwrap();
    assert_eq!(std::fs::read_link(&link).unwrap(), Path::new(&shared));
    assert_eq!(status_line(&shared)["type"].as_str(), Some("command"));
    assert!(f.setup.state(&shared) == State::Connected && f.setup.state(&link) == State::Connected);
    // the second account shares the file: nothing more to do
    let once = text(&shared);
    f.setup.connect(&shared).unwrap();
    assert_eq!(text(&shared), once);
    assert!(!Path::new(&format!("{link}.cooked-backup")).exists());
}

#[test]
fn a_changed_key_is_not_touched() {
    let f = Fixture::new();
    let settings = f.settings();
    write(&settings, "{}");
    f.setup.connect(&settings).unwrap();
    let changed = r#"{"statusLine":{"type":"command","command":"claude-hud"}}"#;
    write(&settings, changed);
    assert_eq!(f.setup.state(&settings), State::Changed);
    assert!(!f.setup.disconnect(&settings).unwrap());
    assert_eq!(text(&settings), changed);

    // connect again wraps the new command
    f.setup.connect(&settings).unwrap();
    let key = StatusLineSetup::key(&StatusLineSetup::real(&settings));
    assert_eq!(
        text(&format!("{}/{key}.command", f.setup.saved_dir())),
        "claude-hud"
    );
    f.setup.disconnect(&settings).unwrap();
    assert_eq!(text(&settings), changed);
}

#[test]
fn the_backup_is_made_once() {
    let f = Fixture::new();
    let settings = f.settings();
    write(&settings, "{\"a\":1}");
    write(&format!("{settings}.cooked-backup"), "older");
    f.setup.connect(&settings).unwrap();
    assert_eq!(text(&format!("{settings}.cooked-backup")), "older");
}

#[test]
fn not_an_object_is_not_written() {
    let f = Fixture::new();
    let settings = f.settings();
    write(&settings, "[1, 2]");
    assert!(matches!(
        f.setup.connect(&settings),
        Err(Failure::NotAnObject)
    ));
    assert_eq!(text(&settings), "[1, 2]");
}

#[test]
fn removing_the_only_key_leaves_an_empty_object() {
    let mut object = JsonObjectText::new(r#"{"statusLine": {"command": "x"}}"#).unwrap();
    object.remove("statusLine");
    assert_eq!(object.text(), "{}");
    let mut first = JsonObjectText::new("{\n  \"statusLine\": 1,\n  \"b\": \"}\"\n}").unwrap();
    first.remove("statusLine");
    assert_eq!(first.text(), "{\n  \"b\": \"}\"\n}");
}

fn run(command: &str, input: &str) -> (String, i32) {
    let mut p = Command::new("/bin/sh")
        .args(["-c", command])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    p.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let out = p.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

/// The script saves the JSON for the account in `transcript_path`, then prints the old status line's output.
#[test]
fn the_bridge_saves_the_json_and_runs_the_old_line() {
    let f = Fixture::new();
    let settings = f.settings();
    write(
        &settings,
        r#"{"statusLine":{"type":"command","command":"read x; echo \"old line $x\" | cut -c1-12"}}"#,
    );
    f.setup.connect(&settings).unwrap();
    let input = r#"{"transcript_path":"/h/.claude-team/projects/p/s.jsonl","rate_limits":{"five_hour":{"used_percentage":42,"resets_at":1900000000}}}"#;
    let line = status_line(&settings);
    let (out, status) = run(line["command"].as_str().unwrap(), input);
    assert_eq!(status, 0);
    assert_eq!(out, "old line {\"t\n");
    assert_eq!(
        text(&format!("{}/claudeAgent_team.json", f.setup.bridge_dir())),
        input
    );
    assert_eq!(
        status_line_bridge::samples(Path::new(&f.setup.bridge_dir()))
            .get("claudeAgent_team")
            .map(|s| s.five),
        Some(0.42)
    );

    // with no old line it prints nothing, and `~/.claude` is `claudeAgent`
    f.setup.disconnect(&settings).unwrap();
    write(&settings, "{}");
    f.setup.connect(&settings).unwrap();
    let (none, _) = run(
        status_line(&settings)["command"].as_str().unwrap(),
        r#"{"transcript_path":"/h/.claude/projects/p/s.jsonl"}"#,
    );
    assert!(none.is_empty());
    assert!(Path::new(&format!("{}/claudeAgent.json", f.setup.bridge_dir())).exists());
}
