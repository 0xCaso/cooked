import Testing
@testable import UsageIsland

struct WaveModelTests {
    @Test func centreRowAlwaysLit() {
        for t in stride(from: 0.0, to: 3.0, by: 0.37) {
            let g = WaveModel.grid(values: Array(repeating: 0, count: 18), threshold: 40000, t: t, urgent: false)
            #expect(g.cols == 18 && g.rows == 7)
            for c in 0..<18 {
                #expect(g.level[c * 7 + 3] > 0)
                #expect(g.level[c * 7 + 0] == 0)
                #expect(g.level[c * 7 + 6] == 0)
            }
        }
    }

    @Test func mirrored() {
        let values = DemoData.series(count: 20, peak: true)
        let g = WaveModel.grid(values: values, threshold: 40000, t: 1.3, urgent: true)
        for c in 0..<20 {
            for k in 1...3 { #expect(g.level[c * 7 + 3 - k] == g.level[c * 7 + 3 + k]) }
        }
    }

    @Test func redOnlyAboveThresholdWhenUrgent() {
        let values: [Double] = [10000, 90000]
        let urgent = WaveModel.grid(values: values, threshold: 40000, t: 0, urgent: true)
        #expect(!urgent.hot[0 * 7 + 3])
        #expect(urgent.hot[1 * 7 + 3])
        let calm = WaveModel.grid(values: values, threshold: 40000, t: 0, urgent: false)
        #expect(!calm.hot.contains(true))
    }
}
