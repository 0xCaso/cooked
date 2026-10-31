import SwiftUI

extension EnvironmentValues {
    /// Time zero of every chart. Set once by the root; it never changes, so it re-renders nothing.
    @Entry var chartEpoch: Date = .distantPast
    /// Reduce Motion (or --reduce-motion), and snapshots: no drops, no growth, no easing.
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

/// Draws a `LiquidField`: every dot in the body or core colour, the pre-drawn glow sprite over the bright ones.
struct LiquidMatrix: View {
    let field: LiquidField
    let pitch: CGFloat
    let radius: CGFloat
    var rimCols = 0
    var thresholdRow: Int?

    // The mockup's hex palette: a warm pearl body, a whiter core where the surface is; coral over the threshold.
    static let tones = 8
    static let pearl = ramp(RGB(r: 219 / 255, g: 212 / 255, b: 196 / 255), RGB(r: 250 / 255, g: 246 / 255, b: 238 / 255))
    static let coral = ramp(Palette.redRGB, RGB(r: 1, g: 168 / 255, b: 153 / 255))

    private static func ramp(_ a: RGB, _ b: RGB) -> [Color] {
        (0...tones).map { i in
            let k = Double(i) / Double(tones)
            return RGB(r: a.r + (b.r - a.r) * k, g: a.g + (b.g - a.g) * k, b: a.b + (b.b - a.b) * k).color
        }
    }

    var body: some View {
        let ink = GlowSprite.image(radius: radius, pitch: pitch, hot: false)
        let red = GlowSprite.image(radius: radius, pitch: pitch, hot: true)
        let spriteSide = ceil(pitch * 3)
        Canvas { ctx, _ in
            let inkSprite = ctx.resolve(ink), redSprite = ctx.resolve(red)
            for i in 0..<field.level.count {
                let r = i / field.cols, c = i % field.cols
                // the threshold row: every other dot a little brighter
                let floor: Float = r == thresholdRow && c % 2 == 0 ? 0.22 : 0.06
                let s = LiquidShade.shade(field, at: i, floor: floor, rimCols: rimCols)
                guard s.alpha > 0.004 else { continue }
                let center = CGPoint(x: (CGFloat(c) + 0.5) * pitch, y: (CGFloat(r) + 0.5) * pitch - CGFloat(field.lift[i]))
                let rr = radius * CGFloat(s.radiusScale)
                let tone = Int((s.fresh * Float(Self.tones)).rounded())
                var dot = ctx
                dot.opacity = Double(s.alpha)
                dot.fill(Path(ellipseIn: CGRect(x: center.x - rr, y: center.y - rr, width: 2 * rr, height: 2 * rr)),
                         with: .color((s.hot ? Self.coral : Self.pearl)[tone]))
                if s.glow > 0.01 {
                    var layer = ctx
                    layer.opacity = Double(s.glow)
                    layer.draw(s.hot ? redSprite : inkSprite,
                               in: CGRect(x: center.x - spriteSide / 2, y: center.y - spriteSide / 2, width: spriteSide, height: spriteSide))
                }
            }
        }
        // rounded to 1/100 pt, so 84 × (348 / 84) is 348 and not a hair more
        .frame(width: (CGFloat(field.cols) * pitch * 100).rounded() / 100,
               height: (CGFloat(field.rows) * pitch * 100).rounded() / 100)
    }
}

/// The island's tank: 36 × 9 dots, pitch 3.4, radius 1.15. The level follows the token rate; drops fall in.
struct TideIslandChart: View {
    let rate: Double
    let threshold: Double
    let urgent: Bool
    @State private var tank = TideIsland()

    var body: some View {
        let level = TideIsland.level(rate: rate, threshold: threshold)
        ChartTimeline { t, still in
            LiquidMatrix(field: tank.frame(at: t, level: level, urgent: urgent, still: still),
                         pitch: 3.4, radius: 1.15, rimCols: 3, thresholdRow: TideIsland.thresholdRow)
        }
    }
}

/// The popover's chart over `range`: 84 × 24 dots across 348 pt, on a square-root scale. Every minute over the threshold is coral
/// above the threshold row, whether or not the rate is over it now.
struct TideHistogramChart: View {
    let source: any HistogramSource
    var range = ChartRange.hour
    @State private var tide = TideHistogram()

    var body: some View {
        ChartTimeline { t, still in
            let field = tide.frame(source.history(at: t, still: still).window(range.minutes), at: t, still: still,
                                   motion: range == .tenMinutes ? .lively : .calm)
            LiquidMatrix(field: field, pitch: CGFloat(TideHistogram.pitch), radius: 1.2, thresholdRow: tide.thresholdRow)
        }
    }
}
