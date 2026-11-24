mod common;

use common::fixture;
use cooked_model::transcript_log::{Titles, TranscriptLog};

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
#[ignore = "needs Task 5: TranscriptLog.swift read() and MinuteLedger.swift"]
fn live_sessions_carry_their_name() {
    // Swift: writes ~/.claude/projects/-r-demo/{main.jsonl, sub.jsonl, sub/subagents/agent-1.jsonl}, then
    // #expect(names == ["main": "Docs pass", "sub": "Refactor the parser"]); appends a custom-title "Docs v2" to
    // main.jsonl and #expect(log.read(now: now).sessions.first { $0.id == "main" }?.name == "Docs v2").
    unimplemented!("port with TranscriptLog::read in Task 5");
}

#[test]
#[ignore = "needs Task 5: TokenSources.swift make() and TranscriptLog.Session"]
fn source_rows_show_the_session_name() {
    // Swift: Session(account: "claudeAgent", id: "s", cwd: "/r/demo", tokens: 15_000, mainActive: true,
    // activeSubagents: 0, name: "Docs pass").steady → TokenSources.make(sessions: [session], threads: [:]);
    // #expect(out.first?.detail == "Docs pass"); #expect(out.first?.project == "demo")
    unimplemented!("port with TokenSources in Task 5");
}
