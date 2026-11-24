//! Port of `Model/SetupModel.swift` (and `LastUse` from `Views/SetupViews.swift`): Setup and Settings (release spec,
//! section 3): which accounts Cooked reads and their names, the Claude percent through the status line bridge, and
//! open at login. Nothing outside Cooked's own folder is written before Allow or Connect. The login item
//! (`SMAppService.mainApp`) is a trait: the app's `mac` module has the real one.

use crate::accounts::{self, AccountStore, ConnectedAccount, Found, Kind};
use crate::clock::{self, TimeZone};
use crate::defaults::DefaultsStore;
use crate::live_snapshot;
use crate::status_line_bridge;
use crate::status_line_setup::{Failure, Saved, State, StatusLineSetup};
use std::fs;
use std::path::Path;

/// Open at login: `SMAppService.mainApp` in the app, a fake in tests.
pub trait LoginItem {
    fn is_enabled(&self) -> bool;
    /// Outside a bundle registering fails: Setup goes on without it. The error, in one line.
    fn set(&mut self, on: bool) -> Result<(), String>;
}

/// One found folder, as the Accounts step shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub found: Found,
    pub ticked: bool,
    pub name: String,
}

impl Row {
    pub fn id(&self) -> &str {
        &self.found.id
    }
}

/// A ticked Claude account that reads a settings file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reader {
    pub name: String,
    /// `~/.claude-work/settings.json`, as the account sees it.
    pub path: String,
    pub is_link: bool,
}

/// One real `settings.json` and the ticked Claude accounts that read it (several, through symlinks).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsFile {
    pub real: String,
    pub readers: Vec<Reader>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Step {
    Accounts,
    Percent,
    Login,
}

impl Step {
    pub const ALL: [Step; 3] = [Step::Accounts, Step::Percent, Step::Login];

    pub fn title(self) -> &'static str {
        match self {
            Step::Accounts => "Accounts",
            Step::Percent => "Claude percent",
            Step::Login => "Open at login",
        }
    }
}

pub const DONE_KEY: &str = "setupDone";

pub fn is_done(defaults: &dyn DefaultsStore) -> bool {
    defaults.get_bool(DONE_KEY)
}

/// Saved accounts keep their tick and name; with nothing saved yet, every folder is ticked with its default name.
/// A saved account whose folder is gone drops out.
pub fn rows(found: &[Found], saved: Option<&[ConnectedAccount]>) -> Vec<Row> {
    found
        .iter()
        .map(|f| match saved {
            None => Row {
                found: f.clone(),
                ticked: true,
                name: accounts::default_name(f),
            },
            Some(saved) => {
                let matched = saved.iter().find(|a| a.id == f.id);
                Row {
                    found: f.clone(),
                    ticked: matched.is_some(),
                    name: matched.map_or_else(|| accounts::default_name(f), |a| a.name.clone()),
                }
            }
        })
        .collect()
}

/// The Remove confirmation's and the settings' error text.
pub fn message(error: &Failure) -> String {
    match error {
        Failure::NotAnObject => {
            "settings.json is not a JSON object, so Cooked did not change it.".to_owned()
        }
        Failure::Io(e) => e.to_string(),
    }
}

pub struct SetupModel<D: DefaultsStore, L: LoginItem> {
    pub home: String,
    pub defaults: D,
    pub status_line: StatusLineSetup,
    pub login: L,
    pub rows: Vec<Row>,
    pub step: Step,
    /// The Open at login toggle.
    pub open_at_login: bool,
    percent: State,
    /// The last error from writing a settings file or the login item, in one line.
    problem: Option<String>,
    /// Called after the accounts change, so the island reads the new list.
    pub on_accounts_change: Option<Box<dyn FnMut()>>,
}

impl<D: DefaultsStore, L: LoginItem> SetupModel<D, L> {
    pub fn new(
        home: impl Into<String>,
        defaults: D,
        status_line: StatusLineSetup,
        login: L,
    ) -> Self {
        let home = home.into();
        let saved = AccountStore::load(&defaults);
        let rows = rows(&accounts::scan(&home), saved.as_deref());
        let open_at_login = login.is_enabled() || !is_done(&defaults);
        let mut m = Self {
            home,
            defaults,
            status_line,
            login,
            rows,
            step: Step::Accounts,
            open_at_login,
            percent: State::NotConnected,
            problem: None,
            on_accounts_change: None,
        };
        m.refresh();
        m
    }

