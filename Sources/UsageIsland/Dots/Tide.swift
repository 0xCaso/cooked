import Foundation

// Tide: the charts are a liquid. Every column is a spring pulled to its target and tied to its neighbours,
// so a new value flows into place instead of being redrawn.

/// Runs a simulation in fixed 240 Hz steps, so the picture does not depend on the frame rate.
/// A stall counts as at most 1/15 s, so it does not explode.
struct FixedStepper {
    static let step = 1.0 / 240
    private var last: Double?
    private var pending = 0.0

    /// Steps run for the time between the previous call and `t`.
    mutating func advance(to t: Double, _ step: (Double) -> Void) {
        pending += min(1.0 / 15, max(0, t - (last ?? t)))
        last = t
        while pending >= Self.step {
            step(Self.step)
            pending -= Self.step
        }
    }

    /// Runs `seconds` of steps at once, so the first frame already shows a settled picture.
    static func warm(_ seconds: Double, _ step: (Double) -> Void) {
        for _ in 0..<Int(seconds / Self.step) { step(Self.step) }
    }
}

/// A row of columns: each one a spring with stiffness `k` to its target, `c` to its neighbours, damping `d`.
struct Water {
    let k: Double, c: Double, d: Double
    var height: [Double]
    var velocity: [Double]

    init(cols: Int, k: Double, c: Double, d: Double, height: Double = 0) {
        self.k = k
        self.c = c
        self.d = d
        self.height = Array(repeating: height, count: cols)
        velocity = Array(repeating: 0, count: cols)
    }

    mutating func step(_ dt: Double, target: [Double]) {
        let n = height.count
        for i in 0..<n {
            let l = height[max(0, i - 1)], r = height[min(n - 1, i + 1)]
            velocity[i] += (k * (target[i] - height[i]) + c * (l + r - 2 * height[i]) - d * velocity[i]) * dt
        }
        for i in 0..<n { height[i] += velocity[i] * dt }
    }
}

/// The square-root y scale: one huge minute no longer flattens the other 59.
enum TideScale {
    static func y(_ v: Double, top: Double) -> Double { top > 0 ? min(1, (max(0, v) / top).squareRoot()) : 0 }

    /// The row (from the top) the threshold sits on.
    static func thresholdRow(rows: Int, threshold: Double, top: Double) -> Int {
        rows - Int((y(threshold, top: top) * Double(rows)).rounded())
    }

    /// Minutes → column heights in rows. Each column shows the average of the minutes it covers, weighted by
    /// overlap, so a single minute is as wide on screen as it is in time. Never below 0.4 rows: the floor is wet.
    static func targets(_ minutes: [Double], cols: Int, rows: Int, top: Double) -> [Double] {
        let n = minutes.count
        guard n > 0 else { return Array(repeating: 0.4, count: cols) }
        let span = Double(n) / Double(cols)
        return (0..<cols).map { c in
            let a = Double(c) * span, b = a + span
            var sum = 0.0
            var m = Int(a)
            while Double(m) < b, m < n {
                sum += minutes[m] * (min(b, Double(m + 1)) - max(a, Double(m)))
                m += 1
            }
            return max(0.4, y(sum / span, top: top) * Double(rows))
        }
    }
}

/// How bright the liquid under the meniscus is: `base + gain·e^(−depth/scale)`.
struct LiquidBody {
    let base: Double, gain: Double, scale: Double
    /// The island's tank: 20–52% light.
    static let tank = LiquidBody(base: 0.2, gain: 0.32, scale: 4)
    /// The popover's chart, brighter: 34–70% light.
    static let chart = LiquidBody(base: 0.34, gain: 0.36, scale: 5)

    func light(depth: Double) -> Double { depth > 0 ? base + gain * exp(-depth / scale) : 0 }
}

/// A grid of dots, row 0 at the top, index `r * cols + c`. `level` 0...1 is brightness,
/// `fresh` 0...1 how white the dot's core is, `hot` coral, `lift` how far the dot sits above its grid place, in points.
struct LiquidField {
    let cols: Int
    let rows: Int
    var level: [Float]
    var fresh: [Float]
    var hot: [Bool]
    var lift: [Float]

    init(cols: Int, rows: Int) {
        self.cols = cols
        self.rows = rows
        level = Array(repeating: 0, count: cols * rows)
        fresh = Array(repeating: 0, count: cols * rows)
        hot = Array(repeating: false, count: cols * rows)
        lift = Array(repeating: 0, count: cols * rows)
    }

