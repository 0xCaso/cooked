//! `DefaultsStore` over `NSUserDefaults`: the same domain, keys and encodings as the Swift app, so an upgrade keeps
//! every setting.

use cooked_model::defaults::{DefaultsStore, DefaultsValue};
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSArray, NSBundle, NSData, NSNumber, NSString, NSUserDefaults};

/// The real domain: the bundle id, where the Swift app keeps its settings.
pub const DOMAIN: &str = "com.0xcaso.Cooked";
/// Tests and unbundled runs (`cargo run`): never the real domain.
pub const DEV_DOMAIN: &str = "com.0xcaso.Cooked.dev";
/// Overrides the suite name.
pub const SUITE_ENV: &str = "COOKED_SUITE";

/// The suite to open: `COOKED_SUITE` when set, the real domain inside the app bundle, the dev suite otherwise.
pub fn suite_name(env: Option<&str>, bundle_id: Option<&str>) -> String {
    match (env.filter(|s| !s.is_empty()), bundle_id) {
        (Some(suite), _) => suite.to_owned(),
        (None, Some(DOMAIN)) => DOMAIN.to_owned(),
        (None, _) => DEV_DOMAIN.to_owned(),
    }
}

/// The main bundle's identifier; `None` for a bare binary.
pub fn bundle_id() -> Option<String> {
    NSBundle::mainBundle()
        .bundleIdentifier()
        .map(|s| s.to_string())
}

/// One `UserDefaults` suite.
pub struct UserDefaults {
    inner: Retained<NSUserDefaults>,
}

impl UserDefaults {
    /// The app's settings: the suite `suite_name` picks for this process.
    pub fn app() -> Self {
        let env = std::env::var(SUITE_ENV).ok();
        Self::suite(&suite_name(env.as_deref(), bundle_id().as_deref()))
    }

    /// `UserDefaults(suiteName:)`. The main bundle's own id is `.standard` (AppKit refuses it as a suite name).
    pub fn suite(name: &str) -> Self {
        if bundle_id().as_deref() == Some(name) {
            return Self {
                inner: NSUserDefaults::standardUserDefaults(),
            };
        }
        let suite = NSString::from_str(name);
        let inner = NSUserDefaults::initWithSuiteName(NSUserDefaults::alloc(), Some(&suite))
            .unwrap_or_else(NSUserDefaults::standardUserDefaults);
        Self { inner }
    }

    fn set_object(&self, key: &str, value: &AnyObject) {
        // SAFETY: `value` is a property-list object (NSData, NSString or NSArray of NSString).
        unsafe {
            self.inner
                .setObject_forKey(Some(value), &NSString::from_str(key))
        };
    }
}

impl DefaultsStore for UserDefaults {
    fn get_data(&self, key: &str) -> Option<Vec<u8>> {
        self.inner
            .dataForKey(&NSString::from_str(key))
            .map(|d| d.to_vec())
    }

    fn set_data(&mut self, key: &str, value: Vec<u8>) {
        self.set_object(key, &NSData::with_bytes(&value));
    }

    fn get_bool(&self, key: &str) -> bool {
        self.inner.boolForKey(&NSString::from_str(key))
    }

    fn set_bool(&mut self, key: &str, value: bool) {
        self.inner.setBool_forKey(value, &NSString::from_str(key));
    }

    fn get_string(&self, key: &str) -> Option<String> {
        self.inner
            .stringForKey(&NSString::from_str(key))
            .map(|s| s.to_string())
    }

    fn set_string(&mut self, key: &str, value: &str) {
        self.set_object(key, &NSString::from_str(value));
    }

    fn get_string_array(&self, key: &str) -> Option<Vec<String>> {
        self.inner
            .stringArrayForKey(&NSString::from_str(key))
            .map(|a| a.iter().map(|s| s.to_string()).collect())
    }

    fn set_string_array(&mut self, key: &str, value: Vec<String>) {
        let items: Vec<Retained<NSString>> = value.iter().map(|s| NSString::from_str(s)).collect();
        self.set_object(key, &NSArray::from_retained_slice(&items));
    }

