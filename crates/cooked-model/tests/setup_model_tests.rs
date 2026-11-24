//! `SetupModelTests.swift`: a fresh user (temporary home) gets through Setup, and skipping each step leaves their
//! files as they were. Every file is under a temporary home, never the real `~/.claude`.
mod common;

use cooked_model::accounts::{self, AccountStore, ConnectedAccount};
use cooked_model::clock::{FixedOffset, iso_date};
use cooked_model::defaults::MemoryDefaults;
use cooked_model::setup_model::{self, LastUse, LoginItem, SetupModel, Step};
use cooked_model::status_line_setup::{State, StatusLineSetup};
use std::path::Path;

#[derive(Default)]
struct FakeLogin {
    is_enabled: bool,
}

impl LoginItem for FakeLogin {
    fn is_enabled(&self) -> bool {
        self.is_enabled
    }
    fn set(&mut self, on: bool) -> Result<(), String> {
        self.is_enabled = on;
        Ok(())
    }
}

const SETTINGS_TEXT: &str =
    "{\n  \"statusLine\": {\"type\": \"command\", \"command\": \"my-line\"}\n}\n";

/// A home with two Claude folders sharing one settings file (a symlink), Codex, and a Claude folder with no sessions.
fn make_home() -> String {
    let home = common::temp_dir("setup").to_string_lossy().into_owned();
    for dir in [
        ".claude/projects/p",
        ".claude-work/projects/p",
        ".codex/sessions/2026/11/04",
        ".claude-empty",
    ] {
        std::fs::create_dir_all(format!("{home}/{dir}")).unwrap();
    }
    std::fs::write(format!("{home}/.claude/settings.json"), SETTINGS_TEXT).unwrap();
    std::os::unix::fs::symlink(
        format!("{home}/.claude/settings.json"),
        format!("{home}/.claude-work/settings.json"),
    )
    .unwrap();
    home
}

fn model(home: &str) -> SetupModel<MemoryDefaults, FakeLogin> {
    SetupModel::new(
        home,
        MemoryDefaults::default(),
        StatusLineSetup::new(format!("{home}/support")),
        FakeLogin::default(),
    )
}

fn settings(home: &str) -> String {
    std::fs::read_to_string(format!("{home}/.claude/settings.json")).unwrap_or_default()
}

fn ids(m: &SetupModel<MemoryDefaults, FakeLogin>) -> Vec<String> {
    m.rows.iter().map(|r| r.id().to_owned()).collect()
}

#[test]
fn a_fresh_user_gets_through_setup() {
    let home = make_home();
    let mut m = model(&home);
    assert_eq!(ids(&m), ["claudeAgent", "claudeAgent_work", "codex"]);
    assert!(m.rows.iter().all(|r| r.ticked));
    assert_eq!(m.steps(), [Step::Accounts, Step::Percent, Step::Login]);
    assert!(m.open_at_login);

    m.rows[0].name = "personal".to_owned();
    m.rows[2].ticked = false;
    let files = m.settings_files();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .readers
            .iter()
            .map(|r| r.name.as_str())
            .collect::<Vec<_>>(),
        ["personal", "work"]
    );
    assert_eq!(
        files[0]
            .readers
            .iter()
            .map(|r| r.is_link)
            .collect::<Vec<_>>(),
        [false, true]
    );
    assert!(m.remove_sentence().starts_with(
        "Your old status line comes back in both accounts, Cooked stops opening at login"
    ));

    m.connect();
    assert_eq!(m.percent(), State::Connected);
    assert_eq!(m.wrapped_command().as_deref(), Some("my-line"));
    m.finish();
    assert_eq!(
        AccountStore::load(&m.defaults).map(|a| a.into_iter().map(|a| a.name).collect::<Vec<_>>()),
        Some(vec!["personal".to_owned(), "work".to_owned()])
    );
    assert!(m.login.is_enabled());
    assert!(setup_model::is_done(&m.defaults));

    // Remove undoes everything and keeps the backup
    m.remove_all();
    assert_eq!(settings(&home), SETTINGS_TEXT);
    assert_eq!(m.percent(), State::NotConnected);
    assert!(!m.login.is_enabled() && !m.open_at_login);
    assert!(!Path::new(&format!("{home}/support/statusline-bridge.sh")).exists());
    assert!(Path::new(&format!("{home}/.claude/settings.json.cooked-backup")).exists());
}

#[test]
fn skipping_every_step_changes_no_file() {
    let home = make_home();
    let mut m = model(&home);
    m.open_at_login = false;
    m.finish();
    assert_eq!(settings(&home), SETTINGS_TEXT);
    assert!(!Path::new(&format!("{home}/support")).exists());
    assert!(!m.login.is_enabled());
    assert_eq!(AccountStore::load(&m.defaults).map(|a| a.len()), Some(3));
    assert!(setup_model::is_done(&m.defaults));
}

#[test]
fn no_claude_account_no_percent_step() {
    let home = make_home();
    let mut m = model(&home);
    m.rows[0].ticked = false;
    m.rows[1].ticked = false;
    assert_eq!(m.steps(), [Step::Accounts, Step::Login]);
    assert_eq!(m.next(Step::Accounts), Some(Step::Login));
    assert_eq!(m.back(Step::Login), Some(Step::Accounts));
}

#[test]
fn saved_accounts_keep_their_tick_and_name() {
    let home = make_home();
    let found = accounts::scan(&home);
    let saved = [ConnectedAccount::new(
        "claudeAgent_work",
        "job",
        format!("{home}/.claude-work"),
    )];
    let rows = setup_model::rows(&found, Some(&saved));
    assert_eq!(
        rows.iter().map(|r| r.ticked).collect::<Vec<_>>(),
        [false, true, false]
    );
    assert_eq!(
        rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
        ["claude", "job", "codex"]
    );
}

#[test]
fn an_empty_name_falls_back_to_the_default() {
    let home = make_home();
    let mut m = model(&home);
    m.rows[1].name = "  ".to_owned();
    assert_eq!(m.accounts()[1].name, "work");
}

#[test]
fn a_changed_line_shows_as_changed() {
    let home = make_home();
    let mut m = model(&home);
    m.connect();
    std::fs::write(
        format!("{home}/.claude/settings.json"),
        r#"{"statusLine":{"type":"command","command":"npx claude-hud"}}"#,
    )
    .unwrap();
    m.refresh();
    assert_eq!(m.percent(), State::Changed);
    assert_eq!(m.current_command().as_deref(), Some("npx claude-hud"));
    m.connect();
    assert_eq!(m.percent(), State::Connected);
}

#[test]
fn last_use_reads_short() {
    let rome = FixedOffset(3600);
    let now = iso_date("2026-11-04T17:00:00.000Z").unwrap();
    assert_eq!(LastUse::text(Some(now - 3600.0), now, &rome), "today 17:00");
    assert_eq!(LastUse::text(Some(now - 86400.0), now, &rome), "yesterday");
    assert_eq!(
        LastUse::text(Some(now - 4.0 * 86400.0), now, &rome),
        "Sat 31 Oct"
    );
    assert_eq!(LastUse::text(None, now, &rome), "never");
}
