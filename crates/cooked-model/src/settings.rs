//! Every setting Cooked keeps in its `UserDefaults` domain (`com.0xcaso.Cooked`), with the keys, encodings and
//! defaults earlier versions used, so an upgrade loses nothing. Each key is read and written by the module
//! that owns it; this gathers them in one load and one save. The data feed (live/demo) is not saved.

use crate::accounts::{AccountStore, ConnectedAccount};
use crate::defaults::DefaultsStore;
use crate::histogram_clock::ChartRange;
use crate::island_model::{DISMISSED_KEY, DisplayMode, GlassStyle};
use crate::position_store::{self, IslandPosition};
use crate::setup_model;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Settings {
    /// `setupDone`: false when missing.
    pub setup_done: bool,
    /// `accounts`: `None` until the first launch saves them (then `AccountStore::current` scans).
    pub accounts: Option<Vec<ConnectedAccount>>,
    /// `islandPosition`: `None` until the panel is moved once.
    pub island_position: Option<IslandPosition>,
    /// `glassStyle`, `displayMode`, `chartRange`: raw strings; missing or unknown → the default.
    pub glass_style: GlassStyle,
    pub display_mode: DisplayMode,
    pub chart_range: ChartRange,
    /// `dismissedNotices`: the notices' dismiss keys, sorted.
    pub dismissed_notices: Vec<String>,
}

impl Settings {
    pub fn load(defaults: &dyn DefaultsStore) -> Settings {
        let raw = |key: &str| defaults.get_string(key);
        let mut dismissed_notices = defaults.get_string_array(DISMISSED_KEY).unwrap_or_default();
        dismissed_notices.sort();
        Settings {
            setup_done: setup_model::is_done(defaults),
            accounts: AccountStore::load(defaults),
            island_position: position_store::load(defaults),
            glass_style: raw(GlassStyle::DEFAULTS_KEY)
                .and_then(|r| GlassStyle::from_raw(&r))
                .unwrap_or_default(),
            display_mode: raw(DisplayMode::DEFAULTS_KEY)
                .and_then(|r| DisplayMode::from_raw(&r))
                .unwrap_or_default(),
            chart_range: raw(ChartRange::DEFAULTS_KEY)
                .and_then(|r| ChartRange::from_raw(&r))
                .unwrap_or_default(),
            dismissed_notices,
        }
    }

    /// Writes every key; `None` accounts or position remove theirs, so the next load sees them missing again.
    pub fn save(&self, defaults: &mut dyn DefaultsStore) {
        defaults.set_bool(setup_model::DONE_KEY, self.setup_done);
        match &self.accounts {
            Some(accounts) => AccountStore::save(accounts, defaults),
            None => defaults.remove(AccountStore::KEY),
        }
        match &self.island_position {
            Some(position) => position_store::save(position, defaults),
            None => defaults.remove(position_store::KEY),
        }
        defaults.set_string(GlassStyle::DEFAULTS_KEY, self.glass_style.raw());
        defaults.set_string(DisplayMode::DEFAULTS_KEY, self.display_mode.raw());
        defaults.set_string(ChartRange::DEFAULTS_KEY, self.chart_range.raw());
        let mut dismissed = self.dismissed_notices.clone();
        dismissed.sort();
        dismissed.dedup();
        defaults.set_string_array(DISMISSED_KEY, dismissed);
    }
}