    /// The surface row is bright, the liquid below fades with depth; where the surface moves fast it catches
    /// more light. Dots above `hotRow` are coral when `urgent`; with `reaches`, only in the columns whose real value
    /// reaches that row, so a spring overshooting the threshold never flashes coral.
    mutating func pour(_ water: Water, hotRow: Int, urgent: Bool, still: Bool, reaches: [Bool]? = nil,
                       body liquid: LiquidBody = .tank) {
        for c in 0..<cols {
            let h = water.height[c]
            let speed = still ? 0 : min(0.35, abs(water.velocity[c]) * 0.05)
            for r in 0..<rows {
                let i = r * cols + c
                let depth = h - (Double(rows - 1 - r) + 0.5)
                // the meniscus: one row, its edge anti-aliased
                let top = clamp01(depth + 0.5) * (1 - Double(smoothstep(0.5, 1.6, Float(depth))))
                let body = liquid.light(depth: depth)
                level[i] = Float(min(1, max(top * (0.8 + speed), body)))
                fresh[i] = Float(top > 0.5 ? 0.6 + speed : 0.15)
                hot[i] = urgent && r < hotRow && reaches?[c] != false
            }
        }
    }
}

/// The island's tank: the level follows the token rate, and tokens fall in as drops that dent the surface.
final class TideIsland {
    static let cols = 36, rows = 9
    /// The threshold sits at 75% of the tank.
    static let thresholdShare = 0.75
    static let thresholdRow = rows - Int((surface(thresholdShare)).rounded())

    private var water = Water(cols: cols, k: 30, c: 900, d: 5, height: 2)
    private var stepper = FixedStepper()
    private var level = 0.0
    private var drip = 0.0
    private var seed: UInt64 = 3
    private var warmed = false
    private(set) var field = LiquidField(cols: cols, rows: rows)

    /// Height in rows of a 0...1 level: never empty, never over the brim.
    static func surface(_ level: Double) -> Double { 1.2 + clamp01(level) * (Double(rows) - 2.4) }

    /// Rate → 0...1 level: the threshold sits at 75% of the tank.
    static func level(rate: Double, threshold: Double) -> Double {
        threshold > 0 ? clamp01(rate / threshold * thresholdShare) : 0
    }

    func frame(at t: Double, level: Double, urgent: Bool, still: Bool) -> LiquidField {
        self.level = level
        if still {
            water.height = Array(repeating: Self.surface(level), count: Self.cols)
            water.velocity = Array(repeating: 0, count: Self.cols)
        } else if !warmed {
            water.height = Array(repeating: Self.surface(level), count: Self.cols)
            FixedStepper.warm(3, step)
            warmed = true
            stepper.advance(to: t) { _ in }
        } else {
            stepper.advance(to: t, step)
        }
        field.pour(water, hotRow: Self.thresholdRow, urgent: urgent, still: still)
        return field
    }

    private func step(_ dt: Double) {
        let target = Array(repeating: Self.surface(level), count: Self.cols)
        drip += dt * (2 + 22 * level)
        while drip > 1 {
            drip -= 1
            velocity(at: Int(random() * Double(Self.cols)), add: -(14 + 18 * level))
        }
        water.step(dt, target: target)
    }

    private func velocity(at c: Int, add dv: Double) { water.velocity[min(Self.cols - 1, c)] += dv }

    /// Park–Miller, like the mockup: the same drops on every run.
    private func random() -> Double {
        seed = (seed * 16807) % 2_147_483_647
        return Double(seed) / 2_147_483_647
    }
}

/// The popover's 60 minutes as a liquid: when a minute closes the targets jump, and the surface flows into its
/// new shape. Slow drops keep it alive while nothing changes, like the island: they fall on a separate ripple layer,
/// added to the data's heights only when drawn, so the data's columns stay where the minutes put them. The ripple
/// reaches into the body: the dots under a moving surface rise and sink with it, less with depth.
final class TideHistogram {
    static let cols = 84, rows = 24
    static let pitch = 348.0 / Double(cols)
    /// The ripple layer runs at this share of real time: 40% slower than the island.
    static let slow = 0.6
    /// Drops per second of ripple time, whatever the data.
    static let dropRate = 1.6
    /// The ripples never move a column further than this, in rows.
    static let maxRipple = 0.6
    /// A body dot never moves further than this from its grid place, in points.
    static let maxLift = 0.5
    /// Rows under the surface over which a body dot's movement fades by e.
    static let liftDepth = 6.0

