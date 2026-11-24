//! Constants of `Model/LiveSnapshot.swift` that `UsageSnapshot` needs, and `short` for `token_sources`. Task 6 ports
//! the rest.

use crate::accounts::ConnectedAccount;

/// The watcher's fallback p99 for 15 minutes, when status.json has none.
pub const FALLBACK_THRESHOLD_15M: i64 = 500_000;
/// The chart keeps a day, for its longest range (`ChartRange.day`).
pub const HISTORY_MINUTES: usize = 24 * 60;

/// The account's short name: "team"; its id when it is not connected.
pub fn short(id: &str, accounts: &[ConnectedAccount]) -> String {
    accounts
        .iter()
        .find(|a| a.id == id)
        .map_or_else(|| id.to_owned(), |a| a.name.clone())
}