    pub fn percent(&self) -> State {
        self.percent
    }

    pub fn problem(&self) -> Option<&str> {
        self.problem.as_deref()
    }

    /// The ticked rows, in the list's order; an empty name falls back to the default one.
    pub fn accounts(&self) -> Vec<ConnectedAccount> {
        self.rows
            .iter()
            .filter(|r| r.ticked)
            .map(|r| {
                // Swift's `.whitespaces`: spaces and tabs, not line breaks
                let name = r.name.trim_matches(|c: char| {
                    c == '\t'
                        || (c.is_whitespace()
                            && !matches!(
                                c,
                                '\n' | '\r'
                                    | '\u{b}'
                                    | '\u{c}'
                                    | '\u{85}'
                                    | '\u{2028}'
                                    | '\u{2029}'
                            ))
                });
                let name = if name.is_empty() {
                    accounts::default_name(&r.found)
                } else {
                    name.to_owned()
                };
                ConnectedAccount::new(r.found.id.clone(), name, r.found.path.clone())
            })
            .collect()
    }

    /// The steps this Mac needs: no Claude account ticked, no status line step.
    pub fn steps(&self) -> Vec<Step> {
        if self.accounts().iter().any(|a| a.kind() == Kind::Claude) {
            Step::ALL.to_vec()
        } else {
            vec![Step::Accounts, Step::Login]
        }
    }

    pub fn settings_files(&self) -> Vec<SettingsFile> {
        let mut out: Vec<SettingsFile> = Vec::new();
        for account in self
            .accounts()
            .into_iter()
            .filter(|a| a.kind() == Kind::Claude)
        {
            let path = format!("{}/settings.json", account.path);
            let real = StatusLineSetup::real(&path);
            let reader = Reader {
                name: account.name.clone(),
                path: self.tilde(&path),
                is_link: fs::read_link(&path).is_ok(),
            };
            match out.iter_mut().find(|f| f.real == real) {
                Some(file) => file.readers.push(reader),
                None => out.push(SettingsFile {
                    real,
                    readers: vec![reader],
                }),
            }
        }
        out
    }

    /// `/Users/x/.claude` → `~/.claude`.
    pub fn tilde(&self, path: &str) -> String {
        match path.strip_prefix(&format!("{}/", self.home)) {
            Some(rest) => format!("~/{rest}"),
            None => path.to_owned(),
        }
    }

    /// Changed wins (the percent is paused somewhere), then connected, then not connected.
    pub fn refresh(&mut self) {
        let states: Vec<State> = self
            .settings_files()
            .iter()
            .map(|f| self.status_line.state(&f.real))
            .collect();
        self.percent = if states.contains(&State::Changed) {
            State::Changed
        } else if states.contains(&State::Connected) {
            State::Connected
        } else {
            State::NotConnected
        };
    }

    fn done(&mut self, result: Result<(), String>) {
        self.problem = result.err();
        self.refresh();
    }

    // MARK: - Actions

    pub fn save_accounts(&mut self) {
        AccountStore::save(&self.accounts(), &mut self.defaults);
        if let Some(f) = self.on_accounts_change.as_mut() {
            f();
        }
        self.refresh();
    }

    /// Allow and Connect (again): every real settings file the ticked Claude accounts read.
    pub fn connect(&mut self) {
        let result = self
            .settings_files()
            .iter()
            .try_for_each(|f| self.status_line.connect(&f.real))
            .map_err(|e| message(&e));
        self.done(result);
    }

    pub fn disconnect(&mut self) {
        let result = self
            .settings_files()
            .iter()
            .try_for_each(|f| self.status_line.disconnect(&f.real).map(|_| ()))
            .map_err(|e| message(&e));
        self.done(result);
    }

    pub fn set_open_at_login(&mut self, on: bool) {
        self.open_at_login = on;
        let result = self.login.set(on);
        self.done(result);
    }

    /// Setup's last step: Done.
    pub fn finish(&mut self) {
        self.save_accounts();
        self.set_open_at_login(self.open_at_login);
        self.defaults.set_bool(DONE_KEY, true);
    }

    /// The step after `step`, or `None` after the last.
    pub fn next(&self, step: Step) -> Option<Step> {
        self.steps().into_iter().find(|s| *s > step)
    }

