//! The window's flow over a temporary home (never the real `~/.claude`), the launch flags, and the scenes'
//! snapshots. The Swift `SetupModelTests` and `StatusLineSetupTests` are ported in `cooked-model` (Task 6).

use std::cell::RefCell;
use std::fs;
use std::path::Path;
use std::rc::Rc;

use cooked_model::accounts::ConnectedAccount;
use cooked_model::setup_model::{self, Step};
use cooked_model::status_line_setup::State;

use super::scenes::{self, OLD_COMMAND};
use super::*;
use crate::snaps::tests::assert_matches_fitted_snap;

fn settings_text(s: &Setup) -> String {
    fs::read_to_string(format!("{}/.claude/settings.json", s.model.home)).expect("settings.json")
}

/// The accounts the window handed the island, each time.
fn recording(s: &mut Setup) -> Rc<RefCell<Vec<Vec<ConnectedAccount>>>> {
    let pushed = Rc::new(RefCell::new(Vec::new()));
    let sink = pushed.clone();
    s.on_accounts = Some(Box::new(move |a| sink.borrow_mut().push(a)));
    pushed
}

#[test]
fn remove_changes_restores_statusline() {
    let mut s = scenes::setup("test-remove", Mode::Settings, Tab::Percent);
    let before = settings_text(&s);
    assert!(before.contains(OLD_COMMAND));
    s.model.connect();
    assert_eq!(s.model.percent(), State::Connected);
    let connected = settings_text(&s);
    assert_ne!(connected, before);
    assert!(Path::new(&s.model.status_line.script()).exists());

    s.confirming = true;
    s.remove_all();
    assert!(!s.confirming);
    assert_eq!(settings_text(&s), before, "every byte comes back");
    assert_eq!(s.model.percent(), State::NotConnected);
    assert!(!Path::new(&s.model.status_line.script()).exists());
    assert!(!s.model.open_at_login);
    // the backup stays
    let real = StatusLineSetup::real(&format!("{}/.claude/settings.json", s.model.home));
    assert_eq!(
        fs::read_to_string(format!("{real}.cooked-backup")).expect("backup"),
        before
    );
}

#[test]
fn a_fresh_user_gets_through_setup() {
    let mut s = scenes::setup("test-flow", Mode::Setup, Tab::Accounts);
    let pushed = recording(&mut s);
    let before = settings_text(&s);
    assert_eq!(s.primary_title(), "Continue");
    s.names[0] = rune::TextEdit::new("home");
    assert!(s.go(false));
    assert_eq!(s.model.step, Step::Percent);
    assert_eq!(pushed.borrow().len(), 1);
    assert_eq!(pushed.borrow()[0][0].name, "home");
    assert_eq!(settings_text(&s), before, "nothing written before Allow");
    assert_eq!(s.primary_title(), "Allow");

    assert!(s.go(false));
    assert_eq!(s.model.step, Step::Login);
    assert_eq!(s.model.percent(), State::Connected);
    s.back();
    assert!(!s.forward);
    assert_eq!(s.model.step, Step::Percent);
    assert_eq!(s.primary_title(), "Continue");
    assert!(s.go(false));

    assert!(s.model.open_at_login, "on by default on the first launch");
    assert!(!s.go(false), "Done closes the window");
    assert!(setup_model::is_done(&s.model.defaults));
    assert!(s.model.login.is_enabled());
    assert_eq!(pushed.borrow().len(), 2);
}

#[test]
fn skipping_every_step_changes_no_file() {
    let mut s = scenes::setup("test-skip", Mode::Setup, Tab::Accounts);
    let before = settings_text(&s);
    assert!(s.go(true));
    assert!(s.go(true));
    assert_eq!(s.model.step, Step::Login);
    s.set_open_at_login(false);
    assert!(!s.model.login.is_enabled(), "Setup's toggle waits for Done");
    assert_eq!(settings_text(&s), before);
    assert!(!Path::new(&s.model.status_line.script()).exists());
}

#[test]
fn settings_save_a_tick_at_once_and_a_name_when_the_user_moves_on() {
    let mut s = scenes::setup("test-settings", Mode::Settings, Tab::Accounts);
    let pushed = recording(&mut s);
    let ticked = s.model.accounts().len();
    s.toggle_row(0);
    assert_eq!(pushed.borrow().len(), 1);
    assert_eq!(pushed.borrow()[0].len(), ticked - 1);

    s.names[2] = rune::TextEdit::new("job");
    s.name_edited();
    assert_eq!(pushed.borrow().len(), 1, "typing alone saves nothing");
    s.select_tab(Tab::Percent);
    assert!(s.forward);
    assert_eq!(pushed.borrow().len(), 2);
    assert!(pushed.borrow()[1].iter().any(|a| a.name == "job"));
    s.select_tab(Tab::Accounts);
    assert!(!s.forward);
    assert_eq!(pushed.borrow().len(), 2, "nothing new to save");
}

