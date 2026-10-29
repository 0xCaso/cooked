import AppKit

@main
@MainActor
enum UsageIslandApp {
    static func main() {
        let app = NSApplication.shared
        let args = CommandLine.arguments
        if let i = args.firstIndex(of: "--snapshots"), i + 1 < args.count {
            AppDelegate.registerFontsOutsideTheBundle()
            let dir = URL(fileURLWithPath: args[i + 1])
            do {
                if args.contains("--live") { try Snapshots.writeLive(to: dir) } else { try Snapshots.write(to: dir) }
            } catch { print(error.localizedDescription); exit(1) }
            exit(0)
        }
        let delegate = AppDelegate()
        app.delegate = delegate
        // No Dock icon and no menu bar, also when run with `swift run` (the bundle sets LSUIElement too).
        app.setActivationPolicy(.accessory)
        withExtendedLifetime(delegate) { app.run() }
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private var panel: IslandPanel?
    let model = IslandModel(feed: .live)

    func applicationDidFinishLaunching(_ notification: Notification) {
        Self.registerFontsOutsideTheBundle()
        let panel = IslandPanel(model: model)
        panel.restorePosition()
        panel.orderFrontRegardless()
        self.panel = panel
        // `--open`: open the popover after a second, to check from the terminal that the app in use stays in front.
        if CommandLine.arguments.contains("--open") {
            DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [model] in model.open() }
        }
        // `--toggle-every <s>`: opens and closes the popover on a timer, to record the transitions without clicking.
        let args = CommandLine.arguments
        if let i = args.firstIndex(of: "--toggle-every"), i + 1 < args.count, let every = Double(args[i + 1]) {
            Timer.scheduledTimer(withTimeInterval: every, repeats: true) { [model] _ in
                MainActor.assumeIsolated { model.isOpen.toggle() }
            }
        }
    }

    /// Opening the app again (Applications, Spotlight, Dock) while it runs opens the popover.
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        model.open()
        return false
    }

    /// Registers Plex from the bundle, or from the repo under `swift run` (no bundle). The bundle also sets
    /// ATSApplicationFontsPath; registering here as well does not depend on that key accepting .woff files.
    static func registerFontsOutsideTheBundle() {
        PlexFont.registerBundled(in: fontsDirectory(resourceURL: Bundle.main.resourceURL))
    }

    static func fontsDirectory(resourceURL: URL?) -> URL {
        if let bundled = resourceURL?.appendingPathComponent("Fonts"), FileManager.default.fileExists(atPath: bundled.path) {
            return bundled
        }
        return URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("Resources/Fonts")
    }
}
