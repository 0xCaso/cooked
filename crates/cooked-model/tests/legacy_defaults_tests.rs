//! `LegacyDefaultsTests.swift`, with the in-memory store for the two suites.

use cooked_model::defaults::{DefaultsStore, DefaultsValue, MemoryDefaults};
use cooked_model::histogram_clock::ChartRange;
use cooked_model::island_model::GlassStyle;
use cooked_model::legacy_defaults;
use cooked_model::position_store;

#[test]
fn copies_the_old_settings_without_overwriting_new_ones() {
    let mut old = MemoryDefaults::default();
    let mut new = MemoryDefaults::default();
    old.set_string(GlassStyle::DEFAULTS_KEY, "clear");
    old.set_string(ChartRange::DEFAULTS_KEY, "1h");
    old.set_data(position_store::KEY, vec![1, 2]);
    old.set_string("unrelated", "x");
    new.set_string(GlassStyle::DEFAULTS_KEY, "darker");

    legacy_defaults::migrate(Some(&old), &mut new);

    assert_eq!(
        new.get_string(GlassStyle::DEFAULTS_KEY).as_deref(),
        Some("darker")
    );
    assert_eq!(
        new.get_string(ChartRange::DEFAULTS_KEY).as_deref(),
        Some("1h")
    );
    assert_eq!(new.get_data(position_store::KEY), Some(vec![1, 2]));
    assert_eq!(new.get_value("unrelated"), None);
}

#[test]
fn no_old_suite_changes_nothing() {
    let mut new = MemoryDefaults::default();
    new.set_string(GlassStyle::DEFAULTS_KEY, "system");
    legacy_defaults::migrate(None, &mut new);
    assert_eq!(
        new.get_value(GlassStyle::DEFAULTS_KEY),
        Some(DefaultsValue::String("system".into()))
    );
    assert_eq!(new.get_value(ChartRange::DEFAULTS_KEY), None);
}

#[test]
fn legacy_domain_and_keys_match_swift() {
    assert_eq!(legacy_defaults::DOMAIN, "com.0xcaso.UsageIsland");
    assert_eq!(
        legacy_defaults::KEYS,
        ["glassStyle", "chartRange", "islandPosition"]
    );
}
