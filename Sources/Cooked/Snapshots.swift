import AppKit
import SwiftUI

/// `Cooked --snapshots <dir>`: renders parts of the island to PNG without a screen capture.
/// Charts are drawn still (t = 0). Glass cannot render off screen, so surfaces are drawn opaque.
@MainActor
enum Snapshots {
    static func write(to dir: URL) throws {
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try writeSetup(to: dir)
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
            // The week pill on its reset day shows a time, not a day: the % stays whole, the word "resets" gives way.
            ("pills-week-today", AnyView(VStack(spacing: 8) {
                ForEach([(2, "15:00"), (23, "09:30"), (100, "23:59"), (64, "Mon")], id: \.0) { left, reset in
                    UsagePill(left: left, timeLeft: 0.1, hot: false, live: false) { PillTimes(reset: reset) }
                        .frame(width: PopoverView.weekColumn)
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
            model.open()
            try render(IslandRoot(model: model, forceStill: true).background(desk),
                       to: dir.appendingPathComponent("open-\(name).png"))
        }
        // Menu bar mode: the status item on a light and a dark menu bar.
        for (name, snapshot) in [("calm", calm), ("peak", peak)] {
            let model = IslandModel(snapshot: snapshot)
            for (bar, scheme, fill) in [("light", ColorScheme.light, Color(white: 0.92)), ("dark", ColorScheme.dark, Color(white: 0.16))] {
                try render(MenuBarLabel(model: model).frame(height: 24).padding(.horizontal, 12).padding(.vertical, 6)
                               .background(fill).environment(\.colorScheme, scheme),
                           to: dir.appendingPathComponent("menubar-\(name)-\(bar).png"))
            }
        }
        try writeNotices(to: dir, desk: desk, clock: clock)
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
        // Ended threads under "Ended": with two live lines, the ended one hovered, and with every thread ended.
        let gym = TokenSource(id: "e1", threadID: "e1", project: "trail-app", detail: "Map tile cache fix", account: "team",
                              rate: 8100, agents: nil, endedAgo: 9)
        let regex = TokenSource(id: "e2", project: "t3 scratch", detail: "Regex for log lines", account: "personal",
                                rate: 2400, agents: nil, endedAgo: 6)
        let live = Array(peak.sources.prefix(2))
        for (name, sources, hovered) in [("sources-ended", live + [gym], nil), ("sources-ended-hover", live + [gym], "e1"),
                                         ("sources-all-ended", [regex, gym], nil)] as [(String, [TokenSource], String?)] {
            try render(SourcesCard(sources: sources, hovered: hovered).frame(width: 388).padding(24).background(desk),
                       to: dir.appendingPathComponent("\(name).png"))
        }
        // A long thread title in a T3 thread, plain and hovered: the hint takes only its own width.
        let long = [TokenSource(id: "l1", threadID: "t", project: "cooked-polish",
                                detail: "Fix five visual bugs in the popover and the island", rate: 52_000, agents: "1 agent")]
        for (name, hovered) in [("sources-long", nil), ("sources-long-hover", "l1")] as [(String, String?)] {
            try render(SourcesCard(sources: long, hovered: hovered).frame(width: 388).padding(24).background(desk),
                       to: dir.appendingPathComponent("\(name).png"))
        }
    }

    /// The five notices, island and open popover, on demo data.
    static func writeNotices(to dir: URL, desk: Color, clock: (UsageSnapshot) -> HistogramClock) throws {
        let defaults = UserDefaults(suiteName: "cooked-notice-shots")!
        defaults.removePersistentDomain(forName: "cooked-notice-shots")
        func without(_ ids: Set<String>) -> UsageSnapshot {
            var s = DemoData.calm
            // as LiveSnapshot makes them: no percent, so no reset and no week either
            s.accounts = s.accounts.map { a in
                ids.contains(a.id) ? Account(id: a.id, short: a.short, fiveHourPercent: 0, fiveHourReset: "—", isLive: a.isLive,
                                             isUnused: a.isUnused, hasPercent: false) : a
            }
            return s
        }
        let states: [(String, UsageSnapshot, Notice)] = [
            ("noaccounts", .empty(clock: "19:28"), .noAccounts),
            ("formatchanged", DemoData.calm, .formatChanged(account: "work", file: "~/.claude-work/projects/p/s.jsonl")),
            ("paused", without(["work", "team", "side"]), .paused),
            ("nopercent", without(["side"]), .noPercent(id: "side", account: "side")),
            ("apikey", without(["side"]), .apiKey(id: "side", account: "side")),
        ]
        for (name, snapshot, notice) in states {
            let model = IslandModel(snapshot: snapshot, defaults: defaults)
            model.show([notice])
            try render(SurfaceShot { IslandView(model: model, namespace: $0) }.padding(24).background(desk),
                       to: dir.appendingPathComponent("notice-island-\(name).png"))
            let histogram = clock(snapshot)
            try render(SurfaceShot { PopoverView(model: model, histogram: histogram, namespace: $0) }.padding(24).background(desk),
                       to: dir.appendingPathComponent("notice-popover-\(name).png"))
        }
    }

    /// Setup's three steps and Settings' states, on a made-up home with four Claude folders (three share one
    /// settings.json through links) and Codex.
    static func writeSetup(to dir: URL) throws {
        let files = FileManager.default
        let home = files.temporaryDirectory.appendingPathComponent("cooked-setup-shots-\(UUID().uuidString)").path
        defer { try? files.removeItem(atPath: home) }
        let now = Date()
        let folders: [(String, TimeInterval)] = [(".claude", -3 * 3600), (".claude-work", -4 * 86400), (".claude-team", -1800),
                                                 (".claude-side", -20 * 86400)]
        for (folder, ago) in folders {
            try files.createDirectory(atPath: home + "/\(folder)/projects/p", withIntermediateDirectories: true)
            try files.setAttributes([.modificationDate: now.addingTimeInterval(ago)], ofItemAtPath: home + "/\(folder)/projects/p")
        }
        try files.createDirectory(atPath: home + "/.codex/sessions/2026/11/04", withIntermediateDirectories: true)
        try #"{"statusLine":{"type":"command","command":"bash ~/.claude/statusline.sh"}}"#
            .write(toFile: home + "/.claude/settings.json", atomically: true, encoding: .utf8)
        for folder in [".claude-work", ".claude-team", ".claude-side"] {
            try files.createSymbolicLink(atPath: home + "/\(folder)/settings.json", withDestinationPath: home + "/.claude/settings.json")
        }
        let defaults = UserDefaults(suiteName: "cooked-setup-shots")!
        defaults.removePersistentDomain(forName: "cooked-setup-shots")
        let model = SetupModel(home: home, defaults: defaults, statusLine: StatusLineSetup(support: home + "/support"), login: ShotLogin())
        let names = ["claudeAgent": "personal", "claudeAgent_work": "work", "claudeAgent_team": "team", "claudeAgent_side": "side"]
        for i in model.rows.indices {
            if let name = names[model.rows[i].id] { model.rows[i].name = name }
            model.rows[i].ticked = model.rows[i].id != "claudeAgent_side"
        }

        let desk = Color(red: 27 / 255, green: 28 / 255, blue: 31 / 255)
        func shot(_ title: String, _ view: some View, _ name: String) throws {
            let window = VStack(spacing: 0) {
                Text(title).textStyle(.body).foregroundStyle(Palette.ink2).frame(maxWidth: .infinity).frame(height: 40)
                view
            }
            .background(Palette.bgDeep)
            .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
            .padding(24).background(desk)
            try render(window, to: dir.appendingPathComponent("\(name).png"))
        }
        for step in SetupModel.Step.allCases {
            model.step = step
            try shot("Set up Cooked", SetupView(model: model), "setup-\(step)")
        }
        model.step = .percent
        try shot("Set up Cooked", SetupView(model: model).environment(\.disclosuresOpen, true), "setup-percent-open")
        try shot("Cooked Settings", SettingsView(model: model, tab: .accounts), "settings-accounts")
        try shot("Cooked Settings", SettingsView(model: model, tab: .percent), "settings-percent-off")
        model.connect()
        try shot("Cooked Settings", SettingsView(model: model, tab: .percent), "settings-percent-connected")
        try shot("Cooked Settings", SettingsView(model: model, tab: .percent).environment(\.disclosuresOpen, true),
                 "settings-percent-connected-details")
        try shot("Cooked Settings", SettingsView(model: model, tab: .general, displayMode: .constant(.menuBar)), "settings-general")
        try shot("Cooked Settings", SettingsView(model: model, tab: .general, confirmingRemove: true), "settings-remove")
        try #"{"statusLine":{"type":"command","command":"npx claude-hud"}}"#
            .write(toFile: home + "/.claude/settings.json", atomically: true, encoding: .utf8)
        model.refresh()
        try shot("Cooked Settings", SettingsView(model: model, tab: .percent).environment(\.disclosuresOpen, true),
                 "settings-percent-changed")
    }

    private final class ShotLogin: LoginItem {
        var isEnabled = true
        func set(_ on: Bool) throws { isEnabled = on }
    }

    /// `--snapshots <dir> --live`: the island and the open popover from the real status.json and database.
    static func writeLive(to dir: URL) throws {
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let model = IslandModel(feed: .live)
        let now = Date()
        // the first launch fills 14 days of history over several readings
        let source = UsageSource()
        var reading = source.read(now: now)
        let deadline = Date().addingTimeInterval(120)
        while reading.history != nil, Date() < deadline { reading = source.read(now: Date()) }
        model.apply(reading)
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
