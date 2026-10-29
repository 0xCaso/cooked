import Foundation

/// The critically damped spring the mockup samples for its curves: 0 → ~0.98 over p = 0...1.
func springProgress(_ p: Double) -> Double { 1 - (1 + 6 * p) * exp(-6 * p) }

/// Tokens per minute over the last 60 minutes as an 18-row dot histogram.
/// The minute in progress grows as tokens arrive; when it closes, every column eases into its neighbour's value
/// on a 0.5 s spring. Reference type: it carries the shift between frames.
final class HistogramClock {
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
        let h = heights(at: t, still: still)
        let rows = Self.rows, threshold = thresholdRow
        return DotGrid(cols: h.count, rows: rows) { c, r in
            let fromBottom = Double(rows - 1 - r)
            let cover = clamp01(h[c] - fromBottom)
            let below = h[c] - fromBottom - 1 // distance below the top of the column
            let level = below < 1 ? 1 + (0.7 - 1) * clamp01(below) : 0.7 + (0.42 - 0.7) * clamp01(below - 1)
            return (Float(cover * level), urgent && r < threshold)
        }
    }
}
