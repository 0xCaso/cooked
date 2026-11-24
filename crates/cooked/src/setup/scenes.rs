//! The mockup's states as headless scenes (`--snap`, `--tree`), over a home made in a temporary folder: three Claude
//! folders reading one `settings.json` through symlinks, a fourth unticked, and Codex. Nothing outside that folder is
//! read or written. The clock stops at Wed 4 Nov 2026 16:58 UTC, the mockup's.

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cooked_model::accounts::{self, AccountStore, ConnectedAccount};
use cooked_model::clock::{self, FixedOffset};
use cooked_model::defaults::MemoryDefaults;
use cooked_model::demo_data;
use cooked_model::island_model::IslandState;
use cooked_model::setup_model::{SetupModel, Step};
use cooked_model::status_line_setup::StatusLineSetup;
use rune::{App, AppSpec};

use super::{Login, Mode, Setup, Tab, WIDTH, view, window_app};
use crate::island::{Prefs, shared};

pub const SCENES: &[&str] = &[
    "setup-accounts",
    "setup-percent",
    "setup-login",
    "settings-connected",
    "settings-off",
    "settings-changed",
    "settings-remove",
    "settings-general",
];

/// The status line the user had: Cooked keeps it running under the bridge.
pub const OLD_COMMAND: &str = "sh ~/bin/statusline.sh";
/// What replaced the bridge in the changed state.
pub const NEW_COMMAND: &str = "npx claude-hud";

/// Wed 4 Nov 2026 16:58 UTC.
pub fn now() -> f64 {
    at(2026, 11, 4, 16, 58)
}

fn at(year: i64, month: i64, day: i64, hour: i64, minute: i64) -> f64 {
    (clock::days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60) as f64
}

fn time(t: f64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs_f64(t)
}

/// A file with its change time at `t`.
fn touch(path: &str, t: f64) {
    if let Some(dir) = Path::new(path).parent() {
        fs::create_dir_all(dir).expect("scene folder");
    }
    let file = fs::File::create(path).expect("scene file");
    file.set_modified(time(t)).expect("scene file time");
}

/// A Claude folder of the scene's home.
struct Folder {
    folder: &'static str,
    name: &'static str,
    /// The last session: month, day, hour, minute of 2026.
    last: (i64, i64, i64, i64),
    ticked: bool,
}

const FOLDERS: &[Folder] = &[
    Folder {
        folder: ".claude",
        name: "personal",
        last: (11, 4, 11, 50),
        ticked: true,
    },
    Folder {
        folder: ".claude-work",
        name: "work",
        last: (10, 31, 10, 2),
        ticked: true,
    },
    Folder {
        folder: ".claude-team",
        name: "team",
        last: (11, 4, 15, 55),
        ticked: true,
    },
    Folder {
        folder: ".claude-side",
        name: "side",
        last: (10, 15, 9, 30),
        ticked: false,
    },
];

