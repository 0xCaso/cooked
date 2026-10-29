import SwiftUI
import Testing
@testable import UsageIsland

struct MotionTests {
    @Test func reduceMotionDisablesAnimations() {
        #expect(Motion.animation(Motion.island, reduceMotion: true) == nil)
        #expect(Motion.animation(Motion.popoverIn, reduceMotion: true) == nil)
        #expect(Motion.animation(Motion.island, reduceMotion: false) == Motion.island)
    }

    @Test func durations() {
        // hex's springs: settle times from the mockup's look.html
        #expect(Motion.Duration.press == 0.08)
        #expect(Motion.Duration.release == 0.18)
        #expect(Motion.Duration.control == 0.2)
        #expect(Motion.Duration.popoverIn == 0.24)
        #expect(Motion.Duration.popoverOut == 0.2)
        #expect(Motion.Duration.island == 0.3)
        #expect(Motion.Scale.button == 0.97)
        #expect(Motion.Scale.island == 0.98)
        #expect(Motion.Scale.popoverFrom == 0.96)
    }

    @MainActor @Test func escapeAndOutsideClickClose() {
        let model = IslandModel()
        model.open()
        model.dismiss(.escape)
        #expect(!model.isOpen)
        model.open()
        model.dismiss(.clickOutside)
        #expect(!model.isOpen)
    }
}
