import Foundation

/// `~/.t3/usage-watch/status.json`, written by `usage-watch.py` every 30 s. Only the fields the island uses.
struct StatusFile: Decodable, Equatable {
    var updatedAt: String
    var accounts: [String: Entry]

    enum CodingKeys: String, CodingKey {
        case updatedAt = "updated_at", accounts
    }

    struct Entry: Decodable, Equatable {
        var name: String
        var tokensFresh: [String: Int]?
        var spikeThreshold15m: Int?
        var fiveHourPct: Int?
        /// "HH:MM" today, or "dd/mm HH:MM".
        var fiveHourResetsAt: String?
        var sevenDayPct: Int?
        var sevenDayResetsAt: String?
        /// "allowed", "allowed_warning", or a state that means the limit is hit.
        var limitStatus: String?
        /// "HH:MM" today, or "dd/mm HH:MM": the account's last event.
        var lastEventAt: String?
        var live: Bool?
        var projection: Projection?
        /// Set when the 5h window has already reset and no new data came in.
        var note: String?

        enum CodingKeys: String, CodingKey {
            case name, live, projection, note
            case tokensFresh = "tokens_fresh", spikeThreshold15m = "spike_threshold_15m"
            case fiveHourPct = "five_hour_pct", fiveHourResetsAt = "five_hour_resets_at"
            case sevenDayPct = "seven_day_pct", sevenDayResetsAt = "seven_day_resets_at"
            case limitStatus = "limit_status", lastEventAt = "last_event_at"
        }
    }

    struct Projection: Decodable, Equatable {
        var window: String
        var eta: String
    }

    static func decode(_ data: Data) throws -> StatusFile {
        // Explicit keys, not .convertFromSnakeCase: that would also rename account ids like "claudeAgent_work".
        try JSONDecoder().decode(StatusFile.self, from: data)
    }
}

/// The watcher's times: "HH:MM" (today) or "dd/mm HH:MM", local time.
enum WatcherTime {
    static func parse(_ s: String, now: Date, calendar: Calendar = .current) -> Date? {
        let parts = s.split(separator: " ")
        guard let hm = parts.last.map(String.init), let (h, m) = pair(hm, ":") else { return nil }
        var day = calendar.dateComponents([.year, .month, .day], from: now)
        if parts.count == 2 {
            guard let (d, mo) = pair(String(parts[0]), "/") else { return nil }
            day.day = d
            day.month = mo
        }
        day.hour = h
        day.minute = m
        guard var date = calendar.date(from: day) else { return nil }
        // "02/01 10:00" read on 30/12 is next year's January.
        if parts.count == 2, date < now.addingTimeInterval(-180 * 86400) {
            date = calendar.date(byAdding: .year, value: 1, to: date) ?? date
        }
        return date
    }

    private static func pair(_ s: String, _ sep: Character) -> (Int, Int)? {
        let p = s.split(separator: sep).compactMap { Int($0) }
        return p.count == 2 ? (p[0], p[1]) : nil
    }
}
