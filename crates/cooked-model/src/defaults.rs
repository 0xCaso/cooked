//! Where settings live: `UserDefaults` in the app (the `mac` module), an in-memory store in tests.
//! Task 9 adds the other value kinds and the legacy migration.

use std::collections::HashMap;

pub trait DefaultsStore {
    fn get_data(&self, key: &str) -> Option<Vec<u8>>;
    fn set_data(&mut self, key: &str, value: Vec<u8>);
    /// `UserDefaults.bool(forKey:)`: false when missing.
    fn get_bool(&self, key: &str) -> bool;
    fn set_bool(&mut self, key: &str, value: bool);
    /// `UserDefaults.string(forKey:)`.
    fn get_string(&self, key: &str) -> Option<String>;
    fn set_string(&mut self, key: &str, value: &str);
    /// `UserDefaults.stringArray(forKey:)`.
    fn get_string_array(&self, key: &str) -> Option<Vec<String>>;
    fn set_string_array(&mut self, key: &str, value: Vec<String>);
    /// `UserDefaults.removeObject(forKey:)`, whatever the kind.
    fn remove(&mut self, key: &str);
}

/// A fake `UserDefaults` suite.
#[derive(Debug, Default, Clone)]
pub struct MemoryDefaults {
    pub data: HashMap<String, Vec<u8>>,
    pub bools: HashMap<String, bool>,
    pub strings: HashMap<String, String>,
    pub string_arrays: HashMap<String, Vec<String>>,
}

impl DefaultsStore for MemoryDefaults {
    fn get_data(&self, key: &str) -> Option<Vec<u8>> {
        self.data.get(key).cloned()
    }

    fn set_data(&mut self, key: &str, value: Vec<u8>) {
        self.data.insert(key.to_owned(), value);
    }

    fn get_bool(&self, key: &str) -> bool {
        self.bools.get(key).copied().unwrap_or(false)
    }

    fn set_bool(&mut self, key: &str, value: bool) {
        self.bools.insert(key.to_owned(), value);
    }

    fn get_string(&self, key: &str) -> Option<String> {
        self.strings.get(key).cloned()
    }

    fn set_string(&mut self, key: &str, value: &str) {
        self.strings.insert(key.to_owned(), value.to_owned());
    }

    fn get_string_array(&self, key: &str) -> Option<Vec<String>> {
        self.string_arrays.get(key).cloned()
    }

    fn set_string_array(&mut self, key: &str, value: Vec<String>) {
        self.string_arrays.insert(key.to_owned(), value);
    }

    fn remove(&mut self, key: &str) {
        self.data.remove(key);
        self.bools.remove(key);
        self.strings.remove(key);
        self.string_arrays.remove(key);
    }
}