    pub fn back(&self, step: Step) -> Option<Step> {
        self.steps().into_iter().rev().find(|s| *s < step)
    }

    /// Undoes all Cooked changed: the old status lines come back, Cooked stops opening at login, and the bridge script
    /// goes. The `settings.json.cooked-backup` copies stay.
    pub fn remove_all(&mut self) {
        let result = self.undo();
        self.done(result);
    }

    fn undo(&mut self) -> Result<(), String> {
        for file in self.settings_files() {
            self.status_line
                .disconnect(&file.real)
                .map_err(|e| message(&e))?;
        }
        // Accounts unticked since connecting: their saved lines too.
        let dir = self.status_line.saved_dir();
        let names: Vec<String> = fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok()?.file_name().into_string().ok())
                    .filter(|n| n.ends_with(".json"))
                    .collect()
            })
            .unwrap_or_default();
        for name in names {
            let saved = fs::read(format!("{dir}/{name}"))
                .ok()
                .and_then(|data| serde_json::from_slice::<Saved>(&data).ok());
            if let Some(saved) = saved {
                self.status_line
                    .disconnect(&saved.settings)
                    .map_err(|e| message(&e))?;
            }
        }
        self.login.set(false)?;
        self.open_at_login = false;
        let script = self.status_line.script();
        if Path::new(&script).exists() {
            fs::remove_file(&script).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// The Remove confirmation, with this Mac's numbers.
    pub fn remove_sentence(&self) -> String {
        let n: usize = self.settings_files().iter().map(|f| f.readers.len()).sum();
        let place = match n {
            1 => String::new(),
            2 => " in both accounts".to_owned(),
            n => format!(" in all {n} accounts"),
        };
        format!(
            "Your old status line comes back{place}, Cooked stops opening at login, and the bridge script is deleted. The backup of settings.json stays."
        )
    }

    // MARK: - Details

    /// The newest bridge sample of a ticked account: its name, percent and time (Unix seconds).
    pub fn last_terminal_percent(&self) -> Option<(String, i64, f64)> {
        let samples = status_line_bridge::samples(Path::new(&self.status_line.bridge_dir()));
        let mut best: Option<(String, i64, f64)> = None;
        for a in self.accounts() {
            if let Some(s) = samples.get(&a.id)
                && best.as_ref().is_none_or(|b| b.2 < s.at)
            {
                best = Some((a.name, (s.five * 100.0).round() as i64, s.at));
            }
        }
        best
    }

    /// The command the user's own status line runs under the bridge, if any.
    pub fn wrapped_command(&self) -> Option<String> {
        self.settings_files().iter().find_map(|f| {
            let path = format!(
                "{}/{}.command",
                self.status_line.saved_dir(),
                StatusLineSetup::key(&f.real)
            );
            fs::read_to_string(path).ok().filter(|t| !t.is_empty())
        })
    }

    /// The command that replaced the bridge, for the changed state.
    pub fn current_command(&self) -> Option<String> {
        let file = self
            .settings_files()
            .into_iter()
            .find(|f| self.status_line.state(&f.real) == State::Changed)?;
        let text = fs::read_to_string(&file.real).ok()?;
        let member = crate::status_line_setup::JsonObjectText::new(text)?.member("statusLine")?;
        let value: serde_json::Value = serde_json::from_str(&member.value).ok()?;
        value.get("command")?.as_str().map(str::to_owned)
    }
}

/// "today 11:50", "yesterday", "Sat 31 Oct"; "never" for a folder with no sessions (`Views/SetupViews.swift`).
pub struct LastUse;

impl LastUse {
    pub fn text(date: Option<f64>, now: f64, tz: &dyn TimeZone) -> String {
        let Some(date) = date else {
            return "never".to_owned();
        };
        let day = |t: f64| {
            let c = clock::local(t, tz);
            clock::days_from_civil(c.year, c.month, c.day)
        };
        if day(date) == day(now) {
            return format!("today {}", live_snapshot::clock(date, tz));
        }
        if day(date) == day(now) - 1 {
            return "yesterday".to_owned();
        }
        const MONTHS: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let c = clock::local(date, tz);
        format!(
            "{} {} {}",
            live_snapshot::weekday(date, tz),
            c.day,
            MONTHS[(c.month - 1).clamp(0, 11) as usize]
        )
    }
}
