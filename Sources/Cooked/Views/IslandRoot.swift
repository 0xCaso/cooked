import AppKit
import SwiftUI

/// Everything the panel shows: the island at the top centre and, when open, the popover 8 pt below.
/// Each surface has its own glass: in one container the island's glass merged with the popover's, went darker
/// while open and vanished for ~240 ms when the popover closed. One chart clock for every chart.
struct IslandRoot: View {
    let model: IslandModel
    var forceStill = false
    /// Makes the reader read every 5 s while the feed is live, again when the accounts change. nil in snapshots.
    var makeSource: (@MainActor () -> UsageSource)? = nil
    @Namespace private var glass
    @Environment(\.self) private var env

    var body: some View {
        let reduce = env.reducesMotion
        Group {
            VStack(spacing: IslandLayout.gap) {
                if model.displayMode == .island {
                    IslandView(model: model, namespace: glass)
                        .contentShape(.rect)
                        .modifier(PressableIsland(model: model) { model.isOpen.toggle() })
                        .contextMenu {
                            Picker("Data", selection: Binding(get: { model.feed }, set: { model.select($0) })) {
                                ForEach(IslandModel.Feed.allCases, id: \.self) { Text($0.title).tag($0) }
                            }
                            .pickerStyle(.inline)
                            Divider()
                            Picker("Glass", selection: Bindable(model).glassStyle) {
                                ForEach(GlassStyle.allCases, id: \.self) { Text($0.title).tag($0) }
                            }
                            .pickerStyle(.menu)
                            Picker("Show in", selection: Bindable(model).displayMode) {
                                ForEach(DisplayMode.allCases, id: \.self) { Text($0.title).tag($0) }
                            }
                            .pickerStyle(.menu)
                            Divider()
                            Button("Settings…") { model.openSettings(nil) }
                            Button("Quit Cooked") { NSApp.terminate(nil) }
                        }
                        .reportsHitRect(into: model, slot: 0)
                }
                    if model.isOpen {
                        PopoverView(model: model, histogram: model.histogram, namespace: glass)
                            // grows out of the island: from 0.96 and transparent, anchored at the top centre
                            .transition(.scale(scale: Motion.Scale.popoverFrom, anchor: .top).combined(with: .opacity))
                            .reportsHitRect(into: model, slot: 1)
                    }
            }
            .animation(Motion.animation(model.isOpen ? Motion.popoverIn : Motion.popoverOut, reduceMotion: reduce), value: model.isOpen)
            .padding(IslandLayout.margin)
            .frame(width: IslandLayout.popoverWidth + 2 * IslandLayout.margin)
            .coordinateSpace(.named(IslandRoot.space))
        }
        .environment(\.chartEpoch, model.chartEpoch)
        .environment(\.chartsStill, forceStill || Motion.forcedReduce)
        .environment(\.glassStyle, model.glassStyle)
        .preferredColorScheme(.dark)
        .task {
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(3))
                model.tickRate()
            }
        }
        .task(id: LiveKey(feed: model.feed, accounts: model.accountsVersion)) {
            guard model.feed == .live, let source = makeSource?() else { return }
            while !Task.isCancelled {
                let reading = await Task.detached(priority: .utility) { source.read() }.value
                model.apply(reading)
                try? await Task.sleep(for: .seconds(5))
            }
        }
    }

    nonisolated static let space = "panel"

    private struct LiveKey: Equatable {
        let feed: IslandModel.Feed
        let accounts: Int
    }
}

private extension View {
    /// Keeps the model's hit rects in step with this surface's frame in the panel.
    func reportsHitRect(into model: IslandModel, slot: Int) -> some View {
        onGeometryChange(for: CGRect.self) { $0.frame(in: .named(IslandRoot.space)) } action: { rect in
            var rects = model.hitRects
            while rects.count <= slot { rects.append(.zero) }
            rects[slot] = rect
            model.hitRects = rects
        }
        .onDisappear { if model.hitRects.indices.contains(slot) { model.hitRects[slot] = .zero } }
    }
}
