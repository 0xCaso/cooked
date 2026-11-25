//! The minute cache: restarts, the first 14-day fill, old minutes, messages in several blocks, load time.
mod common;

use common::{append, append_raw, set_modified, temp_dir};
use cooked_model::clock::iso_string;
use cooked_model::minute_ledger::MinuteLedger;
use cooked_model::transcript_log::TranscriptLog;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

struct Fixture {
    now: f64,
}

impl Fixture {
    fn new() -> Self {
        Self { now: common::now() }
    }

    fn minute_now(&self) -> i64 {
        self.now as i64 / 60
    }

    /// A Claude call `ago` minutes before `now`, with `tokens` new tokens.
    fn call(&self, id: &str, ago: i64, tokens: i64) -> String {
        let at = iso_string(self.now - (ago * 60) as f64, true);
        format!(
            r#"{{"type":"assistant","timestamp":"{at}","message":{{"id":"{id}","usage":{{"input_tokens":{tokens},"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":0}}}}}}"#
        )
    }

    fn minute(&self, ago: i64) -> i64 {
        (self.now - (ago * 60) as f64) as i64 / 60
    }

    /// One reading, like `UsageSource` does it: begin, read, end.
    fn reading(&self, log: &mut TranscriptLog) -> HashMap<i64, f64> {
        self.reading_at(log, self.now)
    }

    fn reading_at(&self, log: &mut TranscriptLog, now: f64) -> HashMap<i64, f64> {
        let ledger = log.ledger();
        ledger.begin();
        let mut r = log.read(now, 14 * 24 * 60);
        ledger.end(now);
        r.minutes.remove("a").unwrap_or_default()
    }
}

/// A config folder with `files` session files in one project.
fn make_dir(files: usize) -> (String, Vec<PathBuf>) {
    let dir = temp_dir("ledger");
    let project = dir.join("projects/p");
    std::fs::create_dir_all(&project).expect("project");
    let urls = (0..files)
        .map(|i| project.join(format!("s{i}.jsonl")))
        .collect();
    (dir.to_string_lossy().into_owned(), urls)
}

fn cache_url() -> PathBuf {
    temp_dir("minutes").join("minutes.json")
}

fn log(dir: &str, ledger: &Arc<MinuteLedger>) -> TranscriptLog {
    TranscriptLog::new(
        HashMap::from([("a".to_owned(), dir.to_owned())]),
        Some(Arc::clone(ledger)),
    )
}

fn sum(m: &HashMap<i64, f64>) -> f64 {
    m.values().sum()
}

#[test]
fn a_restart_reads_the_lines_written_while_closed() {
    let f = Fixture::new();
    let (dir, urls) = make_dir(1);
    let url = cache_url();
    append(&urls[0], &[f.call("m1", 30, 100), f.call("m2", 20, 200)]);
    let before = Arc::new(MinuteLedger::at(url.clone()));
    assert_eq!(
        f.reading(&mut log(&dir, &before)),
        HashMap::from([(f.minute(30), 100.0), (f.minute(20), 200.0)])
    );
    before.save(f.now, true);
    // Cooked is closed for 10 minutes; Claude Code keeps writing
    let lines: Vec<String> = (1..=10)
        .map(|n| f.call(&format!("c{n}"), 11 - n, 10))
        .collect();
    append(&urls[0], &lines);
    let after = Arc::new(MinuteLedger::at(url));
    let m = f.reading(&mut log(&dir, &after));
    assert!(m.get(&f.minute(30)) == Some(&100.0) && m.get(&f.minute(20)) == Some(&200.0)); // from the cache, not counted again
    assert!((1..=10).all(|n| m.get(&f.minute(11 - n)) == Some(&10.0)));
    assert_eq!(sum(&m), 400.0);
}

