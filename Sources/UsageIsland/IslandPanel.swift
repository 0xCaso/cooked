import AppKit
import SwiftUI

/// Borderless, transparent panel under the menu bar that hosts the island and its popover.
/// It never takes focus, shows on every Space and stays above full-screen apps.
/// Fixed size (the open peak state), so it never resizes: clicks on its transparent part go to the window below.
@MainActor
final class IslandPanel: NSPanel, IslandMover {
    let model: IslandModel
    private var monitors: [Any] = []
    /// While the panel takes the mouse it gets no moves from other apps, and as a non-key panel maybe none of its own:
    /// it checks the pointer itself until the pointer leaves the surfaces.
    private var insideTimer: Timer?
    /// A context menu is open: it tracks the mouse itself, outside the island, so leave the panel as it is.
    private var menuTracking = false
    /// Clicks in other apps close the popover; only installed while it is open.
    private var outsideClicks: Any?
    private var wasOpen = false
    /// Pointer and panel origin when the island was pressed.
    private(set) var dragStart: (mouse: CGPoint, origin: CGPoint)?
    /// The island has moved past the tap distance in this press.
    private(set) var isDragging = false
    /// The popover closed during a drag: give the keyboard back when the drag ends, not while the mouse is down.
    private var handBackAfterDrag = false

    init(model: IslandModel) {
        self.model = model
        let size = IslandLayout.panelSize
        let content = NSHostingView(rootView: IslandRoot(model: model, source: UsageSource())
            .frame(width: size.width, height: size.height, alignment: .top))
        content.sizingOptions = []

        super.init(
            contentRect: NSRect(origin: .zero, size: size),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )

        contentView = content
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        level = .statusBar
        collectionBehavior = [.canJoinAllSpaces, .stationary, .fullScreenAuxiliary, .ignoresCycle]
        hidesOnDeactivate = false
        // No fade on order out/in: handing the keyboard back re-orders the panel, and the fade made the island blink.
        animationBehavior = .none
        isReleasedWhenClosed = false
        becomesKeyOnlyIfNeeded = true
        appearance = NSAppearance(named: .darkAqua)
        ignoresMouseEvents = true
        acceptsMouseMovedEvents = true
        model.mover = self
        watchPointer()
        NotificationCenter.default.addObserver(forName: NSApplication.didChangeScreenParametersNotification, object: nil,
                                               queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.restorePosition() }
        }
    }

    /// Key only while the popover is open, so Esc reaches it. The panel is nonactivating: the app in use stays in front.
    override var canBecomeKey: Bool { model.isOpen }
    override var canBecomeMain: Bool { false }

    override func cancelOperation(_ sender: Any?) {
        model.dismiss(.escape)
    }

    /// The panel holds the keyboard only for Esc. Any other key means the user went back to typing elsewhere:
    /// close and hand the keyboard back, so at most this one key is lost, and without a beep.
    override func keyDown(with event: NSEvent) {
        model.dismiss(.typing)
    }

    override func resignKey() {
        super.resignKey()
        model.dismiss(.resignedKey)
    }

    private func openChanged() {
        let open = model.isOpen
        guard open != wasOpen else { return }
        wasOpen = open
        if open {
            makeKey()
            outsideClicks = NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { [weak self] _ in
                MainActor.assumeIsolated { self?.model.dismiss(.clickOutside) }
            }
        } else {
            if let outsideClicks { NSEvent.removeMonitor(outsideClicks) }
            outsideClicks = nil
            if isDragging { handBackAfterDrag = true } else { handBackKeyboard() }
        }
    }

    /// Hand the keyboard back to the app in use: a key window that goes off screen gives it up.
    private func handBackKeyboard() {
        guard isKeyWindow else { return }
        orderOut(nil)
        orderFrontRegardless()
    }

    /// Where the island was left, on the same screen; top centre of the main screen the first time.
    func restorePosition() {
        let screens = NSScreen.screens.map(\.info)
        guard let main = (NSScreen.main ?? NSScreen.screens.first)?.info else { return }
        setFrame(IslandPlacement.restoredFrame(size: frame.size, saved: PositionStore.load(), screens: screens, main: main),
                 display: true)
    }

    func dragBegan() {
        dragStart = (NSEvent.mouseLocation, frame.origin)
        isDragging = false
    }

    func dragChanged() {
        let mouse = NSEvent.mouseLocation
        guard let start = dragStart else { return }
        if !isDragging {
            isDragging = true
            model.close()
        }
        var moved = frame
        moved.origin = CGPoint(x: start.origin.x + mouse.x - start.mouse.x, y: start.origin.y + mouse.y - start.mouse.y)
        let screens = NSScreen.screens.map(\.info)
        if let screen = IslandPlacement.screen(containing: mouse, in: screens) ?? screen?.info {
            moved = IslandPlacement.clamped(moved, to: screen.visibleFrame)
        }
        setFrameOrigin(moved.origin)
    }

    func dragEnded() {
        dragStart = nil
        isDragging = false
        if handBackAfterDrag {
            handBackAfterDrag = false
            handBackKeyboard()
        }
        guard let id = screen?.info.id else { return }
        PositionStore.save(SavedPosition(displayID: id, topCenter: CGPoint(x: frame.midX, y: frame.maxY)))
    }

    /// The panel takes the mouse only while the pointer is over the island or the popover.
    private func watchPointer() {
        let update: (NSEvent) -> Void = { [weak self] _ in
            MainActor.assumeIsolated { self?.updateMouseEvents() }
        }
        let mask: NSEvent.EventTypeMask = [.mouseMoved, .leftMouseDragged, .rightMouseDown, .leftMouseDown]
        if let global = NSEvent.addGlobalMonitorForEvents(matching: mask, handler: update) { monitors.append(global) }
        if let local = NSEvent.addLocalMonitorForEvents(matching: mask, handler: { event in update(event); return event }) {
            monitors.append(local)
        }
        observeHitRects()
        let center = NotificationCenter.default
        center.addObserver(forName: NSMenu.didBeginTrackingNotification, object: nil, queue: nil) { [weak self] _ in
            MainActor.assumeIsolated { self?.menuTracking = true }
        }
        center.addObserver(forName: NSMenu.didEndTrackingNotification, object: nil, queue: nil) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.menuTracking = false
                self?.updateMouseEvents()
            }
        }
    }

    /// Also when the surfaces change under a still pointer (popover opens or closes).
    private func observeHitRects() {
        withObservationTracking { _ = (model.hitRects, model.isOpen) } onChange: { [weak self] in
            Task { @MainActor in
                self?.openChanged()
                self?.updateMouseEvents()
                self?.observeHitRects()
            }
        }
    }

    func updateMouseEvents() {
        guard !menuTracking else { return }
        let inside = IslandHitTest.hits(NSEvent.mouseLocation, rects: model.hitRects, panelFrame: frame)
        if ignoresMouseEvents == inside { ignoresMouseEvents = !inside }
        if inside, insideTimer == nil {
            insideTimer = Timer.scheduledTimer(withTimeInterval: 0.05, repeats: true) { [weak self] _ in
                MainActor.assumeIsolated { self?.updateMouseEvents() }
            }
        } else if !inside {
            insideTimer?.invalidate()
            insideTimer = nil
        }
    }
}

private extension NSScreen {
    var info: ScreenInfo {
        let id = (deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.uint32Value ?? 0
        return ScreenInfo(id: id, visibleFrame: visibleFrame)
    }
}
