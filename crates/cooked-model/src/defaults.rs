//! Where settings live: `UserDefaults` in the app (the `mac` module), an in-memory store in tests.
//! Task 9 adds the other value kinds and the legacy migration.

use std::collections::HashMap;

pub trait DefaultsStore {
    fn get_data(&self, key: &str) -> Option<Vec<u8>>;
    fn set_data(&mut self, key: &str, value: Vec<u8>);
    /// `UserDefaults.bool(forKey:)`: false when missing.
    fn get_bool(&self, key: &str) -> bool;
    fn set_bool(&mut self, key: &str, value: bool);
}

/// A fake `UserDefaults` suite.
#[derive(Debug, Default, Clone)]
pub struct MemoryDefaults {
    pub data: HashMap<String, Vec<u8>>,
    pub bools: HashMap<String, bool>,
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
}
