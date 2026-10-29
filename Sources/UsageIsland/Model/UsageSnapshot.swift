/// What the island shows at one moment. Demo values for now; the watcher's status.json comes next.
struct Account: Identifiable, Equatable {
    let id: String
    let short: String
    let fiveHourPercent: Int
    let fiveHourReset: String
    let isLive: Bool
}

/// The spike line of the urgent island: when the 5h window fills up and what to do.
struct Urgency: Equatable {
    let percent: Int
    let fullAt: String
    let detail: String
    let source: String
    let switchLabel: String
}

struct WeekLine: Equatable {
    let short: String
    let percent: Int
    let reset: String
}

struct UsageSnapshot: Equatable {
    let clock: String
    let historyStart: String
    let accounts: [Account]
    let activeID: String
    let rateNow: Int
    /// p99 threshold of the active account, tokens per minute.
    let threshold: Int
    /// Tokens per minute over the last 60 minutes, oldest first.
    let series: [Double]
    let week: WeekLine
    let urgency: Urgency?

    var isUrgent: Bool { urgency != nil }
    var active: Account? { accounts.first { $0.id == activeID } }
}
