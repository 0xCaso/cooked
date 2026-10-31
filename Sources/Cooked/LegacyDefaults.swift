import Foundation

/// Cooked's bundle id was com.0xcaso.UsageIsland: the settings saved under it (glass, chart range, position) are
/// copied once, so the rename keeps them. A key Cooked already has is never overwritten.
enum LegacyDefaults {
    static let domain = "com.0xcaso.UsageIsland"
    static let keys = [GlassStyle.defaultsKey, ChartRange.defaultsKey, PositionStore.key]

    static func migrate(from old: UserDefaults? = UserDefaults(suiteName: domain), to new: UserDefaults = .standard) {
        guard let old else { return }
        for key in keys where new.object(forKey: key) == nil {
            if let value = old.object(forKey: key) { new.set(value, forKey: key) }
        }
    }
}
