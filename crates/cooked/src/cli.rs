//! The command line, as `CookedApp.swift` reads it: one parser for every flag. The headless commands (`--limits`,
//! `--statusline`) print and exit before any window; the rest change how the app starts.
//!
//! Rune's tooling flags (`--snap`, `--tree`, `--scene`, ...) are Rune's: in a `tooling` build Rune takes them from
//! the same command line, so no Cooked flag may share a name (`cli_flags_disjoint_from_rune_tooling`). Arguments this
//! parser does not know are left alone.

use std::collections::HashSet;
use std::io::{self, Write};
use std::time::Duration;

use cooked_model::accounts::{AccountStore, ConnectedAccount, Kind};
use cooked_model::clock;
use cooked_model::demo_data::Feed;
use cooked_model::notice::Notice;
use cooked_model::status_file::StatusFile;
use cooked_model::status_line_setup::{Failure, State, StatusLineSetup};

use crate::mac::LocalZone;
use crate::mac::defaults::UserDefaults;
use crate::scheduler;

/// `--limits`: every account's limits and alert line as the island reads them, one line each.
pub const LIMITS: &str = "--limits";
/// `--statusline status|connect|disconnect`: the status line bridge of each connected Claude account's settings.
pub const STATUSLINE: &str = "--statusline";
/// `--feed demoCalm|demoPeak|live`: start on that feed.
pub const FEED: &str = "--feed";
/// `--open`: start with the popover open, pinned until a key.
pub const OPEN: &str = "--open";
/// `--setup`: the Setup window at launch, also when Setup is done.
pub const SETUP: &str = "--setup";
/// `--settings`: the Settings window at launch.
pub const SETTINGS: &str = "--settings";
/// `--reduce-motion`: still metal, whatever the system setting says.
pub const REDUCE_MOTION: &str = "--reduce-motion";
/// `--toggle-every <s>`: opens and closes the popover on a timer, to record the transitions without clicking.
pub const TOGGLE_EVERY: &str = "--toggle-every";
/// `--probe-item-click <s>` (feature `tooling`): the menu bar item's click timed by the app itself (`app.rs`).
pub const PROBE_ITEM_CLICK: &str = "--probe-item-click";

/// Every Cooked flag, for the test against Rune's.
#[cfg(test)]
pub const FLAGS: &[&str] = &[
    LIMITS,
    STATUSLINE,
    FEED,
    OPEN,
    SETUP,
    SETTINGS,
    REDUCE_MOTION,
    TOGGLE_EVERY,
    PROBE_ITEM_CLICK,
];

/// What the command line asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Print the limits and exit.
    Limits,
    /// Print (and change) the status line bridges and exit.
    StatusLine(StatusLineAction),
    /// Run the app.
    App(Launch),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLineAction {
    Status,
    Connect,
    Disconnect,
}

/// How the app starts. `Default`: no flags.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Launch {
    pub feed: Option<Feed>,
    pub open: bool,
    pub setup: bool,
    pub settings: bool,
    pub reduce_motion: bool,
    pub toggle_every: Option<Duration>,
    pub probe_item_click: Option<Duration>,
}

/// `args` as `std::env::args()` gives them, the program first. As Swift: `--limits` wins over `--statusline`, which
/// wins over the app; a flag's missing or unreadable value is as if the flag were absent; `--statusline` with no
/// action, or one it does not know, is `status`.
pub fn parse(args: &[String]) -> Command {
    let rest = args.get(1..).unwrap_or_default();
    let has = |flag: &str| rest.iter().any(|a| a == flag);
    let value = |flag: &str| {
        let i = rest.iter().position(|a| a == flag)?;
        rest.get(i + 1).map(String::as_str)
    };
    let seconds = |flag: &str| {
        value(flag)?
            .parse::<f64>()
            .ok()
            .filter(|s| s.is_finite() && *s > 0.0)
            .map(Duration::from_secs_f64)
    };
    if has(LIMITS) {
        return Command::Limits;
    }
    if has(STATUSLINE) {
        return Command::StatusLine(match value(STATUSLINE) {
            Some("connect") => StatusLineAction::Connect,
            Some("disconnect") => StatusLineAction::Disconnect,
            _ => StatusLineAction::Status,
        });
    }
    Command::App(Launch {
        feed: value(FEED).and_then(Feed::from_raw),
        open: has(OPEN),
        setup: has(SETUP),
        settings: has(SETTINGS),
        reduce_motion: has(REDUCE_MOTION),
        toggle_every: seconds(TOGGLE_EVERY),
        probe_item_click: seconds(PROBE_ITEM_CLICK),
    })
}

