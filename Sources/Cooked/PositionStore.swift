import Foundation

/// The panel's last position, in UserDefaults as JSON.
enum PositionStore {
    static let key = "islandPosition"

    static func load(from defaults: UserDefaults = .standard) -> SavedPosition? {
        defaults.data(forKey: key).flatMap { try? JSONDecoder().decode(SavedPosition.self, from: $0) }
    }

    static func save(_ position: SavedPosition, to defaults: UserDefaults = .standard) {
        if let data = try? JSONEncoder().encode(position) { defaults.set(data, forKey: key) }
    }
}
