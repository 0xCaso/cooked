import Foundation

/// The last minutes in tokens per minute, oldest first, the minute in progress last.
/// `top` is the top of the scale, `threshold` the p99 threshold.
struct MinuteHistory: Equatable {
    var values: [Double]
    var top: Double
    var threshold: Double

    /// Never below 1.6 × the threshold, so a calm hour does not fill the chart.
    static func top(_ v: [Double], _ threshold: Double) -> Double { max(v.max() ?? 0, threshold * 1.6, 1) }

    /// The last `minutes` minutes, with empty minutes before the first one known, and the scale's top for them alone.
    func window(_ minutes: Int) -> MinuteHistory {
        let known = Array(values.suffix(minutes))
        let v = Array(repeating: 0, count: max(0, minutes - known.count)) + known
        return MinuteHistory(values: v, top: Self.top(v, threshold), threshold: threshold)
    }
}

/// How far back the popover's chart looks: its 84 columns re-bin the
/// range's minutes, each column the average tokens per minute of the minutes it covers.
enum ChartRange: String, CaseIterable, Identifiable {
    case tenMinutes = "10m", halfHour = "30m", hour = "1h", fourHours = "4h", day = "1d"
    static let defaultsKey = "chartRange"

    var id: String { rawValue }
    var minutes: Int {
        switch self {
        case .tenMinutes: 10
        case .halfHour: 30
        case .hour: 60
        case .fourHours: 4 * 60
        case .day: 24 * 60
        }
    }
}

/// What the popover histogram draws from: the demo clock or the live data.
protocol HistogramSource: AnyObject {
    /// The history at chart time `t`. `still`: no growth (Reduce Motion, snapshots).
    func history(at t: Double, still: Bool) -> MinuteHistory
}

/// The demo's minutes: the minute in progress grows as tokens arrive; when it closes, the history moves left
/// by one and `next()` gives the new minute's total. The chart's springs carry the motion.
final class HistogramClock: HistogramSource {
    private let minuteLength: Double
    private let next: () -> Double
    private let top: Double
    private let threshold: Double
    private var values: [Double]
    private var target: Double
    private var minute: Int?

    init(values: [Double], threshold: Double, minuteLength: Double, next: @escaping () -> Double) {
        self.values = values
        self.threshold = threshold
        self.minuteLength = minuteLength
        self.next = next
        target = values.last ?? 0
        top = MinuteHistory.top(values, threshold)
    }

    func history(at t: Double, still: Bool) -> MinuteHistory {
        let m = Int((t / minuteLength).rounded(.down))
        if let minute, m != minute, !values.isEmpty {
            values[values.count - 1] = target
            values = Array(values.dropFirst()) + [0]
            target = next()
        }
        minute = m
        var v = values
        if !v.isEmpty {
            let phase = t.truncatingRemainder(dividingBy: minuteLength) / minuteLength
            v[v.count - 1] = target * (still ? 1 : phase)
        }
        return MinuteHistory(values: v, top: top, threshold: threshold)
    }
}

/// The real last hour. Minutes follow the wall clock: when one closes, the history moves left and a new empty
/// minute opens, even between readings.
final class LiveHistogram: HistogramSource {
    private let epoch: Double
    private var values: [Double]
    private var threshold: Double
    /// Wall-clock minute (Unix seconds / 60) of the last value.
    private var minute: Int

    /// `epoch`: the chart clock's time zero, Unix seconds. `minute`: the wall-clock minute of `values.last`.
    init(values: [Double], threshold: Double, minute: Int, epoch: Double) {
        self.values = values
        self.threshold = threshold
        self.minute = minute
        self.epoch = epoch
    }

    /// New data from the database.
    func update(values new: [Double], threshold: Double, minute: Int) {
        values = new
        self.threshold = threshold
        self.minute = minute
    }

    func history(at t: Double, still: Bool) -> MinuteHistory {
        let m = Int(((epoch + t) / 60).rounded(.down))
        if !still, m > minute {
            let d = min(values.count, m - minute)
            values = Array(values.dropFirst(d)) + Array(repeating: 0, count: d)
            minute = m
        }
        return MinuteHistory(values: values, top: MinuteHistory.top(values, threshold), threshold: threshold)
    }
}
