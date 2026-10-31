import Foundation
import Testing
@testable import Cooked

@MainActor
struct GlassStyleTests {
    func freshDefaults() throws -> UserDefaults {
        try #require(UserDefaults(suiteName: "cooked-glass-\(UUID().uuidString)"))
    }

    @Test func defaultIsDarker() throws {
        #expect(IslandModel(defaults: try freshDefaults()).glassStyle == .darker)
    }

    @Test func choiceIsKept() throws {
        let defaults = try freshDefaults()
        IslandModel(defaults: defaults).glassStyle = .clear
        #expect(IslandModel(defaults: defaults).glassStyle == .clear)
    }

    @Test func menuTitles() {
        #expect(GlassStyle.allCases.map(\.title) == ["System", "Clear", "Darker"])
    }

    @Test func onlyDarkerAddsTheMockupRim() {
        #expect(GlassStyle.allCases.filter(\.drawsRim) == [.darker])
    }
}
