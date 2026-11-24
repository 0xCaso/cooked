//! Part of `Model/LimitLog.swift`: one percent sample. Task 5 adds the log.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    /// Unix seconds.
    pub at: f64,
    /// Share used, 0...1 (over 1 when blocked).
    pub five: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_reset: Option<f64>,
    pub week: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub week_reset: Option<f64>,
    pub status: String,
}