#[test]
fn settings_toggle_login_at_once() {
    let mut s = scenes::setup("test-login", Mode::Settings, Tab::General);
    s.set_open_at_login(true);
    assert!(s.model.login.is_enabled());
    s.set_open_at_login(false);
    assert!(!s.model.login.is_enabled());
}

#[test]
fn presenting_again_reads_the_disk_and_starts_over() {
    let mut s = scenes::setup("test-present", Mode::Setup, Tab::Accounts);
    s.model.step = Step::Login;
    s.open.insert("what");
    s.confirming = true;
    s.present(Mode::Settings, Tab::Percent);
    assert_eq!(s.title(), "Cooked Settings");
    assert_eq!(s.model.step, Step::Accounts);
    assert!(s.open.is_empty() && !s.confirming);
    assert_eq!(s.names.len(), s.model.rows.len());
    s.present(Mode::Setup, Tab::Accounts);
    assert_eq!(s.title(), "Set up Cooked");
}

#[test]
fn disclosures_toggle() {
    let mut s = scenes::setup("test-disclosure", Mode::Setup, Tab::Accounts);
    s.toggle_disclosure("what");
    assert!(s.open.contains("what"));
    s.toggle_disclosure("what");
    assert!(!s.open.contains("what"));
}

#[test]
fn launch_flags() {
    let launch = |a: &[&str]| match crate::cli::parse(
        &a.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    ) {
        crate::cli::Command::App(launch) => launch,
        other => panic!("{other:?}"),
    };
    assert_eq!(launch_mode(&launch(&["cooked"]), false), Some(Mode::Setup));
    assert_eq!(launch_mode(&launch(&["cooked"]), true), None);
    assert_eq!(
        launch_mode(&launch(&["cooked", "--setup"]), true),
        Some(Mode::Setup)
    );
    assert_eq!(
        launch_mode(&launch(&["cooked", "--settings"]), true),
        Some(Mode::Settings)
    );
}

#[test]
fn every_scene_is_ours() {
    for scene in scenes::SCENES {
        let args = ["cooked", "--snap", "s.png", "--scene", scene].map(String::from);
        assert!(crate::snaps::wanted_scene(&args, scenes::SCENES));
        assert!(!crate::snaps::wanted_scene(
            &args,
            crate::popover::scenes::SCENES
        ));
    }
}

#[test]
fn kept_login_keeps_the_choice() {
    let mut login = KeptLogin::default();
    assert!(!login.is_enabled());
    assert_eq!(login.set(true), Ok(()));
    assert!(login.clone().is_enabled());
}

#[test]
fn only_the_app_uses_the_real_login_item() {
    assert!(matches!(Login::default(), Login::Kept(_)));
    let s = scenes::setup("test-kept-login", Mode::Settings, Tab::General);
    assert!(matches!(s.model.login, Login::Kept(_)));
}

#[test]
fn scenes_show_the_mockup_states() {
    let s = scenes::scene_in("settings-changed", "test-changed");
    assert_eq!(s.model.percent(), State::Changed);
    assert_eq!(
        s.model.current_command().as_deref(),
        Some(scenes::NEW_COMMAND)
    );
    assert_eq!(s.model.wrapped_command().as_deref(), Some(OLD_COMMAND));
    let s = scenes::scene_in("settings-connected", "test-connected");
    assert_eq!(s.model.percent(), State::Connected);
    let files = s.model.settings_files();
    assert_eq!(files.len(), 1, "three accounts, one file");
    assert_eq!(files[0].readers.len(), 3);
    assert!(s.model.remove_sentence().contains("in all 3 accounts"));
}

fn snap(scene: &str) {
    assert_matches_fitted_snap(scenes::app, scene);
}

#[test]
fn setup_accounts_matches_the_checked_in_snap() {
    snap("setup-accounts");
}

#[test]
fn setup_percent_matches_the_checked_in_snap() {
    snap("setup-percent");
}

#[test]
fn setup_login_matches_the_checked_in_snap() {
    snap("setup-login");
}

#[test]
fn settings_connected_matches_the_checked_in_snap() {
    snap("settings-connected");
}

#[test]
fn settings_off_matches_the_checked_in_snap() {
    snap("settings-off");
}

#[test]
fn settings_changed_matches_the_checked_in_snap() {
    snap("settings-changed");
}

#[test]
fn settings_remove_matches_the_checked_in_snap() {
    snap("settings-remove");
}

#[test]
fn settings_general_matches_the_checked_in_snap() {
    snap("settings-general");
}
