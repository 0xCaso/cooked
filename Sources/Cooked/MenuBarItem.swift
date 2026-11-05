import AppKit
import SwiftUI

/// The menu bar's dot wave: 9 columns × 5 rows, mirrored around the centre row. Each column is one of the last
/// 9 minutes against the alert threshold: the centre dot only, then 3 dots, then all 5 from 90% of the threshold.
enum MenuBarWave {
    static let cols = 9

    /// Half-height of each column (0, 1 or 2 dots above and below the centre), oldest minute first.
    static func heights(_ values: [Double], threshold: Double) -> [Int] {
        let last = Array(values.suffix(cols))
        let padded = Array(repeating: 0.0, count: cols - last.count) + last
        return padded.map { v in
            let level = threshold > 0 ? v / threshold : 0
            return level >= 0.9 ? 2 : level >= 0.4 ? 1 : 0
        }
    }
}

/// What the menu bar item shows: the dot wave and the active account's 5-hour percent left ("—" when unknown).
/// Ink follows the menu bar (dark on a light bar, light on a dark one); in a peak the lit dots and the percent are red.
struct MenuBarLabel: View {
    let model: IslandModel

    /// Red text on a light menu bar needs a darker red to reach 4.5:1.
    static let red = Color(nsColor: NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
            ? NSColor(srgbRed: 1, green: 69 / 255, blue: 58 / 255, alpha: 1)
            : NSColor(srgbRed: 215 / 255, green: 0, blue: 21 / 255, alpha: 1)
    })

    var body: some View {
        let s = model.snapshot
        let urgent = s.isUrgent
        HStack(spacing: 6) {
            wave(MenuBarWave.heights(s.series, threshold: Double(s.chartThreshold)), urgent: urgent)
            Text(s.active?.fiveHourLeftIfKnown.map { "\($0)%" } ?? "—")
                .textStyle(.numberEmphasis)
                .foregroundStyle(urgent ? Self.red : Color.primary)
        }
        .padding(.horizontal, 6)
        .fixedSize()
    }

    private func wave(_ heights: [Int], urgent: Bool) -> some View {
        let pitch: CGFloat = 3.4, radius: CGFloat = 1.2
        return Canvas { ctx, _ in
            for (c, h) in heights.enumerated() {
                for k in -2...2 {
                    let lit = abs(k) <= h
                    let center = CGPoint(x: (CGFloat(c) + 0.5) * pitch, y: (CGFloat(k + 2) + 0.5) * pitch)
                    let dot = Path(ellipseIn: CGRect(x: center.x - radius, y: center.y - radius, width: 2 * radius, height: 2 * radius))
                    ctx.fill(dot, with: lit ? .color(urgent ? Self.red : .primary) : .color(.primary.opacity(0.18)))
                }
            }
        }
        .frame(width: CGFloat(MenuBarWave.cols) * pitch, height: 5 * pitch)
    }
}

/// The status item of menu bar mode. A click opens or closes the popover under it; a right-click shows the menu.
@MainActor
final class MenuBarItem: NSObject {
    private let model: IslandModel
    private let item: NSStatusItem
    private let label: NSHostingView<AnyView>
    /// Places the panel under the item before the popover opens.
    private let place: (CGRect) -> Void

    init(model: IslandModel, place: @escaping (CGRect) -> Void) {
        self.model = model
        self.place = place
        item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        label = PassThroughHostingView(rootView: AnyView(MenuBarLabel(model: model)))
        super.init()
        guard let button = item.button else { return }
        button.addSubview(label)
        button.target = self
        button.action = #selector(clicked)
        button.sendAction(on: [.leftMouseUp, .rightMouseUp])
        button.setAccessibilityLabel("Cooked")
        resize()
        observe()
    }

    func remove() { NSStatusBar.system.removeStatusItem(item) }

    /// The item's frame on screen, for the popover.
    var screenFrame: CGRect? { item.button?.window?.frame }

    func openPopover() {
        if let frame = screenFrame { place(frame) }
        model.open()
    }

    @objc private func clicked() {
        if NSApp.currentEvent?.type == .rightMouseUp {
            model.close()
            showMenu()
        } else if model.isOpen {
            model.close()
        } else {
            openPopover()
        }
    }

    /// The label is as wide as its text: the item follows it.
    private func resize() {
        let size = label.fittingSize
        item.length = size.width
        label.frame = CGRect(x: 0, y: 0, width: size.width, height: item.button?.bounds.height ?? NSStatusBar.system.thickness)
    }

    private func observe() {
        withObservationTracking { _ = model.snapshot.active?.fiveHourLeftIfKnown } onChange: { [weak self] in
            Task { @MainActor in
                self?.resize()
                self?.observe()
            }
        }
    }

    // MARK: Menu, the same as the island's right-click menu

    private func showMenu() {
        let menu = NSMenu()
        menu.addItem(withTitle: "Data", action: nil, keyEquivalent: "").isEnabled = false
        for feed in IslandModel.Feed.allCases {
            menu.addItem(choice(feed.title, on: model.feed == feed) { [model] in model.select(feed) })
        }
        menu.addItem(.separator())
        menu.addItem(submenu("Glass", GlassStyle.allCases.map { g in
            choice(g.title, on: model.glassStyle == g) { [model] in model.glassStyle = g }
        }))
        menu.addItem(submenu("Show in", DisplayMode.allCases.map { m in
            choice(m.title, on: model.displayMode == m) { [model] in model.displayMode = m }
        }))
        menu.addItem(.separator())
        menu.addItem(choice("Settings…", on: false) { [model] in model.openSettings(nil) })
        menu.addItem(choice("Quit Cooked", on: false) { NSApp.terminate(nil) })
        guard let button = item.button else { return }
        menu.popUp(positioning: nil, at: NSPoint(x: 0, y: button.bounds.height + 4), in: button)
    }

    private func submenu(_ title: String, _ items: [NSMenuItem]) -> NSMenuItem {
        let parent = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        let sub = NSMenu(title: title)
        items.forEach(sub.addItem)
        parent.submenu = sub
        return parent
    }

    private func choice(_ title: String, on: Bool, _ run: @escaping @MainActor () -> Void) -> NSMenuItem {
        let menuItem = NSMenuItem(title: title, action: #selector(MenuAction.perform(_:)), keyEquivalent: "")
        let action = MenuAction(run)
        menuItem.target = action
        menuItem.representedObject = action // the item keeps its target alive
        menuItem.state = on ? .on : .off
        return menuItem
    }
}

@MainActor
private final class MenuAction: NSObject {
    let run: @MainActor () -> Void
    init(_ run: @escaping @MainActor () -> Void) { self.run = run }
    @objc func perform(_ sender: Any?) { run() }
}

/// Lets clicks through to the status item's button.
private final class PassThroughHostingView: NSHostingView<AnyView> {
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
}
