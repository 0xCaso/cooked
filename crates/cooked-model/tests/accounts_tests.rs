mod common;

use cooked_model::accounts::{self, AccountStore, ConnectedAccount, Kind};
use cooked_model::defaults::MemoryDefaults;
use cooked_model::transcript_log::TranscriptLog;
use std::collections::HashMap;

/// A temporary home with `~/.claude`, `~/.claude-work-team`, `~/.codex`, and folders that are not accounts.
fn make_home() -> String {
    let home = common::temp_dir("home");
    for dir in [
        ".claude/projects/-Users-me-demo",
        ".claude-work-team/projects/p",
        ".codex/sessions/2026/11/01",
        ".claude-empty",
        ".claude-plugins/cache",
        ".config",
    ] {
        std::fs::create_dir_all(home.join(dir)).unwrap();
    }
    std::fs::write(home.join(".codex/sessions/2026/11/01/rollout-a.jsonl"), b"").unwrap();
    home.to_str().unwrap().to_owned()
}

#[test]
fn scan_finds_claude_and_codex_folders() {
    let home = make_home();
    let found = accounts::scan(&home);
    assert_eq!(
        found.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(),
        ["claudeAgent", "claudeAgent_work_team", "codex"]
    );
    assert_eq!(
        found.iter().map(|f| f.kind).collect::<Vec<_>>(),
        [Kind::Claude, Kind::Claude, Kind::Codex]
    );
    assert_eq!(
        found.iter().map(|f| f.path.clone()).collect::<Vec<_>>(),
        [
            format!("{home}/.claude"),
            format!("{home}/.claude-work-team"),
            format!("{home}/.codex")
        ]
    );
    assert!(found.iter().all(|f| f.last_used.is_some()));
    assert_eq!(
        found.iter().map(accounts::default_name).collect::<Vec<_>>(),
        ["claude", "work team", "codex"]
    );
}

#[test]
fn ids_match_t3_code_provider_instances() {
    assert_eq!(accounts::id(".claude"), "claudeAgent");
    assert_eq!(accounts::id(".claude-team"), "claudeAgent_team");
}

#[test]
fn first_launch_connects_every_folder_then_keeps_the_users_names() {
    let home = make_home();
    let mut defaults = MemoryDefaults::default();
    let mut accounts = AccountStore::current(&mut defaults, &home);
    assert_eq!(
        accounts.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        ["claude", "work team", "codex"]
    );
    accounts[0].name = "personal".into();
    AccountStore::save(&[accounts[1].clone(), accounts[0].clone()], &mut defaults); // reordered, Codex not ticked
    let again = AccountStore::current(&mut defaults, &home);
    assert_eq!(
        again.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        ["work team", "personal"]
    );
}

#[test]
fn nothing_found_is_not_saved() {
    let mut defaults = MemoryDefaults::default();
    assert!(AccountStore::current(&mut defaults, "/nonexistent").is_empty());
    assert_eq!(AccountStore::load(&defaults), None);
}

#[test]
fn transcripts_read_only_connected_claude_folders() {
    let log = TranscriptLog::with_accounts(&[
        ConnectedAccount::new("claudeAgent", "a", "/h/.claude"),
        ConnectedAccount::new("codex", "c", "/h/.codex"),
    ]);
    assert_eq!(
        log.dirs,
        HashMap::from([("claudeAgent".to_owned(), "/h/.claude".to_owned())])
    );
}

#[test]
#[ignore = "needs Task 6: LiveSnapshot.swift (and the statusFixture from LiveDataTests)"]
fn island_shows_only_connected_accounts_in_their_order_with_their_names() {
    // Swift:
    // let status = try StatusFile.decode(Data(statusFixture.utf8))
    // let accounts = [ConnectedAccount(id: "claudeAgent", name: "home", path: "/h/.claude"),
    //                 ConnectedAccount(id: "claudeAgent_work", name: "job", path: "/h/.claude-work")]
    // let s = try #require(LiveSnapshot.make(status: status, minutes: .init(), now: Date(), accounts: accounts))
    // #expect(s.accounts.map(\.short) == ["home", "job"])
    unimplemented!("port with LiveSnapshot in Task 6");
}

/// Not a Swift case: the accounts JSON must be byte-equal to what Swift's `JSONEncoder` stores in UserDefaults
/// (keys in declaration order, `/` escaped as `\/`, non-ASCII raw). Expected bytes printed by `swift`.
#[test]
fn accounts_json_matches_swift_encoder() {
    let accounts = [
        ConnectedAccount::new("claudeAgent_work", "wörk “x” ê/ü", "/Users/me/.claude-work"),
        ConnectedAccount::new("codex", "codex", "/h/.codex"),
    ];
    let swift = r#"[{"id":"claudeAgent_work","name":"wörk “x” ê\/ü","path":"\/Users\/me\/.claude-work"},{"id":"codex","name":"codex","path":"\/h\/.codex"}]"#;
    assert_eq!(
        String::from_utf8(AccountStore::encode(&accounts)).unwrap(),
        swift
    );
    assert_eq!(
        AccountStore::decode(swift.as_bytes()),
        Some(accounts.to_vec())
    );
    let controls = [ConnectedAccount::new(
        "a\u{1}\u{1f}\u{7f}\t\n\"\\",
        "\u{2028}😀",
        "",
    )];
    assert_eq!(
        String::from_utf8(AccountStore::encode(&controls)).unwrap(),
        "[{\"id\":\"a\\u0001\\u001f\u{7f}\\t\\n\\\"\\\\\",\"name\":\"\u{2028}😀\",\"path\":\"\"}]"
    );
}
