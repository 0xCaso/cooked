//! Port of `Model/StatusLineBridge.swift`. The Claude percent, from the files the status line bridge writes: for each
//! account, the JSON Claude Code last gave its status line, in
//! `~/Library/Application Support/Cooked/bridge/<account id>.json`. Claude Code sends `rate_limits.five_hour` and
//! `seven_day` (`used_percentage` 0...100, `resets_at` Unix seconds) after each response, for subscribers only. The
//! file's change time is the sample's time.

use crate::limit_log::Sample;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Deserialize)]
struct Window {
    used_percentage: Option<f64>,
    resets_at: Option<f64>,
}

#[derive(Deserialize)]
struct Limits {
    five_hour: Option<Window>,
    seven_day: Option<Window>,
}

#[derive(Deserialize)]
struct Input {
    rate_limits: Option<Limits>,
}

/// `~/Library/Application Support/Cooked/bridge`; `None` without `$HOME`.
pub fn default_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(Path::new(&home).join("Library/Application Support/Cooked/bridge"))
}

fn json_names(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok()?.file_name().into_string().ok())
                .filter(|n| n.ends_with(".json"))
                .collect()
        })
        .unwrap_or_default()
}

fn account_of(name: &str) -> String {
    name.strip_suffix(".json").unwrap_or(name).to_owned()
}

/// Account id → its newest sample, for the files with a 5 h percent.
pub fn samples(dir: &Path) -> HashMap<String, Sample> {
    let mut out = HashMap::new();
    for name in json_names(dir) {
        let path = dir.join(&name);
        let Some(modified) = std::fs::symlink_metadata(&path)
            .ok()
            .and_then(|m| m.modified().ok())
        else {
            continue;
        };
        let at = match modified.duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_secs_f64(),
            Err(e) => -e.duration().as_secs_f64(),
        };
        let Ok(data) = std::fs::read(&path) else {
            continue;
        };
        if let Some(sample) = sample(&data, at) {
            out.insert(account_of(&name), sample);
        }
    }
    out
}

/// The accounts with a bridge file: their status line ran, with or without a percent.
pub fn accounts(dir: &Path) -> HashSet<String> {
    json_names(dir).iter().map(|n| account_of(n)).collect()
}

/// `at`: Unix seconds.
pub fn sample(data: &[u8], at: f64) -> Option<Sample> {
    let limits = serde_json::from_slice::<Input>(data).ok()?.rate_limits?;
    let five_hour = limits.five_hour?;
    let five = five_hour.used_percentage?;
    Some(Sample {
        at,
        five: five / 100.0,
        five_reset: five_hour.resets_at,
        week: limits
            .seven_day
            .as_ref()
            .and_then(|w| w.used_percentage)
            .unwrap_or(0.0)
            / 100.0,
        week_reset: limits.seven_day.and_then(|w| w.resets_at),
        status: if five >= 100.0 { "rejected" } else { "allowed" }.to_owned(),
    })
}
