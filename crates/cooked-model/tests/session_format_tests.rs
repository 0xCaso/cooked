mod common;

use common::{fixture, lines, token_counts};
use cooked_model::session_format::SessionFormat;
use cooked_model::transcript_log::TranscriptLog;
use std::collections::HashSet;

#[test]
fn claude_sample_is_known_and_counts_one_message_once() {
    assert_eq!(
        SessionFormat::check_file(&fixture("claude-session.jsonl")),
        SessionFormat::Claude
    );
    let calls: Vec<_> = lines("claude-session.jsonl")
        .iter()
        .filter_map(|l| TranscriptLog::parse(l))
        .collect();
    assert_eq!(calls.len(), 2); // one message written in two content blocks
    assert_eq!(
        calls
            .iter()
            .map(|c| c.id.as_str())
            .collect::<HashSet<_>>()
            .len(),
        1
    );
    assert_eq!(calls[0].tokens, (2 + 33682 + 1240) as f64); // input + cache write + output, not cache read
    assert_eq!(calls[0].cwd.as_deref(), Some("/Users/me/Projects/demo"));
}

#[test]
fn claude_sample_has_the_name_lines() {
    let kinds: HashSet<String> = lines("claude-session.jsonl")
        .iter()
        .filter_map(|l| {
            let v: serde_json::Value = serde_json::from_slice(l).unwrap();
            v.get("type")?.as_str().map(str::to_owned)
        })
        .collect();
    for kind in ["custom-title", "ai-title", "user", "assistant"] {
        assert!(kinds.contains(kind), "missing {kind}");
    }
}

#[test]
fn codex_samples_are_known() {
    assert_eq!(
        SessionFormat::check_file(&fixture("codex-rollout.jsonl")),
        SessionFormat::Codex
    );
    assert_eq!(
        SessionFormat::check_file(&fixture("codex-imported-rollout.jsonl")),
        SessionFormat::Codex
    );
}

#[test]
fn codex_cli_rollout_has_limits_and_a_repeated_event() {
    let counts = token_counts("codex-rollout.jsonl");
    assert_eq!(counts.len(), 3);
    assert!(counts.iter().all(|c| {
        c.get("rate_limits")
            .is_some_and(serde_json::Value::is_object)
    }));
    // the last two events carry the same totals: the Codex reader must count them once
    let totals: Vec<Option<i64>> = counts
        .iter()
        .map(|c| {
            c.pointer("/info/total_token_usage/total_tokens")
                .and_then(serde_json::Value::as_i64)
        })
        .collect();
    assert!(totals[1].is_some() && totals[1] == totals[2]);
}

#[test]
fn codex_imported_rollout_adds_no_tokens() {
    let counts = token_counts("codex-imported-rollout.jsonl");
    assert!(!counts.is_empty());
    for count in &counts {
        assert!(
            count
                .get("rate_limits")
                .is_none_or(serde_json::Value::is_null)
        );
        let last = count.pointer("/info/last_token_usage");
        assert!(
            last.and_then(|l| l.get("input_tokens"))
                .and_then(serde_json::Value::as_i64)
                == Some(0)
                && last
                    .and_then(|l| l.get("output_tokens"))
                    .and_then(serde_json::Value::as_i64)
                    == Some(0)
        );
    }
}

#[test]
fn codex_session_index_has_names() {
    let rows: Vec<serde_json::Value> = lines("codex-session-index.jsonl")
        .iter()
        .map(|l| serde_json::from_slice(l).unwrap())
        .collect();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|r| r.get("id").is_some_and(|v| v.is_string())
                && r.get("thread_name").is_some_and(|v| v.is_string()))
    );
}

#[test]
fn unknown_lines_are_a_format_change_not_zero() {
    assert_eq!(
        SessionFormat::check_file(&fixture("unknown-format.jsonl")),
        SessionFormat::FormatChanged
    );
    assert_eq!(
        SessionFormat::check([b"not json".as_slice()]),
        SessionFormat::FormatChanged
    );
    assert_eq!(SessionFormat::check([b"".as_slice()]), SessionFormat::Empty);
    assert_eq!(
        SessionFormat::check_file(&fixture("missing.jsonl")),
        SessionFormat::Empty
    );
}
