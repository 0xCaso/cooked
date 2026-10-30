import Foundation

/// Builds what the island shows from the watcher's status.json and the per-minute tokens of T3 Code's database.
enum LiveSnapshot {
    /// Display order, then any other account in id order.
    static let order = ["claudeAgent_work", "claudeAgent_team", "claudeAgent", "claudeAgent_side", "codex"]
    static let shortNames = ["claudeAgent_work": "work", "claudeAgent_team": "team", "claudeAgent": "personal",
                             "claudeAgent_side": "side", "codex": "codex"]
    /// The watcher's fallback p99 for 15 minutes, when status.json has none.
    static let fallbackThreshold15m = 500_000
    /// The chart keeps a day, for its longest range (`ChartRange.day`).
    static let historyMinutes = 24 * 60
    /// Urgency rules.
    static let urgentPercent = 90
    static let urgentETA: TimeInterval = 20 * 60
    static let okStates: Set<String> = ["allowed", "allowed_warning", "?"]
    static let fiveHours: TimeInterval = 5 * 3600
    static let week: TimeInterval = 7 * 86400
    /// An account with no event for this long is hidden from the popover.
    static let unusedAfter: TimeInterval = 5 * 3600

    static func short(id: String, name: String) -> String {
        shortNames[id] ?? name.lowercased().replacingOccurrences(of: "claude-", with: "")
    }

