import SwiftUI

extension EnvironmentValues {
    /// Time zero of every chart. Set once by the root; it never changes, so it re-renders nothing.
    @Entry var chartEpoch: Date = .distantPast
    /// Reduce Motion (or --reduce-motion), and snapshots: no wave, no growth, no easing.
    @Entry var chartsStill: Bool = false
}

/// A chart's own display-synced clock (60 fps), paused when the charts are still.
/// Each chart keeps its clock inside itself, so per-frame time never flows through the rest of the view:
/// a time in the environment made SwiftUI rebuild the open context menu on every frame.
struct ChartTimeline<Content: View>: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.chartEpoch) private var epoch
    @Environment(\.chartsStill) private var forcedStill
    @ViewBuilder var content: (_ t: Double, _ still: Bool) -> Content

    var body: some View {
        let still = reduceMotion || forcedStill
        TimelineView(.animation(minimumInterval: nil, paused: still)) { context in
            content(still ? 0 : context.date.timeIntervalSince(epoch), still)
        }
    }
}

/// Draws a `DotGrid`: a plain dot for the dim part of each dot's light, the pre-drawn glow sprite for the bright part.
struct DotMatrix: View {
    let grid: DotGrid
    let pitch: CGFloat
    let radius: CGFloat
    var floor: (Int) -> Float = { _ in 0.07 }
    var vignette = false

    var body: some View {
        let ink = GlowSprite.image(radius: radius, pitch: pitch, hot: false)
        let red = GlowSprite.image(radius: radius, pitch: pitch, hot: true)
        let spriteSide = ceil(pitch * 3)
        Canvas { ctx, _ in
            let shades = DotShading.shade(grid, floor: floor, vignette: vignette)
            let inkSprite = ctx.resolve(ink), redSprite = ctx.resolve(red)
            for c in 0..<grid.cols {
                for r in 0..<grid.rows {
                    let s = shades[c * grid.rows + r]
                    let center = CGPoint(x: (CGFloat(c) + 0.5) * pitch, y: (CGFloat(r) + 0.5) * pitch)
                    let plain = Double(s.alpha * (1 - s.glow))
                    if plain > 0.001 {
                        let rr = radius * CGFloat(s.radiusScale)
                        let dot = Path(ellipseIn: CGRect(x: center.x - rr, y: center.y - rr, width: 2 * rr, height: 2 * rr))
                        ctx.fill(dot, with: .color((s.hot ? Palette.red : Palette.ink).opacity(plain)))
                    }
                    if s.glow > 0 {
                        var layer = ctx
                        layer.opacity = Double(s.alpha * s.glow)
                        layer.draw(s.hot ? redSprite : inkSprite,
                                   in: CGRect(x: center.x - spriteSide / 2, y: center.y - spriteSide / 2, width: spriteSide, height: spriteSide))
                    }
                }
            }
        }
        .frame(width: CGFloat(grid.cols) * pitch, height: CGFloat(grid.rows) * pitch)
    }
}

/// The island's mirrored wave: 7 rows, pitch 5, radius 1.7, dots fading toward the rim.
struct WaveChart: View {
    let values: [Double]
    let threshold: Double
    let urgent: Bool

    var body: some View {
        ChartTimeline { t, _ in
            DotMatrix(grid: WaveModel.grid(values: values, threshold: threshold, t: t, urgent: urgent),
                      pitch: 5, radius: 1.7, vignette: true)
        }
    }
}

/// The popover's 60 × 18 histogram; the p99 row is a little brighter.
struct HistogramChart: View {
    let clock: HistogramClock
    let urgent: Bool

    var body: some View {
        let row = clock.thresholdRow
        ChartTimeline { t, still in
            DotMatrix(grid: clock.grid(at: t, still: still, urgent: urgent),
                      pitch: 348 / 60, radius: 1.95, floor: { $0 == row ? 0.2 : 0.07 })
        }
    }
}

/// One account's 5h bar: 20 dots, pitch 7, radius 1.9.
struct AccountBarChart: View {
    let percent: Int
    let hot: Bool

    var body: some View {
        DotMatrix(grid: AccountBar.grid(percent: percent, hot: hot), pitch: 7, radius: 1.9, floor: { _ in 0.1 })
    }
}