/// Makes the scene's home under the temporary folder, fresh, and returns its path.
pub fn home(name: &str) -> String {
    let home = std::env::temp_dir().join(format!("cooked-setup-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).expect("scene home");
    let home = fs::canonicalize(&home)
        .expect("scene home")
        .to_string_lossy()
        .into_owned();
    for f in FOLDERS {
        let (m, d, h, min) = f.last;
        touch(
            &format!("{home}/{}/projects/session.jsonl", f.folder),
            at(2026, m, d, h, min),
        );
    }
    touch(
        &format!("{home}/.codex/sessions/2026/10/31/rollout.jsonl"),
        at(2026, 10, 31, 18, 5),
    );
    let settings = format!("{home}/.claude/settings.json");
    fs::write(
        &settings,
        format!(
            "{{\n  \"model\": \"opus\",\n  \"statusLine\": {{\n    \"type\": \"command\",\n    \"command\": \"{OLD_COMMAND}\"\n  }}\n}}\n"
        ),
    )
    .expect("scene settings");
    for f in &FOLDERS[1..] {
        std::os::unix::fs::symlink(&settings, format!("{home}/{}/settings.json", f.folder))
            .expect("scene link");
    }
    home
}

/// The saved accounts: the ticked folders with their names, Codex last.
fn saved(home: &str) -> Vec<ConnectedAccount> {
    let mut out: Vec<ConnectedAccount> = FOLDERS
        .iter()
        .filter(|f| f.ticked)
        .map(|f| {
            ConnectedAccount::new(
                accounts::id(f.folder),
                f.name,
                format!("{home}/{}", f.folder),
            )
        })
        .collect();
    out.push(ConnectedAccount::new(
        accounts::CODEX_ID,
        "codex",
        format!("{home}/.codex"),
    ));
    out
}

/// A window over a fresh home called `name`.
pub fn setup(name: &str, mode: Mode, tab: Tab) -> Setup {
    let home = home(name);
    let make = Box::new(move || {
        let mut defaults = MemoryDefaults::default();
        AccountStore::save(&saved(&home), &mut defaults);
        let defaults: Prefs = Box::new(defaults);
        SetupModel::new(
            home.clone(),
            defaults,
            StatusLineSetup::new(format!("{home}/support")),
            Login::default(),
        )
    });
    let mut s = Setup::new(make, Box::new(FixedOffset(0)));
    s.present(mode, tab);
    s.still_at = Some(now());
    s.reduce_motion = true;
    s
}

/// Settings on the percent tab, connected.
fn connected(name: &str) -> Setup {
    let mut s = setup(name, Mode::Settings, Tab::Percent);
    s.model.connect();
    s
}

pub fn scene(name: &str) -> Setup {
    scene_in(name, name)
}

/// `scene` over a home called `tag`: tests that run beside the snapshots use their own.
pub fn scene_in(name: &str, tag: &str) -> Setup {
    match name {
        "setup-percent" => {
            let mut s = setup(tag, Mode::Setup, Tab::Accounts);
            s.model.step = Step::Percent;
            s
        }
        "setup-login" => {
            let mut s = setup(tag, Mode::Setup, Tab::Accounts);
            s.model.step = Step::Login;
            s
        }
        "settings-connected" => connected(tag),
        "settings-off" => {
            let mut s = setup(tag, Mode::Settings, Tab::Percent);
            s.open.insert("what");
            s
        }
        "settings-changed" => {
            let mut s = connected(tag);
            let settings = format!("{}/.claude/settings.json", s.model.home);
            let text = fs::read_to_string(&settings).unwrap_or_default();
            let bridge = s
                .model
                .settings_files()
                .first()
                .map(|f| s.model.status_line.command(&StatusLineSetup::key(&f.real)))
                .unwrap_or_default();
            let bridge = cooked_model::status_line_setup::json_string(&bridge);
            fs::write(
                &settings,
                text.replace(&bridge, &format!("\"{NEW_COMMAND}\"")),
            )
            .expect("scene settings");
            s.model.refresh();
            s.open.insert("details");
            s
        }
        "settings-remove" => {
            let mut s = connected(tag);
            s.confirming = true;
            s
        }
        "settings-general" => {
            let mut s = setup(tag, Mode::Settings, Tab::General);
            let prefs: Prefs = Box::<MemoryDefaults>::default();
            s.island = Some(shared(IslandState::new(
                demo_data::calm(),
                None,
                prefs,
                0.0,
            )));
            s
        }
        _ => setup(tag, Mode::Setup, Tab::Accounts),
    }
}

/// Before a scene replaces it: nothing on disk.
fn init() -> Setup {
    Setup::blank()
}

pub fn app() -> App<Setup> {
    window_app(AppSpec {
        scenes: &[
            ("setup-accounts", |s: &mut Setup| {
                *s = scene("setup-accounts")
            }),
            ("setup-percent", |s: &mut Setup| *s = scene("setup-percent")),
            ("setup-login", |s: &mut Setup| *s = scene("setup-login")),
            ("settings-connected", |s: &mut Setup| {
                *s = scene("settings-connected")
            }),
            ("settings-off", |s: &mut Setup| *s = scene("settings-off")),
            ("settings-changed", |s: &mut Setup| {
                *s = scene("settings-changed")
            }),
            ("settings-remove", |s: &mut Setup| {
                *s = scene("settings-remove")
            }),
            ("settings-general", |s: &mut Setup| {
                *s = scene("settings-general")
            }),
        ],
        ..AppSpec::new("Set up Cooked", (WIDTH, 400.0), init, view)
    })
}
