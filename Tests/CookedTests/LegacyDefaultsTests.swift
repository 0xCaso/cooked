import Foundation
import Testing
@testable import Cooked

struct LegacyDefaultsTests {
    @Test func copiesTheOldSettingsWithoutOverwritingNewOnes() throws {
        let old = try #require(UserDefaults(suiteName: "cooked-legacy-old-\(UUID().uuidString)"))
        let new = try #require(UserDefaults(suiteName: "cooked-legacy-new-\(UUID().uuidString)"))
        old.set("clear", forKey: GlassStyle.defaultsKey)
        old.set("1h", forKey: ChartRange.defaultsKey)
        old.set(Data([1, 2]), forKey: PositionStore.key)
        old.set("x", forKey: "unrelated")
        new.set("darker", forKey: GlassStyle.defaultsKey)

        LegacyDefaults.migrate(from: old, to: new)

        #expect(new.string(forKey: GlassStyle.defaultsKey) == "darker")
        #expect(new.string(forKey: ChartRange.defaultsKey) == "1h")
        #expect(new.data(forKey: PositionStore.key) == Data([1, 2]))
        #expect(new.object(forKey: "unrelated") == nil)
    }
}
