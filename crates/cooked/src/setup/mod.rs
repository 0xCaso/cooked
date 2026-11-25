//! The Setup window (first launch) and the Settings window: one dark window, 600 pt wide and as tall as its content.
//! Setup walks three optional steps (accounts, the Claude percent, open at login); Settings shows the same groups as
//! tabs, with "Remove Cooked's changes…" under them.
//!
//! The logic is `cooked_model::setup_model`: which folders Cooked reads and their names, the status line bridge
//! (`StatusLineSetup`, the only code that edits a `settings.json`), the login item. Nothing outside Cooked's own folder
//! is written before Allow or Connect. This module holds the window's own state (the step or tab, the name fields, the
//! open disclosures, the confirmation) and hands the new accounts to the island.
//!
//! It is an ordinary Rune window next to the overlays, created hidden on the first open. `Opener` opens it from the
//! island's menu, the popover's notice and the `--setup` / `--settings` flags.

mod controls;
mod views;

use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;

use cooked_model::accounts::ConnectedAccount;
use cooked_model::clock::{self, TimeZone};
use cooked_model::island_model::DisplayMode;
use cooked_model::setup_model::{self, LoginItem, SetupModel, Step};
use cooked_model::status_line_setup::StatusLineSetup;
use rune::{App, AppHandle, AppSpec, Chrome, TextEdit, Tx, WindowOptions, WindowSpec};

use crate::island::{self, Prefs};
use crate::mac::login::MainAppLogin;
use crate::theme::PlexFont;

pub use views::view;

/// The window's width; the height follows the content.
pub const WIDTH: f32 = 600.0;
/// The band at the top with the traffic lights and the title.
pub const TITLEBAR: f32 = 40.0;
/// Open at login kept for this run only: the login item of tests, scenes and the blank window, which must never
/// register the real one.
#[derive(Debug, Clone, Default)]
pub struct KeptLogin {
    on: Rc<Cell<bool>>,
}

impl LoginItem for KeptLogin {
    fn is_enabled(&self) -> bool {
        self.on.get()
    }

    fn set(&mut self, on: bool) -> Result<(), String> {
        self.on.set(on);
        Ok(())
    }
}

/// The login item the window uses: `SMAppService.mainApp` in the app (`for_app`), a kept choice everywhere else
/// (the default).
#[derive(Debug, Clone)]
pub enum Login {
    MainApp(MainAppLogin),
    Kept(KeptLogin),
}

impl Default for Login {
    fn default() -> Self {
        Self::Kept(KeptLogin::default())
    }
}

impl LoginItem for Login {
    fn is_enabled(&self) -> bool {
        match self {
            Self::MainApp(l) => l.is_enabled(),
            Self::Kept(l) => l.is_enabled(),
        }
    }

    fn set(&mut self, on: bool) -> Result<(), String> {
        match self {
            Self::MainApp(l) => l.set(on),
            Self::Kept(l) => l.set(on),
        }
    }
}

pub type Model = SetupModel<Prefs, Login>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Setup,
    Settings,
}

/// Settings' tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tab {
    Accounts,
    Percent,
    General,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Accounts, Tab::Percent, Tab::General];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Accounts => "Accounts",
            Tab::Percent => "Claude percent",
            Tab::General => "General",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Tab::Accounts => "tab-accounts",
            Tab::Percent => "tab-percent",
            Tab::General => "tab-general",
        }
    }
}

/// Where the window's models come from: the app's defaults and home, or a scene's.
pub type MakeModel = Box<dyn Fn() -> Model>;
/// What the window tells the island when the accounts change.
pub type OnAccounts = Box<dyn Fn(Vec<ConnectedAccount>)>;

/// The window's state.
pub struct Setup {
    make: MakeModel,
    pub(crate) model: Model,
    pub(crate) mode: Mode,
    pub(crate) tab: Tab,
    /// The last move went forward (Continue) or back (Back, a tab to the left): where the new step comes from.
    pub(crate) forward: bool,
    /// One per row of `model.rows`.
    pub(crate) names: Vec<TextEdit>,
    /// A name was typed and not saved yet (Settings): saved on Return, a tab change, a tick, closing.
    names_dirty: bool,
    /// The disclosures the user opened.
    pub(crate) open: HashSet<&'static str>,
    /// The "Remove Cooked's changes?" sheet is up.
    pub(crate) confirming: bool,
    /// `--reduce-motion`; the system setting is read by the view.
    pub(crate) reduce_motion: bool,
    /// The island's model, for "Show Cooked in". `None` in scenes.
    pub(crate) island: Option<island::Model>,
    on_accounts: Option<OnAccounts>,
    /// Tells the island that "Show Cooked in" changed. `None` in scenes.
    pub on_display_mode: Option<Box<dyn Fn()>>,
    /// Scenes: the clock stopped here (Unix seconds), for "today 11:50".
    pub(crate) still_at: Option<f64>,
    pub(crate) tz: Box<dyn TimeZone>,
}

