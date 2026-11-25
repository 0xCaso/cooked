//! Cooked's bundle id was `com.0xcaso.UsageIsland`; the settings saved under it
//! (glass, chart range, position) are copied once, so the rename keeps them. A key Cooked already has is never
//! overwritten.

use crate::defaults::DefaultsStore;
use crate::histogram_clock::ChartRange;
use crate::island_model::GlassStyle;
use crate::position_store;

pub const DOMAIN: &str = "com.0xcaso.UsageIsland";
pub const KEYS: [&str; 3] = [
    GlassStyle::DEFAULTS_KEY,
    ChartRange::DEFAULTS_KEY,
    position_store::KEY,
];

/// `old`: the `com.0xcaso.UsageIsland` suite, `None` when it cannot be opened.
pub fn migrate(old: Option<&dyn DefaultsStore>, new: &mut dyn DefaultsStore) {
    let Some(old) = old else { return };
    for key in KEYS {
        if new.get_value(key).is_none()
            && let Some(value) = old.get_value(key)
        {
            new.set_value(key, value);
        }
    }
}
