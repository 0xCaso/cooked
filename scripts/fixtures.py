#!/usr/bin/env python3
"""Makes the format samples in Tests/CookedTests/Fixtures from real Claude Code and Codex files on this Mac.

Cooked reads files whose format is not documented. When a CLI update changes it, run this script again and see
which test breaks:

    python3 -I scripts/fixtures.py

The samples keep the structure, the keys, the numbers and the times. Every free text (prompts, answers, tool
output, paths, titles, instructions) becomes a placeholder, so nothing private goes into the repo.
"""
import glob
import json
import os
import sys

HOME = os.path.expanduser("~")
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "Tests", "CookedTests", "Fixtures")

# Strings kept as they are: line kinds, times, ids, model names and enum-like values.
KEEP = {
    "type", "timestamp", "id", "uuid", "parentUuid", "sessionId", "session_id", "requestId", "leafUuid",
    "model", "role", "stop_reason", "entrypoint", "version", "originator", "source", "cli_version",
    "model_provider", "limit_id", "plan_type", "mode", "userType", "updated_at", "effort", "speed",
    "service_tier", "turn_id", "promptId",
}
TEXT = {"aiTitle": "Demo session title", "customTitle": "Renamed demo session", "thread_name": "Demo Codex thread",
        "lastPrompt": "Fix the flaky login test"}
CWD = "/Users/me/Projects/demo"


def clean(value, key=None):
    if isinstance(value, dict):
        return {k: clean(v, k) for k, v in value.items()}
    if isinstance(value, list):
        return [clean(v, key) for v in value]
    if not isinstance(value, str):
        return value
    if key in TEXT:
        return TEXT[key]
    if key in KEEP and HOME not in value:
        return value
    if key in ("cwd", "originCwd") or value.startswith("/"):
        return CWD
    return "text"


def lines(path):
    with open(path, errors="ignore") as f:
        for line in f:
            try:
                yield json.loads(line)
            except ValueError:
                pass


def newest(pattern, accept):
    for path in sorted(glob.glob(os.path.expanduser(pattern)), key=os.path.getmtime, reverse=True):
        if accept(path):
            return path
    sys.exit(f"no file for {pattern}")


def write(name, rows):
    with open(os.path.join(OUT, name), "w") as f:
        for row in rows:
            f.write(json.dumps(clean(row), separators=(",", ":")) + "\n")
    print(f"{name}: {len(rows)} lines")


def claude():
    """A closed session with a first prompt, calls (one message in two blocks), a name and other kinds."""
    def has_all(path):
        kinds = {j.get("type") for j in lines(path)}
        return os.path.getsize(path) < 5_000_000 and {"user", "assistant", "ai-title", "custom-title"} <= kinds
    path = newest("~/.claude*/projects/*/*.jsonl", has_all)
    rows, calls, ids = [], 0, set()
    for j in lines(path):
        kind = j.get("type")
        if kind == "user" and not any(r.get("type") == "user" for r in rows):
            j["message"] = {"role": "user", "content": "Fix the flaky login test"}
            rows.append(j)
        elif kind == "assistant" and j.get("message", {}).get("usage") and calls < 4:
            msg = j["message"]
            calls += msg.get("id") not in ids
            ids.add(msg.get("id"))
            msg["content"] = [{"type": "text", "text": "text"}]
            rows.append(j)
        elif kind in ("ai-title", "custom-title", "last-prompt", "mode") and kind not in {r.get("type") for r in rows}:
            rows.append(j)
        elif kind == "attachment" and sum(r.get("type") == "attachment" for r in rows) < 2:
            j["attachment"] = {"type": "text"}
            j.pop("rendered", None)
            rows.append(j)
    write("claude-session.jsonl", rows)


def codex():
    """A CLI rollout with calls and limits, a rollout the Codex app writes when it imports a Claude conversation,
    and the thread names."""
    def rollout(path):
        head = next(lines(path), {})
        return head.get("payload", {}).get("originator") in ("codex_cli_rs", "codex-tui", "T3 Code") and any(
            (j.get("payload") or {}).get("type") == "token_count" and j["payload"].get("rate_limits") for j in lines(path))
    path = newest("~/.codex/sessions/*/*/*/rollout-*.jsonl", rollout)
    rows, counts = [], 0
    for j in lines(path):
        p = j.get("payload") or {}
        kind = p.get("type") or j.get("type")
        if j.get("type") == "session_meta":
            p.pop("base_instructions", None)
            p.pop("git", None)
            rows.append(j)
        elif kind in ("task_started", "user_message", "task_complete") and kind not in {
                (r.get("payload") or {}).get("type") for r in rows}:
            rows.append(j)
        elif kind == "token_count" and p.get("info") and counts < 3:
            counts += 1
            rows.append(j)
    write("codex-rollout.jsonl", rows)

    def imported(path):
        rows = list(lines(path))
        counts = [(j.get("payload") or {}) for j in rows if (j.get("payload") or {}).get("type") == "token_count"]
        return rows and rows[0].get("payload", {}).get("originator") == "Codex Desktop" and counts and all(
            not c.get("rate_limits") and not ((c.get("info") or {}).get("last_token_usage") or {}).get("output_tokens")
            for c in counts) and any((j.get("payload") or {}).get("type") == "user_message" for j in rows)
    path = newest("~/.codex/sessions/*/*/*/rollout-*.jsonl", imported)
    rows = []
    for j in lines(path):
        p = j.get("payload") or {}
        kind = p.get("type") or j.get("type")
        if j.get("type") == "session_meta":
            p.pop("base_instructions", None)
            rows.append(j)
        elif kind in ("task_started", "user_message", "agent_message", "task_complete", "token_count") and kind not in {
                (r.get("payload") or {}).get("type") for r in rows}:
            rows.append(j)
    write("codex-imported-rollout.jsonl", rows)

    index = [j for j in lines(os.path.join(HOME, ".codex", "session_index.jsonl"))][-2:]
    write("codex-session-index.jsonl", index)


if __name__ == "__main__":
    os.makedirs(OUT, exist_ok=True)
    claude()
    codex()
    # A file in a format Cooked does not know: it must give "format changed", not 0.
    with open(os.path.join(OUT, "unknown-format.jsonl"), "w") as f:
        f.write('{"kind":"call","at":"2026-11-01T10:00:00Z","tokens":{"in":10,"out":5}}\n')
    print("unknown-format.jsonl: 1 line")