    /// nil when no account has 5h data yet.
    static func make(status: StatusFile, minutes: TokenLog.Minutes, now: Date, calendar: Calendar = .current) -> UsageSnapshot? {
        let ids = status.accounts.keys
            .filter { status.accounts[$0]?.fiveHourPct != nil }
            .sorted { (order.firstIndex(of: $0) ?? 99, $0) < (order.firstIndex(of: $1) ?? 99, $1) }
        guard !ids.isEmpty else { return nil }
        let minuteNow = Int(now.timeIntervalSince1970) / 60

        func entry(_ id: String) -> StatusFile.Entry { status.accounts[id]! }
        func stale(_ id: String) -> Bool { entry(id).note != nil }
        func percent(_ id: String) -> Int { stale(id) ? 0 : entry(id).fiveHourPct ?? 0 }
        func threshold(_ id: String) -> Int { (entry(id).spikeThreshold15m ?? fallbackThreshold15m) / 15 }
        func series(_ id: String) -> [Double] {
            let m = minutes.byAccount[id] ?? [:]
            return (0..<historyMinutes).map { m[minuteNow - historyMinutes + 1 + $0] ?? 0 }
        }
        /// Tokens per minute over the last 5 minutes, from the same minutes as the chart.
        func rate(_ id: String) -> Double { series(id).suffix(5).reduce(0, +) / 5 }
        func recent(_ id: String) -> Int { entry(id).tokensFresh?["15m"] ?? 0 }
        func eta(_ id: String) -> Date? {
            entry(id).projection.flatMap { WatcherTime.parse($0.eta, now: now, calendar: calendar) }
        }

        func resetDate(_ id: String) -> Date? {
            entry(id).fiveHourResetsAt.flatMap { WatcherTime.parse($0, now: now, calendar: calendar) }
        }
        func blocked(_ id: String) -> Bool { !stale(id) && entry(id).limitStatus == "rejected" }
        /// Shown only when it lands before the reset: the watcher means it so, and a later time says nothing.
        func fullAt(_ id: String) -> String? {
            if stale(id) || blocked(id) { return nil }
            if percent(id) >= 100 { return "now" }
            guard let eta = eta(id) else { return nil }
            if let reset = resetDate(id), eta >= reset { return nil }
            return clock(eta, calendar)
        }
        func weekResetDate(_ id: String) -> Date? {
            entry(id).sevenDayResetsAt.flatMap { WatcherTime.parse($0, now: now, calendar: calendar) }
        }
        /// nil once the week has reset with no new data.
        func weekPercent(_ id: String) -> Int? {
            if let reset = weekResetDate(id), reset <= now { return nil }
            return entry(id).sevenDayPct
        }
        func weekReset(_ id: String) -> String? {
            guard weekPercent(id) != nil, let reset = weekResetDate(id) else { return nil }
            return weekday(reset, calendar)
        }
        /// No event in the last 5 hours; without a last event, not live and no tokens in the last 15 minutes.
        func unused(_ id: String) -> Bool {
            if blocked(id) { return false }
            if let last = entry(id).lastEventAt.flatMap({ WatcherTime.parse($0, now: now, calendar: calendar) }) {
                return now.timeIntervalSince(last) > unusedAfter
            }
            return entry(id).live != true && recent(id) == 0
        }

        var accounts = ids.map { id in
            Account(id: id, short: short(id: id, name: entry(id).name), fiveHourPercent: percent(id),
                    fiveHourReset: stale(id) ? "—" : clockPart(entry(id).fiveHourResetsAt) ?? "—",
                    isLive: entry(id).live ?? false, fullAt: fullAt(id), weekPercent: weekPercent(id),
                    isBlocked: blocked(id), isUnused: unused(id),
                    fiveHourElapsed: stale(id) ? nil : resetDate(id).map { elapsed(until: $0, window: fiveHours, now: now) },
                    weekElapsed: weekPercent(id) == nil ? nil : weekResetDate(id).map { elapsed(until: $0, window: week, now: now) },
                    weekReset: weekReset(id))
        }

        func isUrgent(_ id: String) -> Bool {
            let e = entry(id)
            guard !stale(id), e.live == true else { return false }
            if percent(id) >= urgentPercent { return true }
            if let eta = eta(id), eta.timeIntervalSince(now) <= urgentETA { return true }
            if let state = e.limitStatus, !okStates.contains(state) { return true }
            return series(id).suffix(2).contains { $0 > Double(threshold(id)) }
        }
        let urgentID = ids.filter(isUrgent).max { percent($0) < percent($1) }
        let liveIDs = ids.filter { entry($0).live == true }
        let activeID = urgentID ?? (liveIDs.isEmpty ? ids : liveIDs).max { recent($0) < recent($1) } ?? ids[0]
        // the active account always keeps its row
        if let i = accounts.firstIndex(where: { $0.id == activeID }) { accounts[i].isUnused = false }

        let urgency = urgentID.map { id -> Urgency in
            let name = self.short(id: id, name: entry(id).name)
            let reset = clockPart(entry(id).fiveHourResetsAt) ?? "—"
            // Without an ETA the headline says when the window frees up, never "100% at —". Short: the card cuts at ~17 characters.
            let (lead, time, when): (String, String, String?) =
                if blocked(id) { ("free at ", reset, "cooked") }
                else if let eta = eta(id) { ("100% at ", clock(eta, calendar), "in \(max(0, Int(eta.timeIntervalSince(now) / 60))) min") }
                else if percent(id) >= 100 { ("100% ", "now", "resets \(reset)") }
                else { ("resets ", reset, nil) }
            let rate = rate(id)
            let source = minutes.topThread[id].flatMap { $0.tokens / 5 >= 1 ? $0 : nil }
                .map { "\(TokenFormat.short($0.tokens / 5))/min from “\($0.title)”" }
                ?? (rate >= 1 ? "\(TokenFormat.short(rate))/min now" : "Idle for 5 min")
            return Urgency(
                percent: percent(id),
                lead: lead,
                fullAt: time,
                detail: when.map { "\(name) · \($0)" } ?? name,
                source: source)
        }

        return UsageSnapshot(
            clock: clock(now, calendar),
            accounts: accounts,
            activeID: activeID,
            rateNow: Int(rate(activeID)),
            threshold: threshold(activeID),
            series: series(activeID),
            urgency: urgency)
    }

    /// "30/10 19:00" → "19:00".
    static func clockPart(_ s: String?) -> String? {
        s.flatMap { $0.split(separator: " ").last.map(String.init) }
    }

    /// Even pace: how much of a window that ends at `reset` has gone by.
    static func elapsed(until reset: Date, window: TimeInterval, now: Date) -> Double {
        min(1, max(0, 1 - reset.timeIntervalSince(now) / window))
    }

    /// "Mon": English, as every other word in the island.
    static func weekday(_ d: Date, _ calendar: Calendar) -> String {
        let names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
        return names[(calendar.component(.weekday, from: d) - 1) % 7]
    }

    static func clock(_ d: Date, _ calendar: Calendar) -> String {
        let c = calendar.dateComponents([.hour, .minute], from: d)
        return String(format: "%02d:%02d", c.hour ?? 0, c.minute ?? 0)
    }
}
