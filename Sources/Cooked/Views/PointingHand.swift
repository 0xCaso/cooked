import AppKit
import SwiftUI

extension View {
    /// The pointing hand over a clickable surface, also while another app is in front.
    /// `pointerStyle(.link)` alone works only while Cooked is the active app: the panel is non-activating, so the
    /// app stays in the background and AppKit sends its cursor rects no updates. A tracking area that is always active
    /// sets the hand itself.
    func pointingHand() -> some View { modifier(PointingHand()) }
}

private struct PointingHand: ViewModifier {
    /// Snapshots render off screen, where an AppKit view draws as a placeholder.
    @Environment(\.solidSurfaces) private var offScreen

    func body(content: Content) -> some View {
        content.pointerStyle(.link).background { if !offScreen { PointingHandArea() } }
    }
}

/// An invisible view whose tracking area sets the pointing hand while the pointer is over it.
private struct PointingHandArea: NSViewRepresentable {
    func makeNSView(context: Context) -> TrackingView { TrackingView() }
    func updateNSView(_ view: TrackingView, context: Context) {}

    final class TrackingView: NSView {
        /// How many areas hold the pointer: moving from one to the next, the old exit can arrive after the new enter.
        @MainActor private static var inside = 0
        private var holds = false

        // Clicks go to the SwiftUI control above, never to this view.
        override func hitTest(_ point: NSPoint) -> NSView? { nil }

        override func updateTrackingAreas() {
            super.updateTrackingAreas()
            for area in trackingAreas { removeTrackingArea(area) }
            addTrackingArea(NSTrackingArea(rect: .zero, options: [.activeAlways, .inVisibleRect, .mouseEnteredAndExited, .mouseMoved],
                                           owner: self))
        }

        override func mouseEntered(with event: NSEvent) { hold() }
        /// The panel starts taking the mouse only once the pointer is over a surface, so the first event may be a move.
        override func mouseMoved(with event: NSEvent) { hold() }
        override func mouseExited(with event: NSEvent) { release() }

        override func viewWillMove(toWindow newWindow: NSWindow?) {
            super.viewWillMove(toWindow: newWindow)
            if newWindow == nil { release() } // the popover closed under the pointer
        }

        private func hold() {
            if !holds {
                holds = true
                Self.inside += 1
            }
            NSCursor.pointingHand.set()
        }

        private func release() {
            guard holds else { return }
            holds = false
            Self.inside -= 1
            if Self.inside == 0 { NSCursor.arrow.set() }
        }
    }
}

enum BackgroundCursor {
    /// A background app may not change the pointer unless its connection to the window server says so.
    /// `SetsCursorInBackground` is a private CoreGraphics property, looked up at run time: if it is missing, nothing
    /// happens and the hand shows only while Cooked is active, as before.
    @MainActor static func enable() {
        typealias MainConnection = @convention(c) () -> Int32
        typealias SetProperty = @convention(c) (Int32, Int32, CFString, CFTypeRef) -> Int32
        guard let cg = dlopen("/System/Library/Frameworks/ApplicationServices.framework/ApplicationServices", RTLD_LAZY),
              let main = dlsym(cg, "_CGSDefaultConnection") ?? dlsym(cg, "CGSMainConnectionID"),
              let set = dlsym(cg, "CGSSetConnectionProperty") else { return }
        let connection = unsafeBitCast(main, to: MainConnection.self)()
        _ = unsafeBitCast(set, to: SetProperty.self)(connection, connection, "SetsCursorInBackground" as CFString, kCFBooleanTrue)
    }
}
