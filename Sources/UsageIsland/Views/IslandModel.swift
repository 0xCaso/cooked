import AppKit
import Observation
import SwiftUI

/// What the island shows and whether the popover is open: live data, or one of the two demos.
@Observable @MainActor
final class IslandModel {
    var snapshot: UsageSnapshot {
        didSet { if !snapshot.isUrgent { peakMinimized = false } }
    }
    /// The user shrank the peak card back to the calm capsule. Lasts until the peak ends.
    var peakMinimized = false
    /// The peak card, unless the user shrank it.
    var showsPeakCard: Bool { snapshot.isUrgent && !peakMinimized }
    var isOpen = false
    /// Tokens per minute right now. In the calm demo it rolls through `DemoData.calmRates`.
    var rateNow: Int
    /// The visible surfaces (island, popover) in the panel's top-left coordinates, for click-through.
    var hitRects: [CGRect] = []
    /// The popover histogram, rebuilt when the feed changes; live data updates it in place.
    private(set) var histogram: any HistogramSource
    /// Live data, or a demo, chosen in the island's right-click menu.
    private(set) var feed: Feed
    /// Why live data is missing, shown in the menu. nil while live data arrives.
    private(set) var liveProblem: UsageSource.Problem?
    @ObservationIgnored private var liveHistogram: LiveHistogram?
    @ObservationIgnored private var calmRateIndex = 0
    /// Time zero of every chart. Lives as long as the model, so clicks and demo switches don't restart the wave.
    let chartEpoch = Date()
    /// The glass look, chosen in the island's right-click menu and kept across launches.
    var glassStyle: GlassStyle {
        didSet { defaults.set(glassStyle.rawValue, forKey: GlassStyle.defaultsKey) }
    }
    @ObservationIgnored private let defaults: UserDefaults
    /// Moves the panel while the island is dragged (the panel itself).
    @ObservationIgnored weak var mover: IslandMover?

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
        self.feed = feed ?? (snapshot.isUrgent ? .demoPeak : .demoCalm)
        glassStyle = defaults.string(forKey: GlassStyle.defaultsKey).flatMap(GlassStyle.init(rawValue:)) ?? .darker
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
            liveProblem = nil
            snapshot = new == .demoPeak ? DemoData.peak : DemoData.calm
            calmRateIndex = 0
            rateNow = snapshot.rateNow
            histogram = Self.demoClock(snapshot)
        }
    }

    /// A live reading (or why there is none).
    func apply(_ result: Result<UsageSource.Reading, UsageSource.Problem>) {
        guard feed == .live else { return }
        switch result {
        case .failure(let problem):
            liveProblem = problem
        case .success(let reading):
            liveProblem = nil
            let s = reading.snapshot
            if snapshot != s { snapshot = s }
            rateNow = s.rateNow
            if let liveHistogram {
                liveHistogram.update(values: s.series, threshold: Double(s.threshold), minute: reading.minute)
            } else {
                let h = LiveHistogram(values: s.series, threshold: Double(s.threshold), minute: reading.minute,
                                      epoch: chartEpoch.timeIntervalSince1970)
                liveHistogram = h
                histogram = h
            }
        }
    }

    /// The mockup's demo feed: one minute lasts 4 s, each new minute takes a value from the series, ±15%.
    static func demoClock(_ s: UsageSnapshot) -> HistogramClock {
        let source = s.series, n = source.count
        var feed = 0
        return HistogramClock(values: source, threshold: Double(s.threshold), minuteLength: 4) {
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
    }

    /// Every 3 s: the next calm rate. The peak and live data keep their rate.
    func tickRate() {
        guard feed == .demoCalm else { return }
        calmRateIndex = (calmRateIndex + 1) % DemoData.calmRates.count
        rateNow = DemoData.calmRates[calmRateIndex]
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

    /// Big enough for the tallest state (open, peak, every account in use, three sources), measured once.
    static let panelSize: CGSize = {
        let tallest = DemoData.peak
        let model = IslandModel(snapshot: tallest)
        model.isOpen = true
        let view = NSHostingView(rootView: AnyView(IslandRoot(model: model, forceStill: true).environment(\.solidSurfaces, true)))
        let fit = view.fittingSize
        return CGSize(width: popoverWidth + 2 * margin, height: ceil(fit.height))
    }()
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