/// Runs a headless command with the app's settings, home and folders; its exit code. `None` for `App`.
pub fn run_headless(command: &Command) -> Option<i32> {
    let mut out = io::stdout().lock();
    match command {
        Command::App(_) => None,
        Command::Limits => Some(limits(&mut out)),
        Command::StatusLine(action) => {
            let mut defaults = UserDefaults::app();
            let home = std::env::var("HOME").unwrap_or_default();
            let accounts = AccountStore::current(&mut defaults, &home);
            Some(status_line(
                *action,
                &accounts,
                &StatusLineSetup::default(),
                &mut out,
            ))
        }
    }
}

/// `--limits`: reads until the history is complete, then prints what the island would show.
fn limits(out: &mut dyn Write) -> i32 {
    let mut defaults = UserDefaults::app();
    let home = std::env::var("HOME").unwrap_or_default();
    let accounts = AccountStore::current(&mut defaults, &home);
    let mut source = scheduler::live_source(accounts);
    let zone = LocalZone::new();
    let mut reading = source.read(clock::now(), &zone);
    while reading.history.is_some() {
        reading = source.read(clock::now(), &zone);
    }
    let ledger = std::sync::Arc::clone(&source.ledger);
    let buckets = move || ledger.buckets15();
    let status = source.limits.read(
        clock::now(),
        cooked_model::limit_log::LimitInput {
            buckets: Some(&buckets),
            ..Default::default()
        },
        &zone,
    );
    for line in limits_lines(&status, &reading.notices) {
        let _ = writeln!(out, "{line}");
    }
    0
}

/// The lines `--limits` prints, as Swift's `print` writes them (items joined by a space, so an account with no
/// note ends in a space).
pub fn limits_lines(status: &StatusFile, notices: &[Notice]) -> Vec<String> {
    let text = |s: &Option<String>| s.clone().unwrap_or_else(|| "-".to_owned());
    let mut lines: Vec<String> = status
        .accounts
        .iter()
        .map(|(id, e)| {
            let eta = e
                .projection
                .as_ref()
                .map_or_else(|| "-".to_owned(), |p| format!("{} ({})", p.eta, p.window));
            [
                id.clone(),
                format!(
                    "5h {}% reset {}",
                    e.five_hour_pct.unwrap_or(-1),
                    text(&e.five_hour_resets_at)
                ),
                format!(
                    "week {}% reset {}",
                    e.seven_day_pct.unwrap_or(-1),
                    text(&e.seven_day_resets_at)
                ),
                format!(
                    "status {} last {} live {}",
                    text(&e.limit_status),
                    text(&e.last_event_at),
                    e.live.unwrap_or(false)
                ),
                format!("eta {eta} alert {}", e.spike_threshold_15m.unwrap_or(-1)),
                e.note.clone().unwrap_or_default(),
            ]
            .join(" ")
        })
        .collect();
    lines.push(format!(
        "all accounts: alert {}",
        status.total_threshold_15m.unwrap_or(-1)
    ));
    lines.extend(notices.iter().map(|n| format!("notice: {}", n.title())));
    lines
}

/// Swift's `print` of a `StatusLineSetup.State`: the case's name.
fn state_name(state: State) -> &'static str {
    match state {
        State::NotConnected => "notConnected",
        State::Connected => "connected",
        State::Changed => "changed",
    }
}

/// Swift's `print` of the error: the case's name for `notAnObject`.
fn failure_text(failure: &Failure) -> String {
    match failure {
        Failure::NotAnObject => "notAnObject".to_owned(),
        Failure::Io(e) => e.to_string(),
    }
}

