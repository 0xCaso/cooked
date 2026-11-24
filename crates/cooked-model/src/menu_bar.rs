//! Port of `MenuBarWave` and `MenuBarLabel` (`MenuBarItem.swift`) with no AppKit: what the menu bar item shows,
//! what its clicks do, and the display that holds it.

use crate::island_placement::{Rect, ScreenInfo};
use crate::usage_snapshot::UsageSnapshot;

/// The wave's columns: one per minute of the last 9.
pub const COLS: usize = 9;

/// Half-height of each column (0, 1 or 2 dots above and below the centre), oldest minute first: each column is one
/// of the last 9 minutes against the alert threshold, the centre dot only, then 3 dots from 40%, all 5 from 90%.
/// Fewer than 9 minutes start flat on the left.
pub fn wave_heights(values: &[f64], threshold: f64) -> [u8; COLS] {
    let last = &values[values.len().saturating_sub(COLS)..];
    let mut heights = [0; COLS];
    for (h, v) in heights[COLS - last.len()..].iter_mut().zip(last) {
        let level = if threshold > 0.0 { v / threshold } else { 0.0 };
        *h = if level >= 0.9 {
            2
        } else if level >= 0.4 {
            1
        } else {
            0
        };
    }
    heights
}

/// What the item draws: the dot wave and the active account's 5-hour percent left ("—" when unknown). In a peak the
/// lit dots and the percent are red. Equal labels draw the same image, so the item redraws only on a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuBarLabel {
    pub heights: [u8; COLS],
    pub percent: String,
    pub urgent: bool,
}

impl MenuBarLabel {
    pub fn of(snapshot: &UsageSnapshot) -> Self {
        Self {
            heights: wave_heights(&snapshot.series, snapshot.chart_threshold() as f64),
            percent: snapshot
                .active()
                .and_then(|a| a.five_hour_left_if_known())
                .map_or_else(|| "—".to_owned(), |left| format!("{left}%")),
            urgent: snapshot.is_urgent(),
        }
    }
}

/// A click on the item: the left button, or the right one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemClick {
    Primary,
    Secondary,
}

/// What a click on the item does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemAction {
    /// Opens the popover under the item.
    Open,
    /// Closes the open popover.
    Close,
    /// The same click already closed the popover (its mouse-down counted as a click outside): it stays closed.
    LeaveClosed,
    /// Closes the popover and shows the menu.
    Menu,
}

/// `MenuBarItem.clicked`: the right button shows the menu; the left one toggles the popover, unless a click outside
/// closed it in the last half second (`closed_by_this_click`): on macOS 26 the item lives outside the app, so its
/// mouse-down reaches the click-outside monitor first, and the item's click must not open the popover again.
pub fn item_action(click: ItemClick, is_open: bool, closed_by_this_click: bool) -> ItemAction {
    match click {
        ItemClick::Secondary => ItemAction::Menu,
        ItemClick::Primary if is_open => ItemAction::Close,
        ItemClick::Primary if closed_by_this_click => ItemAction::LeaveClosed,
        ItemClick::Primary => ItemAction::Open,
    }
}