    private var water = Water(cols: cols, k: 120, c: 110, d: 9)
    private var ripple = Water(cols: cols, k: 30, c: 900, d: 5)
    private var stepper = FixedStepper()
    private var target: [Double] = []
    private var primed = false
    private var drip = 0.0
    private var seed: UInt64 = 5
    private(set) var thresholdRow = 0
    private(set) var field = LiquidField(cols: cols, rows: rows)
    /// For tests: the column heights in rows as drawn, ripples included.
    private(set) var shownHeights: [Double] = Array(repeating: 0, count: cols)

    /// Coral above the threshold row in every column whose real value is over it, past minutes included: the
    /// chart shows when usage went over, not only whether it is over now. The island's tank keeps the `urgent` rule.
    func frame(_ history: MinuteHistory, at t: Double, still: Bool) -> LiquidField {
        target = TideScale.targets(history.values, cols: Self.cols, rows: Self.rows, top: history.top)
        thresholdRow = TideScale.thresholdRow(rows: Self.rows, threshold: history.threshold, top: history.top)
        if still || !primed {
            water.height = target
            water.velocity = Array(repeating: 0, count: Self.cols)
            ripple = Water(cols: Self.cols, k: ripple.k, c: ripple.c, d: ripple.d)
            primed = true
            stepper.advance(to: t) { _ in }
        } else {
            stepper.advance(to: t, step)
        }
        let line = Double(Self.rows - thresholdRow)
        var shown = water
        for c in 0..<Self.cols {
            let h = water.height[c]
            let r = min(Self.maxRipple, max(-Self.maxRipple, ripple.height[c]))
            // a ripple never lifts a column into the threshold row, and never below the floor
            shown.height[c] = max(0, r > 0 ? min(h + r, max(h, line - 1)) : h + r)
            shown.velocity[c] += ripple.velocity[c] * Self.slow * 0.1
        }
        shownHeights = shown.height
        field.pour(shown, hotRow: thresholdRow, urgent: true, still: still, reaches: target.map { $0 > line }, body: .chart)
        for c in 0..<Self.cols {
            let moved = shown.height[c] - water.height[c]
            for r in 0..<Self.rows {
                let depth = shown.height[c] - (Double(Self.rows - 1 - r) + 0.5)
                field.lift[r * Self.cols + c] = Float(Self.lift(moved: moved, depth: depth))
            }
        }
        return field
    }

    /// Points a dot `depth` rows under the surface rises (negative: sinks) when its column's surface moved by `moved`
    /// rows: the whole ripple's share of 0.5 pt, fading with depth; dry dots stay put.
    static func lift(moved: Double, depth: Double) -> Double {
        guard depth > 0 else { return 0 }
        return max(-1, min(1, moved / maxRipple)) * maxLift * exp(-depth / liftDepth)
    }

    private func step(_ dt: Double) {
        water.step(dt, target: target)
        let slowed = dt * Self.slow
        drip += slowed * Self.dropRate
        while drip > 1 {
            drip -= 1
            ripple.velocity[min(Self.cols - 1, Int(random() * Double(Self.cols)))] -= 8 + 6 * random()
        }
        ripple.step(slowed, target: Array(repeating: 0, count: Self.cols))
    }

    /// Park–Miller, like the island: the same drops on every run.
    private func random() -> Double {
        seed = (seed * 16807) % 2_147_483_647
        return Double(seed) / 2_147_483_647
    }

    /// For tests: the column heights in rows.
    var heights: [Double] { water.height }
}

/// How one liquid dot is drawn (the mockup's `paint`).
struct LiquidShade: Equatable {
    let alpha: Float
    let radiusScale: Float
    /// 0 the body colour, 1 the white core.
    let fresh: Float
    let glow: Float
    let hot: Bool

    /// `rimCols`: how many columns fade in from each side (0 for none). `floor`: the light of an unlit dot.
    static func shade(_ f: LiquidField, at i: Int, floor: Float, rimCols: Int) -> LiquidShade {
        let c = i % f.cols
        let v = min(1, max(0, f.level[i]))
        let rim: Float = rimCols > 0 ? smoothstep(0, 1, Float(min(c, f.cols - 1 - c) + 1) / Float(rimCols)) : 1
        return LiquidShade(alpha: (floor + v * (1 - floor)) * rim,
                           radiusScale: 0.72 + 0.28 * smoothstep(0.1, 0.7, v),
                           fresh: min(1, f.fresh[i]),
                           glow: smoothstep(0.55, 1, v) * rim * 0.8,
                           hot: f.hot[i] && v > 0.04)
    }
}

func clamp01(_ v: Double) -> Double { min(1, max(0, v)) }
