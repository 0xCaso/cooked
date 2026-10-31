import AppKit
import Foundation
import Testing
@testable import Cooked

@MainActor
struct SnapshotsTests {
    func pixelSize(_ url: URL) -> CGSize? {
        guard let rep = NSImageRep(contentsOf: url) else { return nil }
        return CGSize(width: rep.pixelsWide, height: rep.pixelsHigh)
    }

    @Test func writesChartPNGs() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("ui-snap-\(UUID().uuidString)")
        try Snapshots.write(to: dir)
        // chart + 8 pt padding on each side, at 2×
        let tank = CGSize(width: ((36 * 3.4 + 16) * 2 as CGFloat).rounded(.up), height: ((9 * 3.4 + 16) * 2 as CGFloat).rounded(.up))
        #expect(pixelSize(dir.appendingPathComponent("wave-calm.png")) == tank)
        #expect(pixelSize(dir.appendingPathComponent("wave-peak.png")) != nil)
        let histHeight: CGFloat = ((24 * 348 / 84 + 16) * 2 as CGFloat).rounded(.up)
        #expect(pixelSize(dir.appendingPathComponent("hist-calm.png")) == CGSize(width: (348 + 16) * 2, height: histHeight))
        #expect(pixelSize(dir.appendingPathComponent("hist-peak.png")) != nil)
        #expect(pixelSize(dir.appendingPathComponent("pills-peak.png")) != nil)
        // island and popover: surface + 24 pt desk on each side, at 2×
        #expect(pixelSize(dir.appendingPathComponent("island-calm.png"))?.height == CGFloat((44 + 48) * 2))
        #expect(pixelSize(dir.appendingPathComponent("island-peak.png"))?.width == CGFloat((404 + 48) * 2))
        #expect(pixelSize(dir.appendingPathComponent("popover-calm.png"))?.width == CGFloat((404 + 48) * 2))
        #expect(pixelSize(dir.appendingPathComponent("popover-peak.png")) != nil)
    }

    @Test func glowSpriteIsCached() {
        let a = GlowSprite.cgImage(radius: 1.7, pitch: 5, hot: false)
        let b = GlowSprite.cgImage(radius: 1.7, pitch: 5, hot: false)
        let red = GlowSprite.cgImage(radius: 1.7, pitch: 5, hot: true)
        #expect(a === b)
        #expect(a !== red)
        #expect(a.width == 30 && a.height == 30) // ceil(5 × 3) pt at 2×
    }
}
