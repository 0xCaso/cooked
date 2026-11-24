//! Port of `Views/IslandModel.swift` with no I/O: what the island shows and whether the popover is open, as pure
//! state and transitions. Opening windows and the clipboard come back as a `NoticeAction` for the app to run; times
//! come in as Unix seconds. `GlassStyle` is from `Views/Surfaces.swift`.

use crate::defaults::DefaultsStore;
use crate::demo_data::{self, CALM_RATES, Feed};
use crate::histogram_clock::{ChartRange, Histogram, HistogramClock, LiveHistogram};
use crate::notice::Notice;
use crate::token_sources::{LIMIT, TokenSource};
use crate::usage_snapshot::{Account, UsageSnapshot};
use crate::usage_source::Reading;
use std::collections::HashSet;

/// The glass look, chosen in the island's right-click menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GlassStyle {
    System,
    Clear,
    #[default]
    Darker,
}

impl GlassStyle {
    pub const ALL: [GlassStyle; 3] = [GlassStyle::System, GlassStyle::Clear, GlassStyle::Darker];
    pub const DEFAULTS_KEY: &'static str = "glassStyle";

    pub fn raw(self) -> &'static str {
        match self {
            GlassStyle::System => "system",
            GlassStyle::Clear => "clear",
            GlassStyle::Darker => "darker",
        }
    }

    pub fn from_raw(raw: &str) -> Option<GlassStyle> {
        Self::ALL.into_iter().find(|s| s.raw() == raw)
    }

    pub fn title(self) -> &'static str {
        match self {
            GlassStyle::System => "System",
            GlassStyle::Clear => "Clear",
            GlassStyle::Darker => "Darker",
        }
    }
}

/// Where Cooked shows when the popover is closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DisplayMode {
    #[default]
    Island,
    MenuBar,
}

impl DisplayMode {
    pub const ALL: [DisplayMode; 2] = [DisplayMode::Island, DisplayMode::MenuBar];
    pub const DEFAULTS_KEY: &'static str = "displayMode";

    pub fn raw(self) -> &'static str {
        match self {
            DisplayMode::Island => "island",
            DisplayMode::MenuBar => "menuBar",
        }
    }

    pub fn from_raw(raw: &str) -> Option<DisplayMode> {
        Self::ALL.into_iter().find(|m| m.raw() == raw)
    }

    pub fn title(self) -> &'static str {
        match self {
            DisplayMode::Island => "Island",
            DisplayMode::MenuBar => "Menu bar",
        }
    }
}

/// Why the popover closed without a click on the island.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DismissReason {
    Escape,
    ClickOutside,
    ResignedKey,
    Typing,
}

/// What the notice card's button asks the app to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoticeAction {
    OpenSetup,
    /// Settings on its percent tab.
    OpenPercentSettings,
    /// Put this text on the clipboard.
    Copy(String),
    /// The notice was put off; nothing else to do.
    Dismissed,
}

/// What the island shows and whether the popover is open: live data, or one of the two demos.
pub struct IslandState<D: DefaultsStore> {
    pub snapshot: UsageSnapshot,
    pub is_open: bool,
    /// The "What burns tokens" list starts open: only to measure the panel.
    pub sources_expanded: bool,
    /// Tokens per minute right now. In the calm demo it rolls through `CALM_RATES`.
    pub rate_now: i64,
    histogram: Histogram,
    /// The live histogram exists (the first reading since live was chosen arrived).
    live_started: bool,
    feed: Feed,
    notices: Vec<Notice>,
    dismissed: HashSet<String>,
    history_progress: Option<f64>,
    calm_rate_index: usize,
    chart_epoch: f64,
    glass_style: GlassStyle,
    display_mode: DisplayMode,
    chart_range: ChartRange,
    defaults: D,
    accounts_version: u64,
    closed_by_mouse_at: Option<f64>,
}

impl<D: DefaultsStore> IslandState<D> {
    pub const DISMISSED_KEY: &'static str = "dismissedNotices";

    /// `feed`: `None` picks the demo that matches `snapshot`. `epoch`: time zero of every chart, Unix seconds; it lives
    /// as long as the state, so clicks and demo switches don't restart the wave.
    pub fn new(snapshot: UsageSnapshot, feed: Option<Feed>, defaults: D, epoch: f64) -> Self {
        let dismissed = defaults
            .get_string_array(Self::DISMISSED_KEY)
            .unwrap_or_default()
            .into_iter()
            .collect();
        let feed = feed.unwrap_or(if snapshot.is_urgent() {
            Feed::DemoPeak
        } else {
            Feed::DemoCalm
        });
        let read = |key: &str| defaults.get_string(key);
        let glass_style = read(GlassStyle::DEFAULTS_KEY)
            .and_then(|r| GlassStyle::from_raw(&r))
            .unwrap_or_default();
        let display_mode = read(DisplayMode::DEFAULTS_KEY)
            .and_then(|r| DisplayMode::from_raw(&r))
            .unwrap_or_default();
        let chart_range = read(ChartRange::DEFAULTS_KEY)
            .and_then(|r| ChartRange::from_raw(&r))
            .unwrap_or_default();
        Self {
            rate_now: snapshot.rate_now,
            histogram: Histogram::Clock(Self::demo_clock(&snapshot)),
            snapshot,
            is_open: false,
            sources_expanded: false,
            live_started: false,
            feed,
            notices: Vec::new(),
            dismissed,
            history_progress: None,
            calm_rate_index: 0,
            chart_epoch: epoch,
            glass_style,
            display_mode,
            chart_range,
            defaults,
            accounts_version: 0,
            closed_by_mouse_at: None,
        }
    }

