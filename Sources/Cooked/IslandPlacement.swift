import CoreGraphics

/// Where the panel was left: the top centre of the panel, on one display.
struct SavedPosition: Codable, Equatable {
    let displayID: UInt32
    let topCenter: CGPoint
}

/// A screen as the placement needs it: its display id and `NSScreen.visibleFrame`.
struct ScreenInfo: Equatable {
    let id: UInt32
    let visibleFrame: CGRect
}

/// Where the island panel goes on a screen. Pure geometry, so it can be tested.
/// Frames are in screen coordinates (origin bottom-left). `visibleFrame` already excludes the menu bar and the Dock.
enum IslandPlacement {
    /// Space between the bottom of the menu bar and the top of the island (mockup: 7 pt).
    static let gapBelowMenuBar: CGFloat = 8
    /// The panel's transparent margin around the surfaces (`IslandLayout.margin`): it may overlap the menu bar or the
    /// screen edge, since clicks there go through.
    static let overhang: CGFloat = 24

    /// Top centre of the screen, the island just below the menu bar.
    static func initialFrame(size: CGSize, visibleFrame: CGRect) -> CGRect {
        CGRect(
            x: visibleFrame.midX - size.width / 2,
            y: visibleFrame.maxY - gapBelowMenuBar + overhang - size.height,
            width: size.width,
            height: size.height
        )
    }

    /// Moves the frame so its surfaces (the frame minus the margin) are inside `visible`: the open popover stays on screen.
    static func clamped(_ frame: CGRect, to visible: CGRect) -> CGRect {
        let area = visible.insetBy(dx: -overhang, dy: -overhang)
        var f = frame
        f.origin.x = min(max(f.minX, area.minX), area.maxX - f.width)
        f.origin.y = min(max(f.minY, area.minY), area.maxY - f.height)
        return f
    }

    /// The saved position on its screen, kept on screen; the top centre of the main screen when that screen is gone.
    static func restoredFrame(size: CGSize, saved: SavedPosition?, screens: [ScreenInfo], main: ScreenInfo) -> CGRect {
        guard let saved, let screen = screens.first(where: { $0.id == saved.displayID }) else {
            return initialFrame(size: size, visibleFrame: main.visibleFrame)
        }
        let frame = CGRect(x: saved.topCenter.x - size.width / 2, y: saved.topCenter.y - size.height,
                           width: size.width, height: size.height)
        return clamped(frame, to: screen.visibleFrame)
    }

    static func screen(containing point: CGPoint, in screens: [ScreenInfo]) -> ScreenInfo? {
        screens.first { $0.visibleFrame.contains(point) }
    }

    /// Space between the bottom of the menu bar item and the top of the popover.
    static let gapBelowItem: CGFloat = 6

    /// Menu bar mode: the panel whose popover hangs under the status item `item`, centred on it and kept on screen.
    static func underItem(_ item: CGRect, size: CGSize, visibleFrame: CGRect) -> CGRect {
        let frame = CGRect(x: item.midX - size.width / 2, y: item.minY - gapBelowItem + overhang - size.height,
                           width: size.width, height: size.height)
        let area = visibleFrame.insetBy(dx: -overhang, dy: 0)
        var f = frame
        f.origin.x = min(max(f.minX, area.minX), area.maxX - f.width)
        return f
    }
}
