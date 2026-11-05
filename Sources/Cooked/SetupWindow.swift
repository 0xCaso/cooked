import AppKit
import SwiftUI

/// The Setup window (first launch) and the Settings window: one dark window, 600 pt wide, as tall as its content.
@MainActor
final class SetupWindows {
    private var window: NSWindow?
    private let onAccountsChange: () -> Void
    /// The island's model, for the "Show Cooked in" setting.
    private weak var island: IslandModel?

    init(island: IslandModel? = nil, onAccountsChange: @escaping () -> Void) {
        self.island = island
        self.onAccountsChange = onAccountsChange
    }

    func showSetup() {
        let model = makeModel()
        present(title: "Set up Cooked", SetupView(model: model) { [weak self] in self?.close() })
    }

    func showSettings(tab: SettingsView.Tab = .accounts) {
        let mode = island.map { island in
            Binding(get: { island.displayMode }, set: { island.displayMode = $0 })
        }
        present(title: "Cooked Settings", SettingsView(model: makeModel(), tab: tab, displayMode: mode))
    }

    private func makeModel() -> SetupModel {
        let model = SetupModel()
        model.onAccountsChange = onAccountsChange
        return model
    }

    private func present(title: String, _ view: some View) {
        window?.close()
        let host = NSHostingController(rootView: view.preferredColorScheme(.dark))
        host.sizingOptions = [.preferredContentSize]
        let window = NSWindow(contentViewController: host)
        window.title = title
        window.styleMask = [.titled, .closable]
        window.titlebarAppearsTransparent = true
        window.backgroundColor = NSColor(Palette.bgDeep)
        window.appearance = NSAppearance(named: .darkAqua)
        window.isReleasedWhenClosed = false
        window.center()
        self.window = window
        // An accessory app: come to the front so the window takes the keyboard.
        NSApp.activate()
        window.makeKeyAndOrderFront(nil)
    }

    func close() {
        window?.close()
        window = nil
    }
}
