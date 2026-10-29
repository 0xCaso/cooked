import AppKit
import SwiftUI

/// `UsageIsland --snapshots <dir>`: renders parts of the island to PNG without a screen capture.
/// Charts are drawn still (t = 0). Glass cannot render off screen, so surfaces are drawn opaque.
@MainActor
enum Snapshots {
    static func write(to dir: URL) throws {
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let calm = DemoData.calm, peak = DemoData.peak
        func clock(_ s: UsageSnapshot) -> HistogramClock {
            HistogramClock(values: s.series, threshold: Double(s.threshold), minuteLength: 60) { 0 }
        }
        let charts: [(String, AnyView)] = [
            ("wave-calm", AnyView(TideIslandChart(rate: Double(calm.rateNow), threshold: Double(calm.threshold), urgent: false))),
            ("wave-peak", AnyView(TideIslandChart(rate: Double(peak.rateNow), threshold: Double(peak.threshold), urgent: true))),
            ("hist-calm", AnyView(TideHistogramChart(source: clock(calm), urgent: false))),
            ("hist-peak", AnyView(TideHistogramChart(source: clock(peak), urgent: true))),
            ("bars-peak", AnyView(VStack(spacing: 8) {
                ForEach(peak.accounts) { a in AccountBarChart(percent: a.fiveHourPercent, hot: a.id == "work") }
            })),
        ]
        for (name, view) in charts {
            try render(view.padding(8).background(Color(white: 0.06)), to: dir.appendingPathComponent("\(name).png"))
        }
        // Island and popover with opaque surfaces, on the mockup's desk colour (#1b1c1f).
        let desk = Color(red: 27 / 255, green: 28 / 255, blue: 31 / 255)
        for (name, snapshot) in [("calm", calm), ("peak", peak)] {
            let model = IslandModel(snapshot: snapshot)
            try render(SurfaceShot { IslandView(model: model, namespace: $0) }.padding(24).background(desk),
                       to: dir.appendingPathComponent("island-\(name).png"))
            try render(SurfaceShot { PopoverView(model: model, histogram: clock(snapshot), namespace: $0) }.padding(24).background(desk),
                       to: dir.appendingPathComponent("popover-\(name).png"))
            if snapshot.isUrgent {
                model.peakMinimized = true
                try render(SurfaceShot { IslandView(model: model, namespace: $0) }.padding(24).background(desk),
                           to: dir.appendingPathComponent("island-\(name)-minimized.png"))
                model.peakMinimized = false
            }
            model.open()
            try render(IslandRoot(model: model, forceStill: true).background(desk),
                       to: dir.appendingPathComponent("open-\(name).png"))
        }
    }

    /// `--snapshots <dir> --live`: the island and the open popover from the real status.json and database.
    static func writeLive(to dir: URL) throws {
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let model = IslandModel(feed: .live)
        let now = Date()
        model.apply(UsageSource().read(now: now))
        if let problem = model.liveProblem { throw CocoaError(.fileReadUnknown, userInfo: [NSLocalizedDescriptionKey: problem.message]) }
        let desk = Color(red: 27 / 255, green: 28 / 255, blue: 31 / 255)
        try render(IslandRoot(model: model, forceStill: true).background(desk), to: dir.appendingPathComponent("live-closed.png"))
        model.open()
        try render(IslandRoot(model: model, forceStill: true).background(desk), to: dir.appendingPathComponent("live-open.png"))
    }

    /// Gives a surface the glass namespace it needs.
    private struct SurfaceShot<Content: View>: View {
        @ViewBuilder var content: (Namespace.ID) -> Content
        @Namespace private var ns
        var body: some View { content(ns) }
    }

    static func render(_ view: some View, to url: URL) throws {
        let renderer = ImageRenderer(content: view.environment(\.chartsStill, true).environment(\.colorScheme, .dark)
            .environment(\.solidSurfaces, true))
        renderer.scale = 2
        guard let image = renderer.cgImage else { throw CocoaError(.fileWriteUnknown) }
        let rep = NSBitmapImageRep(cgImage: image)
        guard let png = rep.representation(using: .png, properties: [:]) else { throw CocoaError(.fileWriteUnknown) }
        try png.write(to: url)
    }
}
