//! The macOS calls the model keeps behind traits: settings (`NSUserDefaults`) and the local time zone.

pub mod defaults;
pub mod events;
pub mod status_item;
pub mod window;

use cooked_model::clock::TimeZone;
use objc2::rc::Retained;
use objc2_foundation::{NSDate, NSTimeZone};

/// The user's zone as Swift's `Calendar.current` sees it: `NSTimeZone.localTimeZone`, DST included. Make one per
/// reading, so a zone change in System Settings shows up at the next one.
pub struct LocalZone(Retained<NSTimeZone>);

impl LocalZone {
    pub fn new() -> Self {
        Self(NSTimeZone::localTimeZone())
    }
}

impl Default for LocalZone {
    fn default() -> Self {
        Self::new()
    }
}

impl TimeZone for LocalZone {
    fn utc_offset(&self, unix: f64) -> i64 {
        let date = NSDate::dateWithTimeIntervalSince1970(unix);
        self.0.secondsFromGMTForDate(&date) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_zone_offset_is_whole_minutes_within_a_day() {
        let zone = LocalZone::new();
        for unix in [0.0, 1_764_000_000.0, 1_782_000_000.0] {
            let offset = zone.utc_offset(unix);
            assert!(offset.abs() <= 14 * 3600, "{offset}");
            assert_eq!(offset % 60, 0);
        }
    }
}
