//! Cooked's model: reads the Claude and Codex logs and computes what the island and the popover show.
//! No AppKit here; settings, clipboard and clock come in through traits.
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod account_bar;
pub mod accounts;
pub mod clock;
pub mod codex_log;
pub mod contrast;
pub mod defaults;
pub mod demo_data;
pub mod dot_shading;
mod files;
pub mod histogram_clock;
pub mod island_model;
pub mod island_placement;
pub mod legacy_defaults;
pub mod limit_log;
pub mod live_snapshot;
pub mod menu_bar;
pub mod minute_ledger;
pub mod notice;
pub mod position_store;
pub mod session_format;
pub mod settings;
pub mod setup_model;
pub mod soft_dots;
pub mod status_file;
pub mod status_line_bridge;
pub mod status_line_setup;
pub mod t3_code;
pub mod tide;
pub mod tiles;
pub mod token_format;
pub mod token_log;
pub mod token_sources;
pub mod transcript_log;
pub mod usage_snapshot;
pub mod usage_source;
