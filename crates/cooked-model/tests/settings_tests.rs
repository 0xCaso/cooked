//! Review Focus 3: the defaults the Swift app wrote load with no loss, and every key round-trips.
//! `fixtures/defaults.plist` is `defaults export com.0xcaso.Cooked` from a real Swift install, account names and
//! paths anonymized (the JSON bytes are otherwise the ones Swift's `JSONEncoder` wrote).

mod common;

use cooked_model::accounts::{AccountStore, ConnectedAccount};
use cooked_model::defaults::{DefaultsStore, MemoryDefaults};
use cooked_model::histogram_clock::ChartRange;
use cooked_model::island_model::{DisplayMode, GlassStyle};
use cooked_model::position_store::IslandPosition;
use cooked_model::settings::Settings;

/// The plist's top-level dict into the fake store, by value kind, as `UserDefaults` would hand them out.
fn load_plist(name: &str) -> MemoryDefaults {
    let value = plist::Value::from_file(common::fixture(name)).expect("plist");
    let dict = value.into_dictionary().expect("top-level dict");
    let mut store = MemoryDefaults::default();
    for (key, value) in dict {
        match value {
            plist::Value::Boolean(b) => store.set_bool(&key, b),
            plist::Value::String(s) => store.set_string(&key, &s),
            plist::Value::Data(d) => store.set_data(&key, d),
            plist::Value::Array(items) => {
                let strings = items
                    .into_iter()
                    .map(|i| i.into_string().expect("string array"))
                    .collect();
                store.set_string_array(&key, strings);
            }
            other => panic!("unexpected value for {key}: {other:?}"),
        }
    }
    store
}

#[test]
fn real_swift_plist_loads() {
    let store = load_plist("defaults.plist");
    let settings = Settings::load(&store);

    assert!(settings.setup_done);
    assert_eq!(
        settings.accounts,
        Some(vec![
            ConnectedAccount::new("claudeAgent", "claude", "~/.claude"),
            ConnectedAccount::new("claudeAgent_work_team", "work team", "~/.claude-work-team"),
            ConnectedAccount::new("codex", "codex", "~/.codex"),
        ])
    );
    assert_eq!(
        settings.island_position,
        Some(IslandPosition {
            display_id: 3,
            top_center: [1844.0, 1369.0]
        })
    );
    assert_eq!(settings.glass_style, GlassStyle::Clear);
    assert_eq!(settings.display_mode, DisplayMode::Island);
    assert_eq!(settings.chart_range, ChartRange::TenMinutes);
    assert!(settings.dismissed_notices.is_empty()); // the real install has dismissed none

    let mut saved = MemoryDefaults::default();
    settings.save(&mut saved);
    assert_eq!(Settings::load(&saved), settings);
    assert!(saved.get_bool("setupDone"));
    // The accounts JSON goes back out as the bytes Swift wrote.
    assert_eq!(
        saved.get_data(AccountStore::KEY),
        store.get_data(AccountStore::KEY)
    );
}

#[test]
fn empty_store_gives_swift_defaults() {
    let settings = Settings::load(&MemoryDefaults::default());
    assert_eq!(settings, Settings::default());
    assert!(!settings.setup_done);
    assert_eq!(settings.accounts, None);
    assert_eq!(settings.island_position, None);
    assert_eq!(settings.glass_style, GlassStyle::Darker);
    assert_eq!(settings.display_mode, DisplayMode::Island);
    assert_eq!(settings.chart_range, ChartRange::TenMinutes);
    assert!(settings.dismissed_notices.is_empty());
}

#[test]
fn unknown_raw_strings_fall_back_to_defaults() {
    let mut store = MemoryDefaults::default();
    store.set_string("glassStyle", "frosted");
    store.set_string("displayMode", "dock");
    store.set_string("chartRange", "2h");
    store.set_data("islandPosition", b"not json".to_vec());
    store.set_data("accounts", b"{}".to_vec());
    assert_eq!(Settings::load(&store), Settings::default());
}

#[test]
fn every_key_round_trips() {
    let settings = Settings {
        setup_done: true,
        accounts: Some(vec![
            ConnectedAccount::new("claudeAgent_personal", "personal", "~/.claude-personal"),
            ConnectedAccount::new("codex", "codex", "~/.codex"),
        ]),
        island_position: Some(IslandPosition {
            display_id: 2,
            top_center: [756.5, 949.0],
        }),
        glass_style: GlassStyle::System,
        display_mode: DisplayMode::MenuBar,
        chart_range: ChartRange::FourHours,
        dismissed_notices: vec!["apiKey".into(), "formatChanged".into()],
    };
    let mut store = MemoryDefaults::default();
    settings.save(&mut store);

    assert!(store.get_bool("setupDone"));
    assert_eq!(store.get_string("glassStyle").as_deref(), Some("system"));
    assert_eq!(store.get_string("displayMode").as_deref(), Some("menuBar"));
    assert_eq!(store.get_string("chartRange").as_deref(), Some("4h"));
    assert_eq!(
        store.get_string_array("dismissedNotices"),
        Some(vec!["apiKey".to_owned(), "formatChanged".to_owned()])
    );
    assert_eq!(Settings::load(&store), settings);

    for glass_style in GlassStyle::ALL {
        for display_mode in DisplayMode::ALL {
            for chart_range in ChartRange::ALL {
                let s = Settings {
                    glass_style,
                    display_mode,
                    chart_range,
                    ..Settings::default()
                };
                let mut store = MemoryDefaults::default();
                s.save(&mut store);
                assert_eq!(Settings::load(&store), s);
            }
        }
    }
}

#[test]
fn dismissed_notices_save_sorted() {
    let settings = Settings {
        dismissed_notices: vec!["paused".into(), "apiKey".into(), "noPercent".into()],
        ..Settings::default()
    };
    let mut store = MemoryDefaults::default();
    settings.save(&mut store);
    assert_eq!(
        store.get_string_array("dismissedNotices"),
        Some(vec![
            "apiKey".to_owned(),
            "noPercent".to_owned(),
            "paused".to_owned()
        ])
    );
    assert_eq!(
        Settings::load(&store).dismissed_notices,
        vec!["apiKey", "noPercent", "paused"]
    );
}

#[test]
fn absent_accounts_and_position_stay_absent() {
    let mut store = MemoryDefaults::default();
    store.set_data("accounts", AccountStore::encode(&[]));
    store.set_data("islandPosition", b"{}".to_vec());
    Settings::default().save(&mut store);
    assert_eq!(store.get_data("accounts"), None);
    assert_eq!(store.get_data("islandPosition"), None);
}

#[test]
fn setup_done_true_stays_true() {
    let mut store = MemoryDefaults::default();
    store.set_bool("setupDone", true);
    let settings = Settings::load(&store);
    let mut out = MemoryDefaults::default();
    settings.save(&mut out);
    assert!(Settings::load(&out).setup_done);
}
