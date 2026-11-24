//! Port of `PositionStore.swift`: the panel's last position, in the defaults as JSON under `islandPosition`.
//! The JSON is the one Swift's `JSONEncoder` writes for `SavedPosition`: `{"displayID": 2, "topCenter": [x, y]}`
//! (a `CGPoint` encodes as a two-number array).

use serde::{Deserialize, Serialize};

use crate::defaults::DefaultsStore;

pub const KEY: &str = "islandPosition";

/// Where the panel was left: the top centre of the panel, on one display.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct IslandPosition {
    #[serde(rename = "displayID")]
    pub display_id: u32,
    #[serde(rename = "topCenter")]
    pub top_center: [f64; 2],
}

pub fn encode(position: &IslandPosition) -> Vec<u8> {
    // A struct of a number and two floats always serializes (NaN/inf become null, which then fails to load).
    serde_json::to_vec(position).unwrap_or_default()
}

/// `None` when the data is not a saved position, as Swift's `try?` decode.
pub fn decode(data: &[u8]) -> Option<IslandPosition> {
    serde_json::from_slice(data).ok()
}

pub fn load(defaults: &dyn DefaultsStore) -> Option<IslandPosition> {
    defaults.get_data(KEY).and_then(|data| decode(&data))
}

pub fn save(position: &IslandPosition, defaults: &mut dyn DefaultsStore) {
    defaults.set_data(KEY, encode(position));
}
