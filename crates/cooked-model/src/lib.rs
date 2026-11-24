//! Cooked's model: reads the Claude and Codex logs and computes what the island and the popover show.
//! No AppKit here; settings, clipboard and clock come in through traits.
#![deny(clippy::unwrap_used, clippy::expect_used)]

pub mod accounts;
pub mod clock;
pub mod defaults;
pub mod limit_log;
pub mod live_snapshot;
pub mod session_format;
pub mod status_file;
pub mod status_line_bridge;
pub mod token_format;
pub mod token_sources;
pub mod transcript_log;
pub mod usage_snapshot;
