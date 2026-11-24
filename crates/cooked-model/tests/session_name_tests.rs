mod common;

use common::fixture;
use cooked_model::accounts::ConnectedAccount;
use cooked_model::clock::iso_string;
use cooked_model::token_sources;
use cooked_model::transcript_log::{Session, Titles, TranscriptLog};
use std::collections::HashMap;

fn titles(lines: &[&str]) -> Titles {
    let mut t = Titles::default();
    for line in lines {
        TranscriptLog::title(line.as_bytes(), &mut t);
    }
    t
}

fn s(v: &str) -> Option<String> {
    Some(v.to_owned())
}

#[test]
fn sample_names_from_the_real_format() {
    let head = TranscriptLog::head_titles(&fixture("claude-session.jsonl"));
    assert_eq!(
        head,
        Titles {
            custom: s("Renamed demo session"),
            ai: s("Demo session title"),
            first_prompt: s("Fix the flaky login test"),
            cwd: s("/Users/me/Projects/demo"),
        }
    );
    assert_eq!(head.name(&Titles::default()), s("Renamed demo session"));
}

#[test]
fn custom_then_ai_then_first_prompt() {
    let prompt = r#"{"type":"user","message":{"role":"user","content":"Fix the build\nand more"}}"#;
    let ai = r#"{"type":"ai-title","aiTitle":"Build fix","sessionId":"s"}"#;
    let custom = r#"{"type":"custom-title","customTitle":"My build","sessionId":"s"}"#;
    assert_eq!(
        titles(&[prompt]).name(&Titles::default()),
        s("Fix the build")
    );
    assert_eq!(
        titles(&[prompt, ai]).name(&Titles::default()),
        s("Build fix")
    );
    assert_eq!(
        titles(&[prompt, ai, custom]).name(&Titles::default()),
        s("My build")
    );
    // newer lines win: a rename in the tail beats the head's title
    let later_custom = Titles {
        custom: s("Renamed later"),
        ..Titles::default()
    };
    assert_eq!(
        titles(&[prompt, custom]).name(&later_custom),
        s("Renamed later")
    );
    let later_ai = Titles {
        ai: s("Newer AI title"),
        ..Titles::default()
    };
    assert_eq!(titles(&[prompt, custom]).name(&later_ai), s("My build"));
}

#[test]
fn first_prompt_skips_commands_context_and_tool_output() {
    let t = titles(&[
        r#"{"type":"user","isMeta":true,"message":{"content":"Caveat: injected"}}"#,
        r#"{"type":"user","message":{"content":"<command-name>/clear</command-name>"}}"#,
        r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"output"}]}}"#,
        r#"{"type":"user","message":{"content":[{"type":"text","text":"  \n  Plan the release  "}]}}"#,
        r#"{"type":"user","message":{"content":"Second prompt"}}"#,
    ]);
    assert_eq!(t.first_prompt, s("Plan the release"));
}

#[test]
fn long_names_are_cut() {
    let name = TranscriptLog::clean(Some(&"a".repeat(120)));
    assert!(
        name.as_ref()
            .is_some_and(|n| n.chars().count() == 80 && n.ends_with('…'))
    );
}

#[test]
fn live_sessions_carry_their_name() {
    let now = common::now();
    let at = iso_string(now - 60.0, true);
    let call = |id: &str| {
        format!(
            r#"{{"type":"assistant","timestamp":"{at}","cwd":"/r/demo","message":{{"id":"{id}","usage":{{"input_tokens":10,"output_tokens":5}}}}}}"#
        )
    };
    let home = common::temp_dir("home");
    let project = home.join(".claude/projects/-r-demo");
    std::fs::create_dir_all(project.join("sub/subagents")).expect("dirs");
    // a session with its own call, and one where only a subagent works: its name comes from the main file
    let main = [
        r#"{"type":"user","message":{"content":"Write the docs"}}"#.to_owned(),
        r#"{"type":"ai-title","aiTitle":"Docs pass","sessionId":"main"}"#.to_owned(),
        call("m1"),
    ]
    .join("\n")
        + "\n";
    std::fs::write(project.join("main.jsonl"), main).expect("main");
    std::fs::write(
        project.join("sub.jsonl"),
        r#"{"type":"user","message":{"content":"Refactor the parser"}}"#.to_owned() + "\n",
    )
    .expect("sub");
    std::fs::write(
        project.join("sub/subagents/agent-1.jsonl"),
        call("a1") + "\n",
    )
    .expect("agent");
    let accounts = [ConnectedAccount::new(
        "claudeAgent",
        "claude",
        home.join(".claude").to_string_lossy(),
    )];
    let mut log = TranscriptLog::with_accounts(&accounts, None);
    let names: HashMap<String, Option<String>> = log
        .read(now, 60)
        .sessions
        .into_iter()
        .map(|s| (s.id, s.name))
        .collect();
    assert_eq!(
        names,
        HashMap::from([
            ("main".to_owned(), s("Docs pass")),
            ("sub".to_owned(), s("Refactor the parser"))
        ])
    );
    // a rename while running shows at the next reading
    common::append_raw(
        &project.join("main.jsonl"),
        &(r#"{"type":"custom-title","customTitle":"Docs v2","sessionId":"main"}"#.to_owned()
            + "\n"),
    );
    let sessions = log.read(now, 60).sessions;
    assert_eq!(
        sessions
            .iter()
            .find(|s| s.id == "main")
            .and_then(|s| s.name.clone()),
        s("Docs v2")
    );
}

#[test]
fn source_rows_show_the_session_name() {
    let mut session = Session::new(
        "claudeAgent",
        "s",
        Some("/r/demo".to_owned()),
        15_000.0,
        true,
        0,
    );
    session.name = s("Docs pass");
    session.last_five = session.tokens / 3.0; // steady: as busy in its last 5 minutes as in its last 15
    let out = token_sources::make(&[session], &HashMap::new(), &[], None);
    assert_eq!(out.first().map(|o| o.detail.as_str()), Some("Docs pass"));
    assert_eq!(out.first().map(|o| o.project.as_str()), Some("demo"));
}