impl Setup {
    pub fn new(make: MakeModel, tz: Box<dyn TimeZone>) -> Self {
        let model = make();
        let mut setup = Self {
            make,
            model,
            mode: Mode::Setup,
            tab: Tab::Accounts,
            forward: true,
            names: Vec::new(),
            names_dirty: false,
            open: HashSet::new(),
            confirming: false,
            reduce_motion: false,
            island: None,
            on_accounts: None,
            on_display_mode: None,
            still_at: None,
            tz,
        };
        setup.load_names();
        setup
    }

    /// The app's window: `UserDefaults`, the real home and Cooked's real folder (the one the reader's bridge
    /// reads), the app's login item.
    pub fn for_app(
        island: island::Model,
        on_accounts: OnAccounts,
        launch: &crate::cli::Launch,
    ) -> Self {
        let login = Login::MainApp(MainAppLogin);
        let make: MakeModel = Box::new(move || {
            let home = std::env::var("HOME").unwrap_or_default();
            let defaults: Prefs = Box::new(crate::mac::defaults::UserDefaults::app());
            SetupModel::new(home, defaults, StatusLineSetup::default(), login.clone())
        });
        let mut setup = Self::new(make, Box::new(crate::mac::LocalZone::new()));
        setup.island = Some(island);
        setup.on_accounts = Some(on_accounts);
        setup.reduce_motion = launch.reduce_motion;
        setup
    }

    /// An empty window, before `AppSpec::init` is replaced: no folders, nothing written.
    fn blank() -> Self {
        Self::new(
            Box::new(|| {
                let defaults: Prefs = Box::<cooked_model::defaults::MemoryDefaults>::default();
                SetupModel::new(
                    "/nonexistent",
                    defaults,
                    StatusLineSetup::new("/nonexistent/Cooked"),
                    Login::default(),
                )
            }),
            Box::new(clock::FixedOffset(0)),
        )
    }

    pub fn now(&self) -> f64 {
        self.still_at.unwrap_or_else(clock::now)
    }

    fn load_names(&mut self) {
        self.names = self
            .model
            .rows
            .iter()
            .map(|r| TextEdit::new(r.name.clone()))
            .collect();
        self.names_dirty = false;
    }

    /// The window comes up: what is on disk now, the first step or `tab`.
    pub fn present(&mut self, mode: Mode, tab: Tab) {
        self.model = (self.make)();
        self.model.refresh();
        self.load_names();
        self.mode = mode;
        self.tab = tab;
        self.model.step = Step::Accounts;
        self.forward = true;
        self.open.clear();
        self.confirming = false;
    }

    pub fn title(&self) -> &'static str {
        match self.mode {
            Mode::Setup => "Set up Cooked",
            Mode::Settings => "Cooked Settings",
        }
    }

    /// The name fields into the rows.
    fn sync_names(&mut self) {
        for (row, edit) in self.model.rows.iter_mut().zip(&self.names) {
            row.name = edit.text.clone();
        }
    }

    fn save_accounts(&mut self) {
        self.sync_names();
        self.model.save_accounts();
        self.names_dirty = false;
        self.push_accounts();
    }

    fn push_accounts(&self) {
        if let Some(f) = &self.on_accounts {
            f(self.model.accounts());
        }
    }

    /// Settings saves a typed name once the user moves on.
    pub(crate) fn commit_names(&mut self) {
        if self.names_dirty && self.mode == Mode::Settings {
            self.save_accounts();
        }
    }

    pub(crate) fn name_edited(&mut self) {
        self.names_dirty = true;
    }

    pub(crate) fn toggle_row(&mut self, i: usize) {
        if let Some(row) = self.model.rows.get_mut(i) {
            row.ticked = !row.ticked;
        }
        if self.mode == Mode::Settings {
            self.save_accounts();
        } else {
            self.model.refresh();
        }
    }

    // Setup's steps

    /// The primary button acts, then moves on; Skip only moves on (Skip on Accounts keeps every folder ticked,
    /// and saves them as Continue does). Returns false when the window should close (Done).
    pub(crate) fn go(&mut self, skip: bool) -> bool {
        match self.model.step {
            Step::Accounts => self.save_accounts(),
            Step::Percent => {
                if !skip
                    && self.model.percent() != cooked_model::status_line_setup::State::Connected
                {
                    self.model.connect();
                    if self.model.problem().is_some() {
                        return true;
                    }
                }
            }
            Step::Login => {
                self.sync_names();
                self.model.finish();
                self.push_accounts();
                return false;
            }
        }
        if let Some(next) = self.model.next(self.model.step) {
            self.forward = true;
            self.model.step = next;
        }
        true
    }

    pub(crate) fn back(&mut self) {
        if let Some(back) = self.model.back(self.model.step) {
            self.forward = false;
            self.model.step = back;
        }
    }

    /// The primary button's title.
    pub fn primary_title(&self) -> &'static str {
        match self.model.step {
            Step::Accounts => "Continue",
            Step::Percent => {
                if self.model.percent() == cooked_model::status_line_setup::State::Connected {
                    "Continue"
                } else {
                    "Allow"
                }
            }
            Step::Login => "Done",
        }
    }

    // Settings

    pub(crate) fn select_tab(&mut self, tab: Tab) {
        self.commit_names();
        self.forward = tab > self.tab;
        self.tab = tab;
        self.model.refresh();
    }

    /// Setup's toggle is only the choice (Done applies it); Settings' applies at once.
    pub(crate) fn set_open_at_login(&mut self, on: bool) {
        match self.mode {
            Mode::Setup => self.model.open_at_login = on,
            Mode::Settings => self.model.set_open_at_login(on),
        }
    }

    pub(crate) fn display_mode(&self) -> Option<DisplayMode> {
        self.island.as_ref().map(|m| m.borrow().display_mode())
    }

    pub(crate) fn set_display_mode(&mut self, mode: DisplayMode) {
        if let Some(m) = &self.island {
            m.borrow_mut().set_display_mode(mode);
        }
        if let Some(changed) = &self.on_display_mode {
            changed();
        }
    }

    pub(crate) fn toggle_disclosure(&mut self, key: &'static str) {
        if !self.open.remove(key) {
            self.open.insert(key);
        }
    }

    pub(crate) fn remove_all(&mut self) {
        self.model.remove_all();
        self.confirming = false;
    }

    /// The window closed: a typed name is kept, the sheet goes.
    fn closed(&mut self) {
        self.commit_names();
        self.confirming = false;
    }
}

