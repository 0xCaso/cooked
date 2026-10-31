import Testing
@testable import Cooked

struct DotShadingTests {
    let floor: (Int) -> Float = { _ in 0.07 }

    func grid3(_ centre: Float, hot: Bool = false) -> DotGrid {
        DotGrid(cols: 3, rows: 3) { c, r in (c == 1 && r == 1) ? (centre, hot) : (0, false) }
    }

    func shade(_ g: DotGrid, _ c: Int, _ r: Int, vignette: Bool = false) -> DotShade {
        DotShading.shade(g, floor: floor, vignette: vignette)[c * g.rows + r]
    }

    @Test func unlitDotSitsAtFloor() {
        let s = shade(grid3(0), 1, 1)
        #expect(abs(s.alpha - 0.07) < 1e-5)
        #expect(s.glow == 0)
        #expect(abs(s.radiusScale - 0.8) < 1e-5)
    }

    @Test func litDotGlows() {
        let s = shade(grid3(1), 1, 1)
        #expect(abs(s.alpha - 0.98) < 1e-5)
        #expect(s.glow == 1)
        #expect(s.radiusScale == 1)
    }

    @Test func neighbourGetsHalo() {
        let s = shade(grid3(1), 0, 1)
        #expect(abs(s.alpha - (0.07 + 0.16 * 0.91)) < 1e-5)
        let none = shade(grid3(0.5), 0, 1)
        #expect(abs(none.alpha - 0.07) < 1e-5)
        let diagonal = shade(grid3(1), 0, 0)
        #expect(abs(diagonal.alpha - 0.07) < 1e-5)
    }

    @Test func hotSpreadsToHalo() {
        #expect(shade(grid3(1, hot: true), 1, 0).hot)
        #expect(!shade(grid3(1, hot: false), 1, 0).hot)
        #expect(!shade(grid3(0, hot: true), 1, 1).hot)
    }

    @Test func vignetteDimsRim() {
        let g = DotGrid(cols: 9, rows: 1) { _, _ in (1, false) }
        #expect(abs(shade(g, 0, 0, vignette: true).alpha - 0.98 * 0.25) < 1e-5)
        #expect(abs(shade(g, 4, 0, vignette: true).alpha - 0.98) < 1e-5)
        #expect(abs(shade(g, 0, 0).alpha - 0.98) < 1e-5)
    }
}
