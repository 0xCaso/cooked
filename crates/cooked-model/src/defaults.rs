//! Where settings live: `UserDefaults` in the app (the `mac` module), an in-memory store in tests.
//! Task 9 adds the other value kinds and the legacy migration.

use std::collections::HashMap;

pub trait DefaultsStore {
    fn get_data(&self, key: &str) -> Option<Vec<u8>>;
    fn set_data(&mut self, key: &str, value: Vec<u8>);
}

/// A fake `UserDefaults` suite.
#[derive(Debug, Default, Clone)]
pub struct MemoryDefaults {
    pub data: HashMap<String, Vec<u8>>,
}

impl DefaultsStore for MemoryDefaults {
    fn get_data(&self, key: &str) -> Option<Vec<u8>> {
        self.data.get(key).cloned()
    }

    fn set_data(&mut self, key: &str, value: Vec<u8>) {
        self.data.insert(key.to_owned(), value);
    }
}
