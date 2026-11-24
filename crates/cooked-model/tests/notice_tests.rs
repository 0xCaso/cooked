//! `NoticeTests.swift`: the popover's notice card.
mod common;

use cooked_model::accounts::ConnectedAccount;
use cooked_model::clock::FixedOffset;
use cooked_model::defaults::MemoryDefaults;
use cooked_model::demo_data;
use cooked_model::island_model::IslandState;
use cooked_model::live_snapshot::{self, Minutes};
use cooked_model::notice::Notice;
use cooked_model::session_format::SessionFormat;
use cooked_model::status_file::StatusFile;
use cooked_model::status_line_setup::State;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, UNIX_EPOCH};

fn work() -> ConnectedAccount {
    ConnectedAccount::new("claudeAgent_work", "work", "/h/.claude-work")
}
fn personal() -> ConnectedAccount {
    ConnectedAccount::new("claudeAgent", "personal", "/h/.claude")
}
fn codex() -> ConnectedAccount {
    ConnectedAccount::new("codex", "codex", "/h/.codex")
}

fn set(ids: &[&str]) -> HashSet<String> {
    ids.iter().map(|s| (*s).to_owned()).collect()
}

fn lines(states: &[(&str, State)]) -> HashMap<String, State> {
    states.iter().map(|(id, s)| ((*id).to_owned(), *s)).collect()
}

#[test]
fn one_at_a_time_in_order() {
    let all = Notice::all(
        &[work(), personal(), codex()],
        &HashMap::from([("codex".to_owned(), "/h/.codex/r.jsonl".to_owned())]),
        &lines(&[("claudeAgent_work", State::Changed), ("claudeAgent", State::NotConnected)]),
        &set(&["codex"]),
        &HashSet::new(),
    );
    assert_eq!(
        all,
        [
            Notice::FormatChanged {
                account: "codex".to_owned(),
                file: "/h/.codex/r.jsonl".to_owned()
            },
            Notice::Paused,
            Notice::NoPercent {
                id: "claudeAgent".to_owned(),
                account: "personal".to_owned()
            },
        ]
    );
    let none = HashMap::new();
    assert_eq!(
        Notice::all(&[], &none, &HashMap::new(), &HashSet::new(), &HashSet::new()),
        [Notice::NoAccounts]
    );
}

#[test]
fn api_key_only_after_the_status_line_ran() {
    let l = lines(&[("claudeAgent_work", State::Connected)]);
    let none = HashMap::new();
    let empty = HashSet::new();
    // connected, no call yet: nothing to say
    assert!(Notice::all(&[work()], &none, &l, &empty, &empty).is_empty());
    assert_eq!(
        Notice::all(&[work()], &none, &l, &empty, &set(&["claudeAgent_work"])),
        [Notice::ApiKey {
            id: "claudeAgent_work".to_owned(),
            account: "work".to_owned()
        }]
    );
    assert!(
        Notice::all(&[work()], &none, &l, &set(&["claudeAgent_work"]), &set(&["claudeAgent_work"]))
            .is_empty()
    );
    // Codex always has its percent in the rollouts: never asked to connect
    assert!(Notice::all(&[codex()], &none, &HashMap::new(), &empty, &empty).is_empty());
}

#[test]
fn not_now_hides_it_until_the_accounts_change() {
    let mut model = IslandState::new(demo_data::calm(), None, MemoryDefaults::default(), 0.0);
    let missing = Notice::NoPercent {
        id: "claudeAgent".to_owned(),
        account: "personal".to_owned(),
    };
    let api_key = Notice::ApiKey {
        id: "claudeAgent_work".to_owned(),
        account: "work".to_owned(),
    };
    model.show(vec![missing.clone(), api_key.clone()]);
    model.dismiss(&missing);
    assert_eq!(model.notice(), Some(&api_key));
    let defaults = model.defaults().clone();
    let relaunched = IslandState::new(demo_data::calm(), None, defaults, 0.0);
    // kept across launches
    assert_eq!(
        relaunched.dismissed(),
        &HashSet::from(["noPercent:claudeAgent".to_owned()])
    );
    model.accounts_changed();
    assert_eq!(model.notice(), Some(&missing));
    model.dismiss(&Notice::Paused); // a notice with a fix cannot be put off
    assert!(model.dismissed().is_empty());
}

#[test]
fn a_connected_account_without_a_percent_keeps_its_row() {
    let status = StatusFile::default();
    let now = common::now();
    let minute_now = now as i64 / 60;
    let minutes = Minutes::from([("claudeAgent".to_owned(), HashMap::from([(minute_now, 5000.0)]))]);
    let s = live_snapshot::make(&status, &minutes, now, &FixedOffset(0), &[], &[personal()]);
    assert_eq!(
        s.as_ref().map(|s| s.accounts.iter().map(|a| a.id.clone()).collect::<Vec<_>>()),
        Some(vec!["claudeAgent".to_owned()])
    );
    assert_eq!(s.as_ref().and_then(|s| s.active()).map(|a| a.has_percent), Some(false));
    assert_eq!(s.as_ref().and_then(|s| s.active()).and_then(|a| a.five_hour_left_if_known()), None);
    assert_eq!(s.as_ref().map(|s| s.is_urgent()), Some(false));
    assert!(s.map_or(0, |s| s.rate_now) > 0);
}

#[test]
fn only_the_newest_file_is_checked() {
    let dir = common::temp_dir("format");
    let (old, new) = (dir.join("old.jsonl"), dir.join("new.jsonl"));
    std::fs::write(&old, r#"{"type":"user","sessionId":"s"}"#).unwrap();
    std::fs::write(&new, r#"{"kind":"turn","id":"s"}"#).unwrap();
    let mut cache = HashMap::new();
    let urls = [
        (old, UNIX_EPOCH + Duration::from_secs(1)),
        (new.clone(), UNIX_EPOCH + Duration::from_secs(2)),
    ];
    let new_path = new.to_string_lossy().into_owned();
    assert_eq!(SessionFormat::newest_changed(&urls, &mut cache), Some(new_path.clone()));
    assert_eq!(cache, HashMap::from([(new_path, SessionFormat::FormatChanged)]));
    assert_eq!(SessionFormat::newest_changed(&urls[..1], &mut cache), None);
}
