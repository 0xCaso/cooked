//! Constants of `Model/LiveSnapshot.swift` that `UsageSnapshot` needs. Task 6 ports the rest.

/// The watcher's fallback p99 for 15 minutes, when status.json has none.
pub const FALLBACK_THRESHOLD_15M: i64 = 500_000;
/// The chart keeps a day, for its longest range (`ChartRange.day`).
pub const HISTORY_MINUTES: usize = 24 * 60;