fn on_close(s: &mut Setup, _: &mut Tx) {
    s.closed();
}

fn on_focus(s: &mut Setup, focused: bool, _: &mut Tx) {
    if !focused {
        s.commit_names();
    }
}

/// The window's app: Plex, the dark full-size content with the traffic lights in the 40 pt band, the height
/// following the content. `windowless`: created on its first open.
fn window_app(spec: AppSpec<Setup>) -> App<Setup> {
    App::new(AppSpec {
        fonts: PlexFont::FILES,
        families: PlexFont::FAMILIES,
        min_size: Some((WIDTH, 0.0)),
        ..spec
    })
    .fit_content_height()
    .srgb_blend(true)
    .chrome(Chrome {
        full_size_content: true,
        traffic_lights: Some((16.0, 13.0)),
        drag_regions: false,
    })
}

/// Opens the window from the rest of the app (the island's menu, the popover's notice, the launch flags).
#[derive(Clone)]
pub struct Opener {
    app: AppHandle<Setup>,
}

impl Opener {
    /// Shows the window in `mode` and brings it to the front: an accessory app comes forward so the window takes
    /// the keyboard.
    pub fn open(&self, mode: Mode, tab: Tab) {
        self.app.update(move |s, tx| {
            s.present(mode, tab);
            tx.set_title(s.title());
            tx.instant();
            tx.center_window();
        });
        self.app.open();
    }
}

/// Adds the window to `app`, hidden, and returns its opener.
pub fn install<S: 'static>(app: &mut App<S>, setup: Setup) -> Opener {
    let spec = AppSpec::new("Set up Cooked", (WIDTH, 400.0), Setup::blank, view);
    let window = window_app(spec)
        .windowless()
        .init(move || setup)
        .on_close(on_close)
        .on_focus(on_focus);
    let (handle, _) = app.window(
        WindowSpec::from_app("setup", window).options(WindowOptions {
            resizable: false,
            min_size: Some((WIDTH, 0.0)),
            ..Default::default()
        }),
    );
    Opener { app: handle }
}

/// What the launch flags open: Setup on the first launch or with `--setup`, Settings with `--settings`.
pub fn launch_mode(launch: &crate::cli::Launch, done: bool) -> Option<Mode> {
    if launch.setup || !done {
        Some(Mode::Setup)
    } else if launch.settings {
        Some(Mode::Settings)
    } else {
        None
    }
}

/// Whether Setup was finished, in the app's settings.
pub fn is_done(defaults: &dyn cooked_model::defaults::DefaultsStore) -> bool {
    setup_model::is_done(defaults)
}

/// Scenes for Rune's headless tooling (`--snap`, `--tree`; feature `tooling`): Setup's and Settings' states over a home
/// made in a temporary folder, the clock stopped at Wed 4 Nov 2026 16:58 UTC.
#[cfg(any(test, feature = "tooling"))]
pub mod scenes;

#[cfg(test)]
mod tests;
