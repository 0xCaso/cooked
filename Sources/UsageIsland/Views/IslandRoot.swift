import AppKit
import SwiftUI

/// Everything the panel shows: the island at the top centre and, when open, the popover 8 pt below.
/// One glass container, so the two shapes can merge, and one chart clock for every chart.
struct IslandRoot: View {
    let model: IslandModel
    var forceStill = false
    @Namespace private var glass
    @Environment(\.self) private var env

    var body: some View {
        let reduce = env.reducesMotion
        Group {
            GlassEffectContainer(spacing: IslandLayout.gap) {
                VStack(spacing: IslandLayout.gap) {
                    IslandView(model: model, namespace: glass)
                        .contentShape(.rect)
                        .modifier(PressableIsland(model: model) { model.isOpen.toggle() })
                        .contextMenu {
                            Button(model.snapshot.isUrgent ? "Demo: calm" : "Demo: peak") { model.toggleDemo() }
                            Picker("Glass", selection: Bindable(model).glassStyle) {
                                ForEach(GlassStyle.allCases, id: \.self) { Text($0.title).tag($0) }
                            }
                            .pickerStyle(.menu)
                            Divider()
                            Button("Quit Usage Island") { NSApp.terminate(nil) }
                        }
                        .reportsHitRect(into: model, slot: 0)
                    if model.isOpen {
                        PopoverView(model: model, histogram: model.histogram, namespace: glass)
                            // grows out of the island: from 0.96 and transparent, anchored at the top centre
                            .transition(.scale(scale: Motion.Scale.popoverFrom, anchor: .top).combined(with: .opacity))
                            .glassEffectTransition(.matchedGeometry)
                            .reportsHitRect(into: model, slot: 1)
                    }
                }
            }
            .animation(Motion.animation(model.isOpen ? Motion.popoverIn : Motion.popoverOut, reduceMotion: reduce), value: model.isOpen)
            .animation(Motion.animation(Motion.island, reduceMotion: reduce), value: model.snapshot.isUrgent)
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
    }

    nonisolated static let space = "panel"
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