/// `--statusline`: each Claude account's settings file once (several accounts can share one), changed as `action`
/// asks, then `<id> <real path> <state>` per account. A failure prints `<real path> <error>` and stops with 1.
pub fn status_line(
    action: StatusLineAction,
    accounts: &[ConnectedAccount],
    setup: &StatusLineSetup,
    out: &mut dyn Write,
) -> i32 {
    let mut done = HashSet::new();
    for account in accounts.iter().filter(|a| a.kind() == Kind::Claude) {
        let settings = format!("{}/settings.json", account.path);
        let real = StatusLineSetup::real(&settings);
        if done.insert(real.clone()) {
            let result = match action {
                StatusLineAction::Connect => setup.connect(&settings),
                StatusLineAction::Disconnect => setup.disconnect(&settings).map(|given| {
                    let said = if given { "given back" } else { "not touched" };
                    let _ = writeln!(out, "{said} {real}");
                }),
                StatusLineAction::Status => Ok(()),
            };
            if let Err(e) = result {
                let _ = writeln!(out, "{real} {}", failure_text(&e));
                return 1;
            }
        }
        let state = state_name(setup.state(&settings));
        let _ = writeln!(out, "{} {real} {state}", account.id);
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use cooked_model::status_file::{Entry, Projection};
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    fn args(a: &[&str]) -> Vec<String> {
        std::iter::once("cooked")
            .chain(a.iter().copied())
            .map(String::from)
            .collect()
    }

    fn launch(a: &[&str]) -> Launch {
        match parse(&args(a)) {
            Command::App(launch) => launch,
            other => panic!("not the app: {other:?}"),
        }
    }

    #[test]
    fn no_flags_run_the_app_as_is() {
        assert_eq!(parse(&args(&[])), Command::App(Launch::default()));
        assert_eq!(parse(&[]), Command::App(Launch::default()));
    }

    #[test]
    fn app_flags() {
        let l = launch(&[
            "--feed",
            "demoPeak",
            "--open",
            "--setup",
            "--settings",
            "--reduce-motion",
            "--toggle-every",
            "1.5",
            "--probe-item-click",
            "2",
        ]);
        assert_eq!(l.feed, Some(Feed::DemoPeak));
        assert!(l.open && l.setup && l.settings && l.reduce_motion);
        assert_eq!(l.toggle_every, Some(Duration::from_millis(1500)));
        assert_eq!(l.probe_item_click, Some(Duration::from_secs(2)));
    }

    #[test]
    fn a_bad_value_is_no_flag() {
        assert_eq!(launch(&["--feed"]).feed, None);
        assert_eq!(launch(&["--feed", "nope"]).feed, None);
        assert_eq!(launch(&["--feed", "live"]).feed, Some(Feed::Live));
        assert_eq!(launch(&["--toggle-every", "x"]).toggle_every, None);
        assert_eq!(launch(&["--toggle-every", "0"]).toggle_every, None);
        assert_eq!(launch(&["--toggle-every", "-1"]).toggle_every, None);
        assert_eq!(launch(&["--toggle-every", "inf"]).toggle_every, None);
    }

    #[test]
    fn the_program_name_is_no_flag() {
        assert_eq!(
            parse(&["--limits".to_owned()]),
            Command::App(Launch::default())
        );
    }

    #[test]
    fn headless_commands_win_as_in_swift() {
        assert_eq!(parse(&args(&["--open", "--limits"])), Command::Limits);
        assert_eq!(
            parse(&args(&["--statusline", "connect", "--limits"])),
            Command::Limits
        );
        let action = |a: &[&str]| parse(&args(a));
        assert_eq!(
            action(&["--statusline"]),
            Command::StatusLine(StatusLineAction::Status)
        );
        assert_eq!(
            action(&["--statusline", "status"]),
            Command::StatusLine(StatusLineAction::Status)
        );
        assert_eq!(
            action(&["--statusline", "connect"]),
            Command::StatusLine(StatusLineAction::Connect)
        );
        assert_eq!(
            action(&["--statusline", "disconnect", "--open"]),
            Command::StatusLine(StatusLineAction::Disconnect)
        );
        assert_eq!(
            action(&["--statusline", "other"]),
            Command::StatusLine(StatusLineAction::Status)
        );
    }

    #[test]
    fn rune_flags_are_left_alone() {
        let l = launch(&["--snap", "s.png", "--scene", "island-urgent", "--open"]);
        assert_eq!(
            l,
            Launch {
                open: true,
                ..Default::default()
            }
        );
    }

    /// Rune's tooling flags at the pinned rev, copied from the arms of `Args::parse_split` in
    /// `crates/rune/src/tooling/args.rs` (Rune keeps them private). `rune_flag_list_matches_the_source` checks the
    /// copy against that file when the checkout is on disk; update it with each Rune upgrade.
    const RUNE_FLAGS: &[&str] = &[
        "--hot",
        "--snap-on-patch",
        "--hot-control",
        "--snap",
        "--scene",
        "--playground",
        "--variant",
        "--whole-window",
        "--settle",
        "--at",
        "--size",
        "--scale",
        "--theme",
        "--scroll-bars",
        "--script",
        "--frames",
        "--start",
        "--fps",
        "--out",
        "--video",
        "--duration",
        "--keys",
        "--keys-theme",
        "--pointer",
        "--desktop",
        "--tree",
        "--html",
        "--html-template",
        "--html-widths",
        "--trace",
        "--native",
        "--realtime",
        "--window-script",
        "--trace-all",
        "--strip",
        "--crop",
        "--cols",
        "--every",
        "--list-scenes",
        "--first-pixels",
        "--window",
        "--list-windows",
        "--chrome",
        "--cpu-boot",
        "--cpu-boot-threads",
        "-h",
        "--help",
    ];
    /// The prefix of the argument Rune adds when it relaunches an instance (`open_url/instance.rs`).
    const RUNE_PREFIX: &str = "--rune-";
    /// The pinned rev's short form, as Cargo names its checkout folder.
    const RUNE_REV: &str = "09cf4b9";

    #[test]
    fn cli_flags_disjoint_from_rune_tooling() {
        let ours: HashSet<&str> = FLAGS.iter().copied().collect();
        assert_eq!(ours.len(), FLAGS.len(), "a flag is listed twice");
        for flag in FLAGS {
            assert!(!RUNE_FLAGS.contains(flag), "{flag} is a Rune tooling flag");
            assert!(!flag.starts_with(RUNE_PREFIX), "{flag} looks like Rune's");
        }
    }

    /// Rune's flags as the pinned checkout's parser has them, or `None` without the checkout.
    fn rune_source_flags() -> Option<HashSet<String>> {
        let cargo = std::env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".cargo")))?;
        let checkouts = cargo.join("git/checkouts");
        let source = std::fs::read_dir(&checkouts)
            .ok()?
            .flatten()
            .filter(|d| d.file_name().to_string_lossy().starts_with("rune-"))
            .map(|d| {
                d.path()
                    .join(RUNE_REV)
                    .join("crates/rune/src/tooling/args.rs")
            })
            .find(|p| p.exists())?;
        let text = std::fs::read_to_string(source).ok()?;
        let start = text.find("fn parse_split")?;
        let end = start + text[start..].find("fn validate")?;
        let body = &text[start..end];
        let mut flags = HashSet::new();
        for part in body.split('"').skip(1).step_by(2) {
            let is_flag = part.starts_with('-')
                && part.len() > 1
                && part[1..]
                    .trim_start_matches('-')
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-')
                && !part.contains(' ');
            if is_flag && !part.contains("{}") {
                flags.insert(part.to_owned());
            }
        }
        Some(flags)
    }

    #[test]
    fn rune_flag_list_matches_the_source() {
        let Some(source) = rune_source_flags() else {
            eprintln!("Rune {RUNE_REV} checkout not found: the copied list is not checked");
            return;
        };
        let copied: HashSet<String> = RUNE_FLAGS.iter().map(|s| s.to_string()).collect();
        assert_eq!(source, copied);
    }

    #[test]
    fn limits_lines_print_as_swift() {
        let mut accounts = BTreeMap::new();
        accounts.insert(
            "claude".to_owned(),
            Entry {
                five_hour_pct: Some(42),
                five_hour_resets_at: Some("18:30".to_owned()),
                seven_day_pct: Some(7),
                seven_day_resets_at: Some("28/11 09:00".to_owned()),
                limit_status: Some("allowed".to_owned()),
                last_event_at: Some("17:01".to_owned()),
                live: Some(true),
                projection: Some(Projection {
                    window: "5h".to_owned(),
                    eta: "18:05".to_owned(),
                }),
                spike_threshold_15m: Some(1200),
                ..Default::default()
            },
        );
        accounts.insert(
            "codex".to_owned(),
            Entry {
                note: Some("5h window reset, no new data".to_owned()),
                ..Default::default()
            },
        );
        let status = StatusFile {
            accounts,
            total_threshold_15m: Some(3000),
            ..Default::default()
        };
        assert_eq!(
            limits_lines(&status, &[Notice::Paused]),
            [
                "claude 5h 42% reset 18:30 week 7% reset 28/11 09:00 status allowed last 17:01 live true \
                 eta 18:05 (5h) alert 1200 ",
                "codex 5h -1% reset - week -1% reset - status - last - live false eta - alert -1 \
                 5h window reset, no new data",
                "all accounts: alert 3000",
                "notice: Claude percent paused",
            ]
        );
        assert_eq!(
            limits_lines(&StatusFile::default(), &[]),
            ["all accounts: alert -1"]
        );
    }

    /// A scratch folder with one Claude account (`settings.json` holding a status line) and Cooked's support
    /// folder; never the real home.
    struct Scratch {
        root: PathBuf,
        setup: StatusLineSetup,
        accounts: Vec<ConnectedAccount>,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "cooked-cli-{name}-{}-{}",
                std::process::id(),
                clock::now()
            ));
            let claude = root.join(".claude");
            std::fs::create_dir_all(&claude).unwrap();
            std::fs::write(
                claude.join("settings.json"),
                "{\n  \"theme\": \"dark\",\n  \"statusLine\": {\"type\": \"command\", \"command\": \"echo hi\"}\n}\n",
            )
            .unwrap();
            let root = PathBuf::from(StatusLineSetup::real(&root.to_string_lossy()));
            let claude = root.join(".claude").to_string_lossy().into_owned();
            let setup = StatusLineSetup::new(root.join("support").to_string_lossy());
            let accounts = vec![
                ConnectedAccount::new("claude", "Claude", claude.clone()),
                // A second account sharing the same settings file: changed once, listed twice.
                ConnectedAccount::new("claude-2", "Work", claude),
                ConnectedAccount::new("codex", "Codex", root.join(".codex").to_string_lossy()),
            ];
            assert!(
                accounts
                    .iter()
                    .all(|a| a.path.starts_with(&*root.to_string_lossy())),
                "the scratch accounts stay in the scratch folder"
            );
            Self {
                root,
                setup,
                accounts,
            }
        }

        fn settings(&self) -> String {
            format!("{}/.claude/settings.json", self.root.display())
        }

        fn run(&self, action: StatusLineAction) -> (i32, String) {
            let mut out = Vec::new();
            let code = status_line(action, &self.accounts, &self.setup, &mut out);
            (code, String::from_utf8(out).unwrap())
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn statusline_connects_and_gives_back_in_a_scratch_home() {
        let s = Scratch::new("cycle");
        let settings = s.settings();
        let before = std::fs::read_to_string(&settings).unwrap();
        let ids = |state: &str| format!("claude {settings} {state}\nclaude-2 {settings} {state}\n");

        assert_eq!(s.run(StatusLineAction::Status), (0, ids("notConnected")));
        assert_eq!(s.run(StatusLineAction::Connect), (0, ids("connected")));
        let connected = std::fs::read_to_string(&settings).unwrap();
        assert!(connected.contains(&s.setup.script()), "{connected}");
        assert!(connected.contains("\"theme\": \"dark\""));
        assert_eq!(s.run(StatusLineAction::Status), (0, ids("connected")));
        assert_eq!(
            s.run(StatusLineAction::Disconnect),
            (0, format!("given back {settings}\n{}", ids("notConnected")))
        );
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), before);
        assert_eq!(
            s.run(StatusLineAction::Disconnect),
            (
                0,
                format!("not touched {settings}\n{}", ids("notConnected"))
            )
        );
    }

    #[test]
    fn statusline_stops_on_a_file_it_cannot_write() {
        let s = Scratch::new("bad");
        std::fs::write(s.settings(), "[1, 2]").unwrap();
        let settings = s.settings();
        assert_eq!(
            s.run(StatusLineAction::Connect),
            (1, format!("{settings} notAnObject\n"))
        );
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), "[1, 2]");
    }
}
