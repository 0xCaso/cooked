import Foundation

/// The critically damped spring the mockup samples for its curves: 0 → ~0.98 over p = 0...1.
func springProgress(_ p: Double) -> Double { 1 - (1 + 6 * p) * exp(-6 * p) }

/// Tokens per minute over the last 60 minutes as an 18-row dot histogram.
/// The minute in progress grows as tokens arrive; when it closes, every column eases into its neighbour's value
/// on a 0.5 s spring. Reference type: it carries the shift between frames.
final class HistogramClock: HistogramSource {
    static let rows = 18

    private let minuteLength: Double
    private let shift: Double
    private let next: () -> Double
    private let top: Double
    private var values: [Double]
    private var previous: [Double]
    private var target: Double
    private var minute: Int?

    /// First row (from the top) above the p99 threshold.
    let thresholdRow: Int

    init(values: [Double], threshold: Double, minuteLength: Double, shift: Double = 0.5, next: @escaping () -> Double) {
        self.values = values
        self.minuteLength = minuteLength
        self.shift = shift
        self.next = next
        previous = values
        target = values.last ?? 0
        top = max(values.max() ?? 0, threshold * 1.6)
        thresholdRow = Self.rows - Int((threshold / top * Double(Self.rows)).rounded())
    }

    /// Column heights in rows (1...18) at time `t` in seconds. `still`: no growth and no easing (Reduce Motion).
    func heights(at t: Double, still: Bool) -> [Double] {
        let m = Int((t / minuteLength).rounded(.down))
        if let minute, m != minute {
            values[values.count - 1] = target
            previous = values
            values = Array(values.dropFirst()) + [0]
            target = next()
        }
        minute = m
        let into = t.truncatingRemainder(dividingBy: minuteLength)
        let phase = into / minuteLength
        let k = still ? 1 : springProgress(clamp01(into / shift))
        let rows = Double(Self.rows)
        return values.indices.map { i in
            let current = i == values.count - 1 ? target * (still ? 1 : phase) : values[i]
            let v = previous[i] + (current - previous[i]) * k
            return min(rows, max(1, v / top * rows))
        }
    }

    func grid(at t: Double, still: Bool, urgent: Bool) -> DotGrid {
        Self.grid(heights: heights(at: t, still: still), thresholdRow: thresholdRow, urgent: urgent)
    }

    /// Column heights in rows → dots: the top dot of a column lights in proportion, the tip brighter than the body.
    static func grid(heights h: [Double], thresholdRow threshold: Int, urgent: Bool) -> DotGrid {
        let rows = Self.rows
        return DotGrid(cols: h.count, rows: rows) { c, r in
            let fromBottom = Double(rows - 1 - r)
            let cover = clamp01(h[c] - fromBottom)
            let below = h[c] - fromBottom - 1 // distance below the top of the column
            let level = below < 1 ? 1 + (0.7 - 1) * clamp01(below) : 0.7 + (0.42 - 0.7) * clamp01(below - 1)
            return (Float(cover * level), urgent && r < threshold)
        }
    }
}

/// What the popover histogram draws from: the demo clock or the live data.
protocol HistogramSource: AnyObject {
    var thresholdRow: Int { get }
    func grid(at t: Double, still: Bool, urgent: Bool) -> DotGrid
}

/// The real last hour. Minutes follow the wall clock: when one closes, the columns shift left on the same
/// 0.5 s spring as the demo, and a new empty column opens. New data eases in from what is on screen.
final class LiveHistogram: HistogramSource {
    private let epoch: Double
    private let shift: Double
    private var values: [Double]
    private var previous: [Double]
    private var top: Double
    private var previousTop: Double
    private var threshold: Double
    /// Wall-clock minute (Unix seconds / 60) of the last column.
    private var minute: Int
    private var easeStart = -Double.infinity

    /// `epoch`: the chart clock's time zero, Unix seconds. `minute`: the wall-clock minute of `values.last`.
    init(values: [Double], threshold: Double, minute: Int, epoch: Double, shift: Double = 0.5) {
        self.values = values
        previous = values
        self.threshold = threshold
        self.minute = minute
        self.epoch = epoch
        self.shift = shift
        top = Self.top(values, threshold)
        previousTop = top
    }

    var thresholdRow: Int { HistogramClock.rows - Int((threshold / top * Double(HistogramClock.rows)).rounded()) }

    /// New data from the database. `t` is the chart time it arrived at.
    func update(values new: [Double], threshold: Double, minute: Int, at t: Double) {
        let wall = epoch + t
        previous = displayed(wall)
        previousTop = displayedTop(wall)
        values = new
        self.minute = minute
        self.threshold = threshold
        top = Self.top(new, threshold)
        easeStart = wall
    }

    func heights(at t: Double, still: Bool) -> [Double] {
        let rows = Double(HistogramClock.rows)
        let wall = epoch + t
        if !still {
            let m = Int((wall / 60).rounded(.down))
            if m > minute {
                let d = min(values.count, m - minute)
                previous = displayed(wall)
                previousTop = displayedTop(wall)
                values = Array(values.dropFirst(d)) + Array(repeating: 0, count: d)
                minute = m
                easeStart = wall
            }
        }
        let v = still ? values : displayed(wall)
        let scale = still ? top : displayedTop(wall)
        return v.map { min(rows, max(1, $0 / scale * rows)) }
    }

    func grid(at t: Double, still: Bool, urgent: Bool) -> DotGrid {
        HistogramClock.grid(heights: heights(at: t, still: still), thresholdRow: thresholdRow, urgent: urgent)
    }

    /// Exactly 1 once the spring time is over: live columns stay put between minutes, so no residue may remain.
    private func progress(_ wall: Double) -> Double {
        let p = (wall - easeStart) / shift
        return p >= 1 ? 1 : springProgress(max(0, p))
    }

    private func displayed(_ wall: Double) -> [Double] {
        let k = progress(wall)
        return values.indices.map { previous[$0] + (values[$0] - previous[$0]) * k }
    }

    private func displayedTop(_ wall: Double) -> Double { previousTop + (top - previousTop) * progress(wall) }

    private static func top(_ v: [Double], _ threshold: Double) -> Double { max(v.max() ?? 0, threshold * 1.6, 1) }
}
