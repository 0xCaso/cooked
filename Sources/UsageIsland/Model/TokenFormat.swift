import Foundation

enum TokenFormat {
    /// 950 → "950", 9 000 → "9k", 1 200 000 → "1.2M".
    static func short(_ n: Double) -> String {
        if n >= 1_000_000 { return String(format: "%.1f", n / 1_000_000) + "M" }
        if n >= 1000 { return "\(Int((n / 1000).rounded()))k" }
        return "\(Int(n.rounded()))"
    }
}