    fn remove(&mut self, key: &str) {
        self.inner.removeObjectForKey(&NSString::from_str(key));
    }

    fn get_value(&self, key: &str) -> Option<DefaultsValue> {
        let object = self.inner.objectForKey(&NSString::from_str(key))?;
        if let Some(data) = object.downcast_ref::<NSData>() {
            return Some(DefaultsValue::Data(data.to_vec()));
        }
        if let Some(number) = object.downcast_ref::<NSNumber>() {
            return Some(DefaultsValue::Bool(number.boolValue()));
        }
        if let Some(string) = object.downcast_ref::<NSString>() {
            return Some(DefaultsValue::String(string.to_string()));
        }
        if object.downcast_ref::<NSArray>().is_some() {
            return self.get_string_array(key).map(DefaultsValue::StringArray);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dev suite, with keys only these tests use, removed again at the end.
    fn dev() -> UserDefaults {
        UserDefaults::suite(DEV_DOMAIN)
    }

    #[test]
    fn suite_name_never_picks_the_real_domain_outside_the_bundle() {
        assert_eq!(suite_name(None, None), DEV_DOMAIN);
        assert_eq!(suite_name(None, Some("com.other.app")), DEV_DOMAIN);
        assert_eq!(suite_name(Some(""), None), DEV_DOMAIN);
        assert_eq!(suite_name(None, Some(DOMAIN)), DOMAIN);
        assert_eq!(suite_name(Some("x.test"), Some(DOMAIN)), "x.test");
    }

    #[test]
    fn every_kind_round_trips() {
        let mut d = dev();
        let k = |s: &str| format!("cooked.test.roundtrip.{s}");
        d.set_data(&k("data"), vec![0, 1, 2, 255]);
        d.set_bool(&k("bool"), true);
        d.set_string(&k("string"), "darker");
        d.set_string_array(&k("array"), vec!["a".into(), "b".into()]);
        assert_eq!(d.get_data(&k("data")), Some(vec![0, 1, 2, 255]));
        assert!(d.get_bool(&k("bool")));
        assert_eq!(d.get_string(&k("string")).as_deref(), Some("darker"));
        assert_eq!(
            d.get_string_array(&k("array")),
            Some(vec!["a".to_owned(), "b".to_owned()])
        );
        assert_eq!(
            d.get_value(&k("data")),
            Some(DefaultsValue::Data(vec![0, 1, 2, 255]))
        );
        assert_eq!(d.get_value(&k("bool")), Some(DefaultsValue::Bool(true)));
        assert_eq!(
            d.get_value(&k("string")),
            Some(DefaultsValue::String("darker".into()))
        );
        assert_eq!(
            d.get_value(&k("array")),
            Some(DefaultsValue::StringArray(vec!["a".into(), "b".into()]))
        );
        for key in ["data", "bool", "string", "array"] {
            d.remove(&k(key));
            assert_eq!(d.get_value(&k(key)), None);
        }
        assert!(!d.get_bool(&k("bool")));
    }

    #[test]
    fn settings_round_trip_through_the_dev_suite() {
        use cooked_model::settings::Settings;
        // Its own suite, so the real keys don't touch anything a dev run keeps in the dev suite.
        let name = format!("{DEV_DOMAIN}.test-settings");
        let mut d = UserDefaults::suite(&name);
        let mut settings = Settings::load(&d);
        settings.setup_done = true;
        settings.dismissed_notices = vec!["b".into(), "a".into()];
        settings.save(&mut d);
        let loaded = Settings::load(&d);
        assert!(loaded.setup_done);
        assert_eq!(
            loaded.dismissed_notices,
            vec!["a".to_owned(), "b".to_owned()]
        );
        assert_eq!(loaded.glass_style, settings.glass_style);
        d.inner
            .removePersistentDomainForName(&NSString::from_str(&name));
    }
}
