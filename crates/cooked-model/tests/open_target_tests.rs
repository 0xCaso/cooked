//! A click on a row goes where the session runs (release spec, section 1, Click on a row).
//! Where the app sends each target (`SessionOpener`) is tested in `crates/cooked/src/mac/open.rs`.

use std::collections::HashMap;

use cooked_model::accounts::ConnectedAccount;
use cooked_model::token_log::Thread;
use cooked_model::token_sources::{self, OpenTarget};
use cooked_model::transcript_log::Session;

fn accounts() -> Vec<ConnectedAccount> {
    vec![
        ConnectedAccount::new("claudeAgent", "personal", "/Users/x/.claude"),
        ConnectedAccount::new("claudeAgent_work", "work", "/Users/x/.claude-work"),
        ConnectedAccount::new("codex", "codex", "/Users/x/.codex"),
    ]
}

/// A session with a steady rate.
fn session(account: &str, id: &str) -> Session {
    let mut s = Session::new(account, id, Some("/Users/x/My Repo".into()), 600.0, true, 0);
    s.last_five = s.tokens / 3.0;
    s
}

#[test]
fn each_kind_of_row_has_its_place() {
    let threads = HashMap::from([(
        "t3-session".to_owned(),
        Thread {
            id: "thread-1".into(),
            title: "A thread".into(),
            project: Some("p".into()),
        },
    )]);
    let sessions = [
        session("claudeAgent", "t3-session"),
        session("claudeAgent", "home"),
        session("claudeAgent_work", "work"),
        session("codex", "cx"),
    ];
    let sources = token_sources::make(&sessions, &threads, &accounts(), None);
    let by_id: HashMap<_, _> = sources
        .iter()
        .map(|s| (s.id.as_str(), s.target()))
        .collect();
    let cwd = Some("/Users/x/My Repo".to_owned());
    assert_eq!(by_id["thread-1"], Some(OpenTarget::t3_code("thread-1")));
    assert_eq!(
        by_id["home"],
        Some(OpenTarget::claude("home", cwd.clone(), None))
    );
    assert_eq!(
        by_id["work"],
        Some(OpenTarget::claude(
            "work",
            cwd.clone(),
            Some("/Users/x/.claude-work".into())
        ))
    );
    assert_eq!(by_id["cx"], Some(OpenTarget::codex("cx", cwd)));
}

#[test]
fn the_terminal_fallback_resumes_in_the_project_folder() {
    let other = OpenTarget::claude(
        "work",
        Some("/Users/x/it's".into()),
        Some("/Users/x/.claude-work".into()),
    );
    assert_eq!(
        other.script(),
        "#!/bin/sh\ncd '/Users/x/it'\\''s' || exit 1\nCLAUDE_CONFIG_DIR='/Users/x/.claude-work' claude --resume 'work'\n"
    );
    assert_eq!(
        OpenTarget::claude("home", None, None).command(),
        "claude --resume 'home'"
    );
    assert_eq!(OpenTarget::codex("cx", None).command(), "codex resume 'cx'");
}
