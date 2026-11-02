import Foundation

/// The Claude percent, from the files the status line bridge writes (`StatusLineSetup` connects it): for each account, the JSON
/// Claude Code last gave its status line, in `~/Library/Application Support/Cooked/bridge/<account id>.json`.
/// Claude Code sends `rate_limits.five_hour` and `seven_day` (`used_percentage` 0...100, `resets_at` Unix seconds)
/// after each response, for subscribers only. The file's change time is the sample's time.
enum StatusLineBridge {
    static var defaultDir: String {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].path + "/Cooked/bridge"
    }

    private struct Input: Decodable {
        struct Limits: Decodable {
            struct Window: Decodable {
                let used_percentage: Double?
                let resets_at: Double?
            }
            let five_hour: Window?
            let seven_day: Window?
        }
        let rate_limits: Limits?
    }

    /// Account id → its newest sample, for the files with a 5 h percent.
    static func samples(dir: String = defaultDir) -> [String: LimitLog.Sample] {
        var out: [String: LimitLog.Sample] = [:]
        for name in (try? FileManager.default.contentsOfDirectory(atPath: dir)) ?? [] where name.hasSuffix(".json") {
            let path = dir + "/" + name
            guard let modified = (try? FileManager.default.attributesOfItem(atPath: path))?[.modificationDate] as? Date,
                  let data = FileManager.default.contents(atPath: path),
                  let sample = sample(data, at: modified) else { continue }
            out[String(name.dropLast(".json".count))] = sample
        }
        return out
    }

    static func sample(_ data: Data, at: Date) -> LimitLog.Sample? {
        guard let limits = (try? JSONDecoder().decode(Input.self, from: data))?.rate_limits,
              let five = limits.five_hour?.used_percentage else { return nil }
        return .init(at: at.timeIntervalSince1970, five: five / 100, fiveReset: limits.five_hour?.resets_at,
                     week: (limits.seven_day?.used_percentage ?? 0) / 100, weekReset: limits.seven_day?.resets_at,
                     status: five >= 100 ? "rejected" : "allowed")
    }
}