/// The display whose menu bar holds `item` (AppKit coordinates): the screen whose visible frame spans the item's
/// centre across and whose top is nearest below the item. `None` when no screen has it: macOS hid the item (a full
/// menu bar parks it off screen, at y = -height).
pub fn screen_under_item(item: Rect, screens: &[ScreenInfo]) -> Option<&ScreenInfo> {
    /// The item's window may overlap the visible frame's top by a point.
    const SLACK: f64 = 2.0;
    if item.width <= 0.0 {
        return None;
    }
    let x = item.mid_x();
    screens
        .iter()
        .filter(|s| {
            let v = s.visible_frame;
            v.min_x() <= x && x < v.max_x() && item.min_y() >= v.max_y() - SLACK
        })
        .min_by(|a, b| {
            let gap = |s: &ScreenInfo| item.min_y() - s.visible_frame.max_y();
            gap(a).total_cmp(&gap(b))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo_data;

    /// `MenuBarTests.waveColumnsFollowTheLastNineMinutesAgainstTheThreshold`.
    #[test]
    fn wave_columns_follow_the_last_nine_minutes_against_the_threshold() {
        let values = [999.0, 0.0, 39.0, 40.0, 89.0, 90.0, 200.0, 10.0, 50.0, 100.0];
        assert_eq!(wave_heights(&values, 100.0), [0, 0, 1, 1, 2, 2, 0, 1, 2]);
    }

    /// `MenuBarTests.waveWithFewMinutesStartsFlat`.
    #[test]
    fn wave_with_few_minutes_starts_flat() {
        assert_eq!(wave_heights(&[100.0], 100.0), [0, 0, 0, 0, 0, 0, 0, 0, 2]);
        assert_eq!(wave_heights(&[50.0, 50.0], 0.0), [0; COLS]);
        assert_eq!(wave_heights(&[], 100.0), [0; COLS]);
    }

    #[test]
    fn label_shows_the_active_percent_and_turns_red_in_a_peak() {
        let calm = demo_data::calm();
        let label = MenuBarLabel::of(&calm);
        let left = calm.active().and_then(|a| a.five_hour_left_if_known());
        assert_eq!(label.percent, format!("{}%", left.unwrap_or_default()));
        assert!(!label.urgent);
        let peak = MenuBarLabel::of(&demo_data::peak());
        assert!(peak.urgent);
        assert_ne!(peak, label);
        // no account: a dash, a flat wave
        let empty = MenuBarLabel::of(&UsageSnapshot::empty("12:00"));
        assert_eq!(empty.percent, "—");
        assert_eq!(empty.heights, [0; COLS]);
    }

    #[test]
    fn left_click_toggles_and_right_click_shows_the_menu() {
        use ItemAction::*;
        use ItemClick::*;
        assert_eq!(item_action(Primary, false, false), Open);
        assert_eq!(item_action(Primary, true, false), Close);
        assert_eq!(item_action(Secondary, false, false), Menu);
        assert_eq!(item_action(Secondary, true, false), Menu);
    }

    /// Regression 1688b1a: the item's mouse-down closed the popover as a click outside, then its click opened it
    /// again. The second click must leave it closed.
    #[test]
    fn second_click_closes_popover() {
        use crate::defaults::MemoryDefaults;
        use crate::island_model::{DismissReason, IslandState};
        let mut model = IslandState::new(demo_data::calm(), None, MemoryDefaults::default(), 0.0);
        let click = |m: &IslandState<MemoryDefaults>, now: f64| {
            item_action(ItemClick::Primary, m.is_open, m.closed_by_this_click(now))
        };
        // first click opens
        assert_eq!(click(&model, 10.0), ItemAction::Open);
        model.open();
        // second click, the monitor first: its mouse-down closes the popover...
        model.dismiss_popover(DismissReason::ClickOutside, 20.0);
        // ...and the click that follows leaves it closed
        assert_eq!(click(&model, 20.1), ItemAction::LeaveClosed);
        assert!(!model.is_open);
        // the monitor after the click: the click closes it
        model.open();
        assert_eq!(click(&model, 30.0), ItemAction::Close);
        model.close();
        // half a second on, a click opens it again
        assert_eq!(click(&model, 20.6), ItemAction::Open);
    }

    fn screen(id: u32, x: f64, y: f64, w: f64, h: f64) -> ScreenInfo {
        ScreenInfo {
            id,
            visible_frame: Rect::new(x, y, w, h - 30.0),
        }
    }

    #[test]
    fn item_is_on_the_display_under_it_or_hidden() {
        // a 3440 x 1440 display with a 1920 x 1200 one below it, at the same x
        let screens = [
            screen(1, 0.0, 0.0, 3440.0, 1440.0),
            screen(2, 0.0, -1200.0, 1920.0, 1200.0),
        ];
        // measured on macOS 26: the item's window sits 1 pt into the visible frame
        let item = Rect::new(2444.0, 1409.0, 94.0, 33.0);
        assert_eq!(screen_under_item(item, &screens).map(|s| s.id), Some(1));
        let lower = Rect::new(500.0, -31.0, 60.0, 31.0);
        assert_eq!(screen_under_item(lower, &screens).map(|s| s.id), Some(2));
        // a full menu bar parks the item at (0, -height): no display
        let hidden = Rect::new(0.0, -33.0, 154.0, 33.0);
        assert_eq!(screen_under_item(hidden, &[screens[0]]).map(|s| s.id), None);
        assert_eq!(
            screen_under_item(Rect::new(10.0, 1409.0, 0.0, 33.0), &screens),
            None
        );
    }
}
