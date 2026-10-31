import AppKit
import SwiftUI

/// `Cooked --snapshots <dir>`: renders parts of the island to PNG without a screen capture.
/// Charts are drawn still (t = 0). Glass cannot render off screen, so surfaces are drawn opaque.
@MainActor
enum Snapshots {
    static func write(to dir: URL) throws {
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let calm = DemoData.calm, peak = DemoData.peak
        var pastPeak = calm.series
        for m in 26...35 { pastPeak[m] = 52000 + Double((m * 7) % 5) * 6000 }
        func clock(_ s: UsageSnapshot) -> HistogramClock {
            HistogramClock(values: s.series, threshold: Double(s.threshold), minuteLength: 60) { 0 }
        }
        let charts: [(String, AnyView)] = [
            ("wave-calm", AnyView(TideIslandChart(rate: Double(calm.rateNow), threshold: Double(calm.threshold), urgent: false))),
            ("wave-peak", AnyView(TideIslandChart(rate: Double(peak.rateNow), threshold: Double(peak.threshold), urgent: true))),
            ("hist-calm", AnyView(TideHistogramChart(source: clock(calm)))),
            ("hist-peak", AnyView(TideHistogramChart(source: clock(peak)))),
            // Calm now, but ten minutes over the threshold half an hour ago: those columns are coral above the line.
            ("hist-past-peak", AnyView(TideHistogramChart(source: HistogramClock(values: pastPeak, threshold: Double(calm.threshold), minuteLength: 60) { 0 }))),
            ("pills-peak", AnyView(VStack(spacing: 8) {
                ForEach(peak.accounts) { a in
                    UsagePill(name: a.short, left: PopoverView.left(a.fiveHourPercent), timeLeft: a.fiveHourElapsed.map { 1 - $0 },
                              hot: a.fullAt != nil, live: a.isLive) {
                        PillTimes(full: a.fullAt, reset: a.fiveHourReset)
                    }.frame(width: 220)
                }
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
        // "What burns tokens" with the first line hovered (lifted), then the middle one (neighbours on both sides).
        for (name, index) in [("sources-hover", 0), ("sources-hover-middle", 1)] where peak.sources.indices.contains(index) {
            try render(SourcesCard(sources: peak.sources, hovered: peak.sources[index].id).frame(width: 388).padding(24).background(desk),
                       to: dir.appendingPathComponent("\(name).png"))
        }
        // Five sources: the first three and "2 more", then the list open.
        let five = peak.sources + [TokenSource(id: "t4", project: "cooked", detail: "Sync forks with upstream", account: "personal",
                                               rate: 3000, agents: nil),
                                   TokenSource(id: "t5", project: "trail-log", detail: "Plan the week", account: "side", rate: 1200, agents: nil)]
        for (name, open) in [("sources-more", false), ("sources-more-open", true)] {
            try render(SourcesCard(sources: five, expanded: open).frame(width: 388).padding(24).background(desk),
                       to: dir.appendingPathComponent("\(name).png"))
        }
        // A long thread title in a T3 thread, plain and hovered: the hint takes only its own width.
        let long = [TokenSource(id: "l1", threadID: "t", project: "cooked-polish",
                                detail: "Fix five visual bugs in the popover and the island", rate: 52_000, agents: "1 agent")]
        for (name, hovered) in [("sources-long", nil), ("sources-long-hover", "l1")] as [(String, String?)] {
            try render(SourcesCard(sources: long, hovered: hovered).frame(width: 388).padding(24).background(desk),
                       to: dir.appendingPathComponent("\(name).png"))
        }
        // The peak card without an ETA: blocked, and urgent by % with no tokens.
        for (name, lead, time, detail) in [("blocked", "free at ", "20:20", "personal is cooked"),
                                           ("no-eta", "resets ", "20:20", "personal")] {
            var s = peak
            s.urgency = Urgency(percent: name == "blocked" ? 103 : 91, lead: lead, fullAt: time, detail: detail,
                                source: "Idle for 5 min")
            try render(SurfaceShot { IslandView(model: IslandModel(snapshot: s), namespace: $0) }.padding(24).background(desk),
                       to: dir.appendingPathComponent("island-peak-\(name).png"))
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
        try render(SourcesCard(sources: model.snapshot.sources, expanded: true).frame(width: 388).padding(24).background(desk),
                   to: dir.appendingPathComponent("live-sources-open.png"))
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