#[test]
fn the_history_fills_over_several_readings_newest_first() {
    let f = Fixture::new();
    let (dir, urls) = make_dir(4);
    for (i, url) in urls.iter().enumerate() {
        let i = i as i64;
        let lines: Vec<String> = (0..20)
            .map(|n| f.call(&format!("f{i}m{n}"), 60 * 24 * (i + 1) + 19 - n, 1))
            .collect(); // in time order, like a transcript
        append(url, &lines);
        // file 0 is the newest
        set_modified(url, f.now - (i * 3600) as f64);
    }
    let size = std::fs::metadata(&urls[0]).expect("size").len();
    let ledger = Arc::new(MinuteLedger::new(None, size + 10)); // about one file per reading
    let mut log = log(&dir, &ledger);
    let mut progress: Vec<f64> = Vec::new();
    let mut first = HashMap::new();
    for n in 0..10 {
        let m = f.reading(&mut log);
        if n == 0 {
            first = m;
        }
        let Some(p) = ledger.history_progress() else {
            break;
        };
        progress.push(p);
    }
    assert!(first.keys().all(|k| *k > f.minute(60 * 24 + 30))); // the newest file first
    assert!(progress.len() >= 3, "{progress:?}");
    let mut sorted = progress.clone();
    sorted.sort_by(f64::total_cmp);
    assert_eq!(progress, sorted);
    assert!(progress.iter().all(|p| *p < 1.0));
    assert_eq!(ledger.history_progress(), None);
    assert_eq!(sum(&f.reading(&mut log)), 80.0);
}

#[test]
fn minutes_older_than_fourteen_days_are_dropped() {
    let f = Fixture::new();
    let ledger = MinuteLedger::memory();
    let old = f.minute_now() - MinuteLedger::KEEP_MINUTES - 1;
    let kept = f.minute_now() - MinuteLedger::KEEP_MINUTES + 1;
    ledger.add("a", old, 5.0);
    ledger.add("a", kept, 7.0);
    ledger.begin();
    ledger.end(f.now);
    assert_eq!(
        ledger.minutes("a", old - 10, f.minute_now()),
        HashMap::from([(kept, 7.0)])
    );
}

#[test]
fn a_message_written_in_several_blocks_counts_once_across_readings_and_restarts() {
    let f = Fixture::new();
    let (dir, urls) = make_dir(1);
    let url = cache_url();
    append(&urls[0], &[f.call("m1", 5, 300)]);
    let ledger = Arc::new(MinuteLedger::at(url.clone()));
    let mut log1 = log(&dir, &ledger);
    let expected = HashMap::from([(f.minute(5), 300.0)]);
    assert_eq!(f.reading(&mut log1), expected);
    append(&urls[0], &[f.call("m1", 5, 300)]); // the second block, after the reading
    assert_eq!(f.reading(&mut log1), expected);
    ledger.save(f.now, true);
    append(&urls[0], &[f.call("m1", 5, 300)]); // a third block, while Cooked is closed
    assert_eq!(
        f.reading(&mut log(&dir, &Arc::new(MinuteLedger::at(url)))),
        expected
    );
}

#[test]
fn a_full_cache_loads_quickly() {
    let f = Fixture::new();
    let url = cache_url();
    let full = MinuteLedger::at(url.clone());
    // five accounts busy every minute for 14 days
    for account in ["a", "b", "c", "d", "e"] {
        for m in (f.minute_now() - MinuteLedger::KEEP_MINUTES + 1)..=f.minute_now() {
            full.add(account, m, 1234.0);
        }
    }
    full.save(f.now, true);
    let start = Instant::now();
    let loaded = MinuteLedger::at(url);
    let seconds = start.elapsed().as_secs_f64();
    assert_eq!(
        loaded.minutes("e", f.minute_now(), f.minute_now()),
        HashMap::from([(f.minute_now(), 1234.0)])
    );
    assert!(seconds < 2.0, "loaded in {seconds} s");
}

