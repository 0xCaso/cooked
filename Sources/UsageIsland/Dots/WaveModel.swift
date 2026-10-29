import Foundation

/// The island's equaliser: a wave mirrored around the centre row, one continuous height per column.
enum WaveModel {
    static let rows = 7

    static func grid(values: [Double], threshold: Double, t: Double, urgent: Bool) -> DotGrid {
        let top = max(values.max() ?? 0, threshold * 1.6)
        let heights = values.enumerated().map { c, v in
            let w = max(0, sin(t * 2.6 - Double(c) * 0.6)) // the slow wave that runs through the line
            return min(3, max(0, v / top * 3 + 0.95 * w * w * w))
        }
        return DotGrid(cols: values.count, rows: rows) { c, r in
            let h = heights[c], k = Double(abs(r - 3))
            let cover = clamp01(h - k + 1) // the centre row is always lit
            let level = cover * (0.55 + 0.45 * clamp01(1 - (h - k))) // the tip is brighter than the body
            return (Float(level), urgent && values[c] > threshold)
        }
    }
}

func clamp01(_ v: Double) -> Double { min(1, max(0, v)) }
