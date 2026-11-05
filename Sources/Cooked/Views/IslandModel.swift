import AppKit
import Observation
import SwiftUI

/// What the island shows and whether the popover is open: live data, or one of the two demos.
@Observable @MainActor
final class IslandModel {
    var snapshot: UsageSnapshot
    var isOpen = false
    /// The "What burns tokens" list starts open: only to measure the panel.
    var sourcesExpanded = false
    /// Tokens per minute right now. In the calm demo it rolls through `DemoData.calmRates`.
    var rateNow: Int
    /// The popover's "now": every account summed in live data, else `rateNow`.
    var chartRate: Int { snapshot.totalRate ?? rateNow }
    /// The visible surfaces (island, popover) in the panel's top-left coordinates, for click-through.
    var hitRects: [CGRect] = []
    /// The popover histogram, rebuilt when the feed changes; live data updates it in place.
    private(set) var histogram: any HistogramSource
    /// Live data, or a demo, chosen in the island's right-click menu.
    private(set) var feed: Feed
    /// What live data lacks and how to fix it, first the one to show (`notice`).
    private(set) var notices: [Notice] = []
    /// The notices put off with "Not now" or "OK", kept until the accounts change.
    private(set) var dismissed: Set<String>
    /// The popover's notice card: the first notice not put off.
    var notice: Notice? { Notice.first(notices, dismissed: dismissed) }
    /// 0...1 while the first launch reads 14 days of history; nil when complete or in a demo.
    private(set) var historyProgress: Double?
    @ObservationIgnored private var liveHistogram: LiveHistogram?
    @ObservationIgnored private var calmRateIndex = 0
    /// Time zero of every chart. Lives as long as the model, so clicks and demo switches don't restart the wave.
    let chartEpoch = Date()
    /// The glass look, chosen in the island's right-click menu and kept across launches.
    var glassStyle: GlassStyle {
        didSet { defaults.set(glassStyle.rawValue, forKey: GlassStyle.defaultsKey) }
    }
    /// Where Cooked shows when closed: the island under the menu bar, or an item in the menu bar. Kept across launches.
    var displayMode: DisplayMode {
        didSet { defaults.set(displayMode.rawValue, forKey: DisplayMode.defaultsKey) }
    }
    /// How far back the popover's chart looks, kept across launches.
    var chartRange: ChartRange {
        didSet { defaults.set(chartRange.rawValue, forKey: ChartRange.defaultsKey) }
    }
    @ObservationIgnored private let defaults: UserDefaults
    /// Moves the panel while the island is dragged (the panel itself).
    @ObservationIgnored weak var mover: IslandMover?
    /// Goes up when Setup or Settings change the accounts: the live feed starts a new reader.
    private(set) var accountsVersion = 0
    /// Opens the Settings window, on a tab or the first one (the app delegate sets it).
    @ObservationIgnored var openSettings: (SettingsView.Tab?) -> Void = { _ in }
    /// Opens the Setup window (the app delegate sets it).
    @ObservationIgnored var openSetup: () -> Void = {}
    static let dismissedKey = "dismissedNotices"

    func accountsChanged() {
        accountsVersion += 1
        dismissed = []
        defaults.removeObject(forKey: Self.dismissedKey)
    }

    /// The notice card's button.
    func act(on notice: Notice) {
        switch notice {
        case .noAccounts: openSetup()
        case .paused, .noPercent: openSettings(.percent)
        case .formatChanged:
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(notice.details ?? notice.title, forType: .string)
        case .apiKey: dismiss(notice)
        }
    }

    /// "Not now" and "OK": hides the notice until the accounts change.
    func dismiss(_ notice: Notice) {
        guard let key = notice.dismissKey else { return }
        dismissed.insert(key)
        defaults.set(Array(dismissed).sorted(), forKey: Self.dismissedKey)
    }

    /// Shows notices without live data: Snapshots and the panel's measure.
    func show(_ notices: [Notice]) { self.notices = notices }

    enum Feed: String, CaseIterable { case live, demoCalm, demoPeak
        var title: String {
            switch self {
            case .live: "Live"
            case .demoCalm: "Demo: calm"
            case .demoPeak: "Demo: peak"
            }
        }
    }

    init(snapshot: UsageSnapshot = DemoData.calm, feed: Feed? = nil, defaults: UserDefaults = .standard) {
        self.defaults = defaults
        dismissed = Set(defaults.stringArray(forKey: Self.dismissedKey) ?? [])
        self.feed = feed ?? (snapshot.isUrgent ? .demoPeak : .demoCalm)
        glassStyle = defaults.string(forKey: GlassStyle.defaultsKey).flatMap(GlassStyle.init(rawValue:)) ?? .darker
        displayMode = defaults.string(forKey: DisplayMode.defaultsKey).flatMap(DisplayMode.init(rawValue:)) ?? .island
        chartRange = defaults.string(forKey: ChartRange.defaultsKey).flatMap(ChartRange.init(rawValue:)) ?? .tenMinutes
        self.snapshot = snapshot
        rateNow = snapshot.rateNow
        histogram = Self.demoClock(snapshot)
    }

    func toggleDemo() {
        select(snapshot.isUrgent ? .demoCalm : .demoPeak)
    }

    /// Switches the feed. Live keeps what is on screen until the first reading arrives.
    func select(_ new: Feed) {
        feed = new
        switch new {
        case .live:
            liveHistogram = nil
        case .demoCalm, .demoPeak:
            liveHistogram = nil
            notices = []
            historyProgress = nil
            snapshot = new == .demoPeak ? DemoData.peak : DemoData.calm
            calmRateIndex = 0
            rateNow = snapshot.rateNow
            histogram = Self.demoClock(snapshot)
        }
    }

