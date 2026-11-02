import AppKit

@main
@MainActor
enum CookedApp {
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
        // `--limits`: every account's limits and alert line as the island reads them, one line each.
        if args.contains("--limits") {
            let source = UsageSource()
            var reading = source.read()
            while case .success(let r) = reading, r.history != nil { reading = source.read() }
            let status = source.limits.read(now: Date(), buckets: source.ledger.buckets15)
            for (id, e) in status.accounts.sorted(by: { $0.key < $1.key }) {
                print(id, "5h \(e.fiveHourPct ?? -1)% reset \(e.fiveHourResetsAt ?? "-")", "week \(e.sevenDayPct ?? -1)% reset \(e.sevenDayResetsAt ?? "-")",
                      "status \(e.limitStatus ?? "-") last \(e.lastEventAt ?? "-") live \(e.live ?? false)",
                      "eta \(e.projection.map { "\($0.eta) (\($0.window))" } ?? "-") alert \(e.spikeThreshold15m ?? -1)",
                      e.note ?? "")
            }
            print("all accounts: alert \(status.totalThreshold15m ?? -1)")
            exit(0)
        }
        // `--statusline status|connect|disconnect`: the status line bridge of each connected Claude account's settings
        // file, until the Settings window has the button.
        if let i = args.firstIndex(of: "--statusline") {
            let action = i + 1 < args.count ? args[i + 1] : "status"
            let setup = StatusLineSetup()
            var done = Set<String>()
            for account in AccountStore.current() where account.kind == .claude {
                let settings = account.path + "/settings.json"
                let real = StatusLineSetup.real(settings)
                if done.insert(real).inserted {
                    do {
                        switch action {
                        case "connect": try setup.connect(settings: settings)
                        case "disconnect": print(try setup.disconnect(settings: settings) ? "given back" : "not touched", real)
                        default: break
                        }
                    } catch { print(real, error); exit(1) }
                }
                print(account.id, real, setup.state(settings: settings))
            }
            exit(0)
        }
        // Before the model reads its settings.
        LegacyDefaults.migrate()
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
        BackgroundCursor.enable()
        let panel = IslandPanel(model: model)
        panel.restorePosition()
        panel.orderFrontRegardless()
        self.panel = panel
        // `--feed demoCalm|demoPeak|live`: start on that feed, to check a state on the real glass from the terminal.
        let launchArgs = CommandLine.arguments
        if let i = launchArgs.firstIndex(of: "--feed"), i + 1 < launchArgs.count, let feed = IslandModel.Feed(rawValue: launchArgs[i + 1]) {
            model.select(feed)
        }
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
