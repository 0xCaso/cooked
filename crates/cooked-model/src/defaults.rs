//! Where settings live: `UserDefaults` in the app (the `mac` module), an in-memory store in tests.

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
    /// `UserDefaults.object(forKey:)`: the value whatever its kind, `None` when the key is missing.
    fn get_value(&self, key: &str) -> Option<DefaultsValue>;
    /// `UserDefaults.set(_:forKey:)` with a value read from another suite.
    fn set_value(&mut self, key: &str, value: DefaultsValue) {
        match value {
            DefaultsValue::Data(v) => self.set_data(key, v),
            DefaultsValue::Bool(v) => self.set_bool(key, v),
            DefaultsValue::String(v) => self.set_string(key, &v),
            DefaultsValue::StringArray(v) => self.set_string_array(key, v),
        }
    }
}

/// The kinds of value Cooked keeps in its defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefaultsValue {
    Data(Vec<u8>),
    Bool(bool),
    String(String),
    StringArray(Vec<String>),
}

/// A fake `UserDefaults` suite: one value per key, whatever its kind.
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
        self.remove(key); // one value per key, as UserDefaults
        self.data.insert(key.to_owned(), value);
    }

    fn get_bool(&self, key: &str) -> bool {
        self.bools.get(key).copied().unwrap_or(false)
    }

    fn set_bool(&mut self, key: &str, value: bool) {
        self.remove(key);
        self.bools.insert(key.to_owned(), value);
    }

    fn get_string(&self, key: &str) -> Option<String> {
        self.strings.get(key).cloned()
    }

    fn set_string(&mut self, key: &str, value: &str) {
        self.remove(key);
        self.strings.insert(key.to_owned(), value.to_owned());
    }

    fn get_string_array(&self, key: &str) -> Option<Vec<String>> {
        self.string_arrays.get(key).cloned()
    }

    fn set_string_array(&mut self, key: &str, value: Vec<String>) {
        self.remove(key);
        self.string_arrays.insert(key.to_owned(), value);
    }

    fn remove(&mut self, key: &str) {
        self.data.remove(key);
        self.bools.remove(key);
        self.strings.remove(key);
        self.string_arrays.remove(key);
    }

    fn get_value(&self, key: &str) -> Option<DefaultsValue> {
        if let Some(v) = self.data.get(key) {
            return Some(DefaultsValue::Data(v.clone()));
        }
        if let Some(v) = self.bools.get(key) {
            return Some(DefaultsValue::Bool(*v));
        }
        if let Some(v) = self.strings.get(key) {
            return Some(DefaultsValue::String(v.clone()));
        }
        self.string_arrays
            .get(key)
            .map(|v| DefaultsValue::StringArray(v.clone()))
    }
}
