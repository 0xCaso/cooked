//! Port of `IslandPlacement.swift`: where the island panel goes on a screen. Pure geometry: the displays come in as
//! plain data (`ScreenInfo`), so no AppKit. Frames are in screen coordinates (origin bottom-left), and
//! `visible_frame` is `NSScreen.visibleFrame`: it already excludes the menu bar and the Dock.

use crate::position_store::IslandPosition;

/// Space between the bottom of the menu bar and the top of the island (mockup: 7 pt).
pub const GAP_BELOW_MENU_BAR: f64 = 8.0;
/// The panel's transparent margin around the surfaces: it may overlap the menu bar or the screen edge, since clicks
/// there go through.
pub const OVERHANG: f64 = 24.0;
/// Space between the bottom of the menu bar item and the top of the popover.
pub const GAP_BELOW_ITEM: f64 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

impl Size {
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

/// A `CGRect` with a positive size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn min_x(&self) -> f64 {
        self.x
    }
    pub fn min_y(&self) -> f64 {
        self.y
    }
    pub fn max_x(&self) -> f64 {
        self.x + self.width
    }
    pub fn max_y(&self) -> f64 {
        self.y + self.height
    }
    pub fn mid_x(&self) -> f64 {
        self.x + self.width / 2.0
    }
    pub fn mid_y(&self) -> f64 {
        self.y + self.height / 2.0
    }
    pub fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    /// `CGRect.insetBy`: a negative inset grows the rect.
    pub fn inset_by(&self, dx: f64, dy: f64) -> Rect {
        Rect::new(
            self.x + dx,
            self.y + dy,
            self.width - 2.0 * dx,
            self.height - 2.0 * dy,
        )
    }

    /// `CGRect.contains(CGPoint)`: the min edges are inside, the max edges are not.
    pub fn contains_point(&self, p: Point) -> bool {
        p.x >= self.min_x() && p.x < self.max_x() && p.y >= self.min_y() && p.y < self.max_y()
    }

    /// `CGRect.contains(CGRect)`: `other` lies wholly inside, edges included.
    pub fn contains_rect(&self, other: Rect) -> bool {
        other.min_x() >= self.min_x()
            && other.max_x() <= self.max_x()
            && other.min_y() >= self.min_y()
            && other.max_y() <= self.max_y()
    }
}

/// A screen as the placement needs it: its display id (`CGDirectDisplayID`) and `NSScreen.visibleFrame`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenInfo {
    pub id: u32,
    pub visible_frame: Rect,
}

/// Top centre of the screen, the island just below the menu bar.
pub fn initial_frame(size: Size, visible_frame: Rect) -> Rect {
    Rect::new(
        visible_frame.mid_x() - size.width / 2.0,
        visible_frame.max_y() - GAP_BELOW_MENU_BAR + OVERHANG - size.height,
        size.width,
        size.height,
    )
}

/// Moves the frame so its surfaces (the frame minus the margin) are inside `visible`: the open popover stays on
/// screen.
pub fn clamped(frame: Rect, visible: Rect) -> Rect {
    let area = visible.inset_by(-OVERHANG, -OVERHANG);
    let mut f = frame;
    f.x = f.min_x().max(area.min_x()).min(area.max_x() - f.width);
    f.y = f.min_y().max(area.min_y()).min(area.max_y() - f.height);
    f
}

/// The saved position on its screen, kept on screen; the top centre of the main screen when that screen is gone.
pub fn restored_frame(
    size: Size,
    saved: Option<&IslandPosition>,
    screens: &[ScreenInfo],
    main: &ScreenInfo,
) -> Rect {
    let Some((saved, screen)) = saved.and_then(|saved| {
        screens
            .iter()
            .find(|s| s.id == saved.display_id)
            .map(|screen| (saved, screen))
    }) else {
        return initial_frame(size, main.visible_frame);
    };
    let [x, y] = saved.top_center;
    let frame = Rect::new(
        x - size.width / 2.0,
        y - size.height,
        size.width,
        size.height,
    );
    clamped(frame, screen.visible_frame)
}

/// The first screen whose visible frame holds `point`.
pub fn screen_containing(point: Point, screens: &[ScreenInfo]) -> Option<&ScreenInfo> {
    screens
        .iter()
        .find(|s| s.visible_frame.contains_point(point))
}

/// Menu bar mode: the panel whose popover hangs under the status item `item`, centred on it and kept on screen.
pub fn under_item(item: Rect, size: Size, visible_frame: Rect) -> Rect {
    let mut f = Rect::new(
        item.mid_x() - size.width / 2.0,
        item.min_y() - GAP_BELOW_ITEM + OVERHANG - size.height,
        size.width,
        size.height,
    );
    let area = visible_frame.inset_by(-OVERHANG, 0.0);
    f.x = f.min_x().max(area.min_x()).min(area.max_x() - f.width);
    f
}
