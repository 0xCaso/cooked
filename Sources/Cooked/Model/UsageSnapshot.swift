/// What the island shows at one moment: live data from the watcher, or a demo.
struct Account: Identifiable, Equatable {
    let id: String
    let short: String
    let fiveHourPercent: Int
    let fiveHourReset: String
    let isLive: Bool
    /// When the watcher expects the 5h window to reach 100%, before it resets: "HH:MM", or "now" at 100% and over.
    /// nil when it is not expected to fill up before the reset, or when the account is blocked.
    var fullAt: String? = nil
    /// The week's %, nil when the week has already reset and no new data came in.
    var weekPercent: Int? = nil
    /// The limit is hit (`limit_status` "rejected"): blocked until `fiveHourReset`.
    var isBlocked = false
    /// No event in the last 5 hours: hidden from the popover.
    var isUnused = false
    /// Even pace: the share of the 5h window that has gone by, 0...1. nil when the reset is unknown.
    var fiveHourElapsed: Double? = nil
    /// The share of the week that has gone by, 0...1.
    var weekElapsed: Double? = nil
    /// The day the week resets: "Mon".
    var weekReset: String? = nil

    /// What is left of a window, counted like T3 Code's Usage page: 100 − used, within 0...100.
    static func left(_ used: Int) -> Int { 100 - min(max(used, 0), 100) }
    /// The 5 h window's % left; 0 when blocked.
    var fiveHourLeft: Int { isBlocked ? 0 : Self.left(fiveHourPercent) }
}

/// The spike line of the urgent island: when the 5h window fills up and what to do.
struct Urgency: Equatable {
    /// % used of the 5 h window.
    let percent: Int
    /// The words before the time in the headline: "full at ", "free at ", "resets ".
    var lead = "full at "
    let fullAt: String
    let detail: String
    let source: String

    /// % left, what the card shows.
    var left: Int { Account.left(percent) }
}

struct UsageSnapshot: Equatable {
    let clock: String
    var accounts: [Account]
    let activeID: String
    let rateNow: Int
    /// p99 threshold of the active account, tokens per minute: the island's.
    let threshold: Int
    /// The popover chart's tokens per minute, oldest first: live, every account summed; the demos, one account.
    let series: [Double]
    /// The popover's "now" and alert line, every account summed; nil in the demos, which show one account.
    var totalRate: Int?
    var totalThreshold: Int?
    /// The popover's alert line, tokens per minute.
    var chartThreshold: Int { totalThreshold ?? threshold }
    var urgency: Urgency?
    /// "What burns tokens": the top sources of the last 15 minutes, empty when idle.
    var sources: [TokenSource] = []

    var isUrgent: Bool { urgency != nil }
    var active: Account? { accounts.first { $0.id == activeID } }
    /// The accounts with a full row, in display order.
    var rows: [Account] { accounts.filter { !$0.isUnused } }
    /// The accounts not used in the last 5 hours, in display order.
    var unused: [Account] { accounts.filter(\.isUnused) }

    /// The top source of `account`, else the top source: the popover marks its rate in red in urgency.
    static func peakSource(_ sources: [TokenSource], account: String?) -> TokenSource? {
        sources.first { $0.account == account } ?? sources.first
    }
}