    pub fn defaults(&self) -> &D {
        &self.defaults
    }

    pub fn into_defaults(self) -> D {
        self.defaults
    }

    /// The popover's "now": every account summed in live data, else `rate_now`.
    pub fn chart_rate(&self) -> i64 {
        self.snapshot.total_rate.unwrap_or(self.rate_now)
    }

    /// The popover histogram, rebuilt when the feed changes; live data updates it in place.
    pub fn histogram(&mut self) -> &mut Histogram {
        &mut self.histogram
    }

    pub fn feed(&self) -> Feed {
        self.feed
    }

    /// What live data lacks and how to fix it, first the one to show (`notice`).
    pub fn notices(&self) -> &[Notice] {
        &self.notices
    }

    /// The notices put off with "Not now" or "OK", kept until the accounts change.
    pub fn dismissed(&self) -> &HashSet<String> {
        &self.dismissed
    }

    /// The popover's notice card: the first notice not put off.
    pub fn notice(&self) -> Option<&Notice> {
        Notice::first(&self.notices, &self.dismissed)
    }

    /// 0...1 while the first launch reads 14 days of history; `None` when complete or in a demo.
    pub fn history_progress(&self) -> Option<f64> {
        self.history_progress
    }

    pub fn chart_epoch(&self) -> f64 {
        self.chart_epoch
    }

    /// Goes up when Setup or Settings change the accounts: the live feed starts a new reader.
    pub fn accounts_version(&self) -> u64 {
        self.accounts_version
    }

    pub fn glass_style(&self) -> GlassStyle {
        self.glass_style
    }

    pub fn set_glass_style(&mut self, style: GlassStyle) {
        self.glass_style = style;
        self.defaults
            .set_string(GlassStyle::DEFAULTS_KEY, style.raw());
    }

    pub fn display_mode(&self) -> DisplayMode {
        self.display_mode
    }

    pub fn set_display_mode(&mut self, mode: DisplayMode) {
        self.display_mode = mode;
        self.defaults
            .set_string(DisplayMode::DEFAULTS_KEY, mode.raw());
    }

    /// How far back the popover's chart looks, kept across launches.
    pub fn chart_range(&self) -> ChartRange {
        self.chart_range
    }

    pub fn set_chart_range(&mut self, range: ChartRange) {
        self.chart_range = range;
        self.defaults
            .set_string(ChartRange::DEFAULTS_KEY, range.raw());
    }

    /// The island says "Set up Cooked" instead of the numbers.
    pub fn shows_setup(&self) -> bool {
        self.notice() == Some(&Notice::NoAccounts)
    }

    /// The active account's 5h % left, or "—" without a percent (the Swift island showed no text there; the
    /// popover's notice says why).
    pub fn five_hour_label(&self) -> String {
        match self
            .snapshot
            .active()
            .and_then(Account::five_hour_left_if_known)
        {
            Some(left) => format!("{left}%"),
            None => "—".to_owned(),
        }
    }

    pub fn accounts_changed(&mut self) {
        self.accounts_version += 1;
        self.dismissed.clear();
        self.defaults.remove(Self::DISMISSED_KEY);
    }

    /// The notice card's button.
    pub fn act(&mut self, notice: &Notice) -> NoticeAction {
        match notice {
            Notice::NoAccounts => NoticeAction::OpenSetup,
            Notice::Paused | Notice::NoPercent { .. } => NoticeAction::OpenPercentSettings,
            Notice::FormatChanged { .. } => {
                NoticeAction::Copy(notice.details().unwrap_or_else(|| notice.title()))
            }
            Notice::ApiKey { .. } => {
                self.dismiss(notice);
                NoticeAction::Dismissed
            }
        }
    }

    /// "Not now" and "OK": hides the notice until the accounts change.
    pub fn dismiss(&mut self, notice: &Notice) {
        let Some(key) = notice.dismiss_key() else {
            return;
        };
        self.dismissed.insert(key);
        let mut keys: Vec<String> = self.dismissed.iter().cloned().collect();
        keys.sort();
        self.defaults.set_string_array(Self::DISMISSED_KEY, keys);
    }

    /// Shows notices without live data: snapshots and the panel's measure.
    pub fn show(&mut self, notices: Vec<Notice>) {
        self.notices = notices;
    }

    pub fn toggle_demo(&mut self) {
        self.select(if self.snapshot.is_urgent() {
            Feed::DemoCalm
        } else {
            Feed::DemoPeak
        });
    }

