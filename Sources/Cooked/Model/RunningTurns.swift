import Foundation

/// New tokens of the turns still running. T3 Code keeps only the last call's usage of a running turn, so this
/// adds it up between readings: the growth of the context since the previous reading counts every call in
/// between, and the last call's new input counts a cache miss. Shared by every reading of one source.
final class RunningTurns: @unchecked Sendable {
    struct Sample {
        let turn: String
        let account: String
        let input: Double
        let cached: Double
        let output: Double
        let at: Date
    }

    private struct Turn {
        var account: String
        var input: Double
        var at: Date
        var minutes: [Int: Double]
    }

    private let lock = NSLock()
    private var turns: [String: Turn] = [:]

    /// The running turns now in the database. Turns that are gone have finished: their total replaces them.
    /// Returns account → minute → new tokens.
    func record(_ samples: [Sample]) -> [String: [Int: Double]] {
        lock.lock()
        defer { lock.unlock() }
        let live = Set(samples.map(\.turn))
        turns = turns.filter { live.contains($0.key) }
        for s in samples {
            let minute = Int(s.at.timeIntervalSince1970) / 60
            let call = max(0, s.input - s.cached) + s.output
            if var t = turns[s.turn] {
                guard s.at > t.at else { continue }
                t.minutes[minute, default: 0] += max(s.input - t.input, call - s.output) + s.output
                t.input = s.input
                t.at = s.at
                turns[s.turn] = t
            } else {
                turns[s.turn] = Turn(account: s.account, input: s.input, at: s.at, minutes: call > 0 ? [minute: call] : [:])
            }
        }
        var out: [String: [Int: Double]] = [:]
        for t in turns.values {
            for (m, v) in t.minutes { out[t.account, default: [:]][m, default: 0] += v }
        }
        return out
    }
}