    /// A live reading.
    func apply(_ reading: UsageSource.Reading) {
        guard feed == .live else { return }
        if notices != reading.notices { notices = reading.notices }
        if historyProgress != reading.history { historyProgress = reading.history }
        let s = reading.snapshot
        if snapshot != s { snapshot = s }
        rateNow = s.rateNow
        if let liveHistogram {
            liveHistogram.update(values: s.series, threshold: Double(s.chartThreshold), minute: reading.minute)
        } else {
            let h = LiveHistogram(values: s.series, threshold: Double(s.chartThreshold), minute: reading.minute,
                                  epoch: chartEpoch.timeIntervalSince1970)
            liveHistogram = h
            histogram = h
        }
    }

    /// The mockup's demo feed: one minute lasts 4 s, each new minute takes a value from the series, ±15%.
    static func demoClock(_ s: UsageSnapshot) -> HistogramClock {
        let source = s.series, n = source.count
        var feed = 0
        return HistogramClock(values: source, threshold: Double(s.chartThreshold), minuteLength: 4) {
            let v = source[(feed * 7) % max(1, n - 10) + 5]
            feed += 1
            return v * (0.85 + 0.3 * Double((feed * 37) % 10) / 10)
        }
    }

    func open() { isOpen = true }
    func close() { isOpen = false }

    enum DismissReason { case escape, clickOutside, resignedKey, typing }

    /// Esc, a click outside the island and popover, or another window taking the keyboard.
    func dismiss(_ reason: DismissReason) {
        guard isOpen else { return }
        isOpen = false
        if reason == .clickOutside || reason == .resignedKey { closedByMouseAt = Date() }
    }

    /// When a click elsewhere last closed the popover. The menu bar item lives outside the app on macOS 26, so the
    /// mouse-down on it closes the popover before the item's click arrives.
    @ObservationIgnored private var closedByMouseAt: Date?

    /// A click elsewhere closed the popover in the last half second: the menu bar item's click is that same click,
    /// and must leave the popover closed.
    func closedByThisClick(now: Date = Date()) -> Bool {
        guard let closedByMouseAt else { return false }
        return now.timeIntervalSince(closedByMouseAt) < 0.5
    }

    /// Every 3 s: the next calm rate. The peak and live data keep their rate.
    func tickRate() {
        guard feed == .demoCalm else { return }
        calmRateIndex = (calmRateIndex + 1) % DemoData.calmRates.count
        rateNow = DemoData.calmRates[calmRateIndex]
    }
}

/// Where Cooked shows when the popover is closed.
enum DisplayMode: String, CaseIterable {
    case island, menuBar
    static let defaultsKey = "displayMode"
    var title: String {
        switch self {
        case .island: "Island"
        case .menuBar: "Menu bar"
        }
    }
}

@MainActor
protocol IslandMover: AnyObject {
    /// The island was pressed: a drag may follow.
    func dragBegan()
    /// The pointer moved while the island is held down, past the tap distance.
    func dragChanged()
    func dragEnded()
}

/// Fixed sizes of the island and its panel.
@MainActor
enum IslandLayout {
    static let popoverWidth: CGFloat = 404
    static let gap: CGFloat = 8
    /// Transparent margin around the surfaces, so glass edges and shadows are not clipped.
    static let margin: CGFloat = 24

    /// Big enough for the tallest state, measured once: open, peak, the full list of sources open, and every
    /// connected account in use (at least the demo's five).
    static let panelSize: CGSize = {
        let model = IslandModel(snapshot: tallest(accounts: AccountStore.current().count))
        model.isOpen = true
        model.sourcesExpanded = true
        model.show([.formatChanged(account: "measure", file: "")])
        let view = NSHostingView(rootView: AnyView(IslandRoot(model: model, forceStill: true).environment(\.solidSurfaces, true)))
        let fit = view.fittingSize
        return CGSize(width: popoverWidth + 2 * margin, height: min(ceil(fit.height), maxPanelHeight))
    }()

    /// The main screen under the menu bar: a taller popover scrolls its lower part.
    static var maxPanelHeight: CGFloat { (NSScreen.main ?? NSScreen.screens.first)?.visibleFrame.height ?? 945 }

    /// The demo peak with `TokenSources.limit` sources and at least `accounts` accounts in use.
    static func tallest(accounts: Int) -> UsageSnapshot {
        var s = DemoData.peak
        let template = s.sources[s.sources.count - 1]
        for i in s.sources.count..<max(s.sources.count, TokenSources.limit) {
            s.sources.append(TokenSource(id: "measure-\(i)", project: template.project, detail: template.detail,
                                         account: template.account, rate: template.rate, agents: template.agents))
        }
        let last = s.accounts[s.accounts.count - 1]
        for i in s.accounts.count..<max(s.accounts.count, accounts) {
            s.accounts.append(Account(id: "measure-\(i)", short: last.short, fiveHourPercent: last.fiveHourPercent,
                                      fiveHourReset: last.fiveHourReset, isLive: last.isLive, weekPercent: last.weekPercent,
                                      fiveHourElapsed: last.fiveHourElapsed, weekElapsed: last.weekElapsed, weekReset: last.weekReset))
        }
        return s
    }
}

/// Clicks on the transparent part of the panel go to the window below.
enum IslandHitTest {
    /// `point` and `panelFrame` in screen coordinates (bottom-left origin); `rects` in the panel's top-left coordinates.
    static func hits(_ point: CGPoint, rects: [CGRect], panelFrame: CGRect) -> Bool {
        guard panelFrame.contains(point) else { return false }
        let local = CGPoint(x: point.x - panelFrame.minX, y: panelFrame.maxY - point.y)
        return rects.contains { $0.contains(local) }
    }
}