    /// Switches the feed. Live keeps what is on screen until the first reading arrives.
    pub fn select(&mut self, new: Feed) {
        self.feed = new;
        self.live_started = false;
        if let Some(snapshot) = new.demo() {
            self.notices.clear();
            self.history_progress = None;
            self.calm_rate_index = 0;
            self.rate_now = snapshot.rate_now;
            self.histogram = Histogram::Clock(Self::demo_clock(&snapshot));
            self.snapshot = snapshot;
        }
    }

    /// A live reading.
    pub fn apply(&mut self, reading: &Reading) {
        if self.feed != Feed::Live {
            return;
        }
        self.notices.clone_from(&reading.notices);
        self.history_progress = reading.history;
        let s = &reading.snapshot;
        if self.snapshot != *s {
            self.snapshot = s.clone();
        }
        self.rate_now = s.rate_now;
        let threshold = s.chart_threshold() as f64;
        match &mut self.histogram {
            Histogram::Live(h) if self.live_started => {
                h.update(s.series.clone(), threshold, reading.minute)
            }
            _ => {
                self.histogram = Histogram::Live(LiveHistogram::new(
                    s.series.clone(),
                    threshold,
                    reading.minute,
                    self.chart_epoch,
                ));
                self.live_started = true;
            }
        }
    }

    /// The mockup's demo feed: one minute lasts 4 s, each new minute takes a value from the series, ±15%.
    pub fn demo_clock(s: &UsageSnapshot) -> HistogramClock {
        let source = s.series.clone();
        let n = source.len();
        let mut feed = 0usize;
        HistogramClock::new(
            source.clone(),
            s.chart_threshold() as f64,
            4.0,
            Box::new(move || {
                let v = source
                    .get((feed * 7) % n.saturating_sub(10).max(1) + 5)
                    .copied()
                    .unwrap_or(0.0);
                feed += 1;
                v * (0.85 + 0.3 * ((feed * 37) % 10) as f64 / 10.0)
            }),
        )
    }

    pub fn open(&mut self) {
        self.is_open = true;
    }

    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Esc, a click outside the island and popover, or another window taking the keyboard, at `now` (Unix seconds).
    pub fn dismiss_popover(&mut self, reason: DismissReason, now: f64) {
        if !self.is_open {
            return;
        }
        self.is_open = false;
        if matches!(
            reason,
            DismissReason::ClickOutside | DismissReason::ResignedKey
        ) {
            self.closed_by_mouse_at = Some(now);
        }
    }

    /// A click elsewhere closed the popover in the last half second: the menu bar item's click is that same click,
    /// and must leave the popover closed. (The menu bar item lives outside the app on macOS 26, so the mouse-down on
    /// it closes the popover before the item's click arrives.)
    pub fn closed_by_this_click(&self, now: f64) -> bool {
        self.closed_by_mouse_at.is_some_and(|at| now - at < 0.5)
    }

    /// Every 3 s: the next calm rate. The peak and live data keep their rate.
    pub fn tick_rate(&mut self) {
        if self.feed != Feed::DemoCalm {
            return;
        }
        self.calm_rate_index = (self.calm_rate_index + 1) % CALM_RATES.len();
        self.rate_now = CALM_RATES[self.calm_rate_index];
    }

    /// The demo peak with `LIMIT` sources and at least `accounts` accounts in use: the tallest popover
    /// (`IslandLayout.tallest`).
    pub fn tallest(accounts: usize) -> UsageSnapshot {
        let mut s = demo_data::peak();
        if let Some(template) = s.sources.last().cloned() {
            for i in s.sources.len()..LIMIT.max(s.sources.len()) {
                s.sources.push(TokenSource {
                    id: format!("measure-{i}"),
                    thread_id: None,
                    ended_ago: None,
                    session: None,
                    ..template.clone()
                });
            }
        }
        if let Some(last) = s.accounts.last().cloned() {
            for i in s.accounts.len()..accounts.max(s.accounts.len()) {
                let mut a = Account::new(
                    &format!("measure-{i}"),
                    &last.short,
                    last.five_hour_percent,
                    &last.five_hour_reset,
                    last.is_live,
                );
                a.week_percent = last.week_percent;
                a.five_hour_elapsed = last.five_hour_elapsed;
                a.week_elapsed = last.week_elapsed;
                a.week_reset.clone_from(&last.week_reset);
                s.accounts.push(a);
            }
        }
        s
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    /// `CGRect.contains`: the min edges in, the max edges out.
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.x + self.width && p.y >= self.y && p.y < self.y + self.height
    }
}

/// Clicks on the transparent part of the panel go to the window below.
pub struct IslandHitTest;

impl IslandHitTest {
    /// `point` and `panel` in screen coordinates (bottom-left origin); `rects` in the panel's top-left coordinates.
    pub fn hits(point: Point, rects: &[Rect], panel: Rect) -> bool {
        if !panel.contains(point) {
            return false;
        }
        let local = Point {
            x: point.x - panel.x,
            y: panel.y + panel.height - point.y,
        };
        rects.iter().any(|r| r.contains(local))
    }
}