/// After `/compact`, Claude Code writes copies of the latest calls again, with the same id and time. Copies of
/// calls older than the 1-hour keys must not count again, in a later reading or after a restart.
#[test]
fn compact_copies_of_old_calls_do_not_count_again() {
    let f = Fixture::new();
    let (dir, urls) = make_dir(1);
    let url = cache_url();
    append(&urls[0], &[f.call("m1", 180, 100), f.call("m2", 120, 200)]);
    let before = Arc::new(MinuteLedger::at(url.clone()));
    assert_eq!(sum(&f.reading(&mut log(&dir, &before))), 300.0);
    before.save(f.now, true);
    // the compaction copies m1 and m2, then a new call
    append(
        &urls[0],
        &[
            f.call("m1", 180, 100),
            f.call("m2", 120, 200),
            f.call("m3", 1, 7),
        ],
    );
    let after = Arc::new(MinuteLedger::at(url));
    let mut log2 = log(&dir, &after);
    let m = f.reading(&mut log2);
    assert_eq!(
        m,
        HashMap::from([
            (f.minute(180), 100.0),
            (f.minute(120), 200.0),
            (f.minute(1), 7.0)
        ])
    );
    // a second compaction in the same running app
    append(
        &urls[0],
        &[
            f.call("m2", 120, 200),
            f.call("m3", 1, 7),
            f.call("m4", 0, 5),
        ],
    );
    assert_eq!(sum(&f.reading(&mut log2)), 312.0);
}

/// A cache from before this fix is dropped, so the 14 days are read again without the copies.
#[test]
fn an_old_cache_is_read_again() {
    let url = cache_url();
    std::fs::write(
        &url,
        r#"{"version":1,"minutes":{"a":{"100":5}},"marks":{},"historyDone":true}"#,
    )
    .expect("write");
    assert!(MinuteLedger::at(url).history_progress().is_some());
}

/// A file seen for the first time is read from 14 days back. A `/compact` copy with an older time, after newer
/// calls, must not move the start past those calls.
#[test]
fn an_old_copy_does_not_move_the_start_past_newer_calls() {
    let f = Fixture::new();
    let (_, urls) = make_dir(1);
    let days = MinuteLedger::KEEP_MINUTES;
    let mut all: Vec<String> = (0..40)
        .map(|n| f.call(&format!("o{n}"), days + 200 - n, 1))
        .collect();
    all.extend((0..40).map(|n| f.call(&format!("n{n}"), days - 100 + 40 - n, 1)));
    all.push(f.call("o0", days + 200, 1));
    all.push(f.call("z", 1, 1));
    append(&urls[0], &all);
    let data = std::fs::read(&urls[0]).expect("read");
    let needle = br#""id":"n0""#;
    let first_new = data
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("n0") as u64;
    // with some block sizes the copy is the first time in a block
    for block in (300..=3000).step_by(7) {
        let start = TranscriptLog::start(&urls[0], MinuteLedger::since(f.now), block);
        assert!(start <= first_new, "block {block}");
    }
}

/// A line cut mid-write (Claude Code still appending) is skipped, and the next reading counts it.
#[test]
fn half_written_line_is_skipped_then_read() {
    let f = Fixture::new();
    let (dir, urls) = make_dir(1);
    let ledger = Arc::new(MinuteLedger::memory());
    let mut log = log(&dir, &ledger);
    append(&urls[0], &[f.call("m1", 3, 100)]);
    let before = HashMap::from([(f.minute(3), 100.0)]);
    assert_eq!(f.reading(&mut log), before);
    let line = f.call("m2", 2, 50);
    let (head, tail) = line.split_at(line.len() / 2);
    append_raw(&urls[0], head);
    assert_eq!(f.reading(&mut log), before); // no new entry
    append_raw(&urls[0], &format!("{tail}\n"));
    assert_eq!(
        f.reading(&mut log),
        HashMap::from([(f.minute(3), 100.0), (f.minute(2), 50.0)])
    );
}
