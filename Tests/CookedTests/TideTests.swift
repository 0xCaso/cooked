import Foundation
import Testing
@testable import Cooked

struct TideTests {
    @Test func stepperRunsFixedStepsWhateverTheFrameRate() {
        var stepper = FixedStepper()
        var steps = 0
        stepper.advance(to: 0) { _ in steps += 1 }
        for frame in 1...60 { stepper.advance(to: Double(frame) / 60) { _ in steps += 1 } }
        #expect(abs(steps - 240) <= 1)
        var slow = FixedStepper(), slowSteps = 0
        slow.advance(to: 0) { _ in }
        for frame in 1...24 { slow.advance(to: Double(frame) / 24) { _ in slowSteps += 1 } }
        #expect(abs(slowSteps - 240) <= 1)
    }

    @Test func stepperCapsAStall() {
        var stepper = FixedStepper()
        var steps = 0
        stepper.advance(to: 0) { _ in }
        stepper.advance(to: 5) { _ in steps += 1 }
        #expect(steps == 16) // 1/15 s at 240 Hz
    }

    @Test func springSettlesOnItsTargetWithoutBlowingUp() {
        var water = Water(cols: 84, k: 120, c: 110, d: 9)
        let target = (0..<84).map { Double($0 % 7) * 3 }
        var peak = 0.0
        FixedStepper.warm(4) { dt in
            water.step(dt, target: target)
            peak = max(peak, water.height.map(abs).max() ?? 0)
        }
        #expect(peak < 30)
        #expect(water.velocity.allSatisfy { abs($0) < 0.05 })
        // neighbours pull each column a little off its target, never far
        #expect(zip(water.height, target).allSatisfy { abs($0 - $1) < 6 })
    }

    @Test func oneStepFollowsTheSpringLaw() {
        var water = Water(cols: 3, k: 120, c: 110, d: 9)
        water.height = [0, 1, 0]
        water.step(0.01, target: [0, 0, 0])
        // v = (k·(0 - 1) + c·(0 + 0 - 2) - d·0)·dt, then h += v·dt
        let v = (-120.0 - 220) * 0.01
        #expect(abs(water.velocity[1] - v) < 1e-12)
        #expect(abs(water.height[1] - (1 + v * 0.01)) < 1e-12)
        #expect(abs(water.velocity[0] - 110 * 0.01) < 1e-12)
    }

    @Test func squareRootScale() {
        #expect(TideScale.y(16000, top: 64000) == 0.5)
        #expect(TideScale.y(64000, top: 64000) == 1)
        #expect(TideScale.y(200_000, top: 64000) == 1)
        #expect(TideScale.y(-5, top: 64000) == 0)
        // the threshold at 1/1.6 of the top sits on sqrt(0.625) of 24 rows
        #expect(TideScale.thresholdRow(rows: 24, threshold: 40000, top: 64000) == 24 - 19)
    }

    @Test func targetsKeepAMinuteAsWideAsItIs() {
        var minutes = Array(repeating: 0.0, count: 60)
        minutes[30] = 64000
        let t = TideScale.targets(minutes, cols: 84, rows: 24, top: 64000)
        #expect(t.count == 84)
        #expect(t.filter { $0 > 0.4 }.count == 2) // minute 30 is columns 42 and 43 (0.714 min each)
        // back to tokens: the two columns hold the whole minute
        let tokens = t.filter { $0 > 0.4 }.map { pow($0 / 24, 2) * 64000 * 60 / 84 }.reduce(0, +)
        #expect(abs(tokens - 64000) < 1)
        #expect(t.allSatisfy { $0 >= 0.4 && $0 <= 24 })
    }

    /// The polish criterion: a single spike, once settled, is no more than 1 column wider than its real value.
    @Test func settledSpikeStaysNarrow() {
        var minutes = Array(repeating: 2000.0, count: 60)
        minutes[30] = 64000
        let history = MinuteHistory(values: minutes, top: 64000, threshold: 40000)
        let flat = TideHistogram(), spike = TideHistogram()
        _ = flat.frame(MinuteHistory(values: Array(repeating: 2000, count: 60), top: 64000, threshold: 40000), at: 0, still: false)
        _ = spike.frame(MinuteHistory(values: Array(repeating: 2000, count: 60), top: 64000, threshold: 40000), at: 0, still: false)
        for frame in 1...240 { _ = spike.frame(history, at: Double(frame) / 60, still: false) }
        let target = TideScale.targets(minutes, cols: 84, rows: 24, top: 64000)
        let base = flat.heights[0]
        func wide(_ h: [Double]) -> Int {
            let top = h.max()! - base
            return h.filter { $0 - base >= top / 2 }.count
        }
        #expect(wide(spike.heights) <= wide(target) + 1)
    }

    /// Task 4: the open chart keeps moving on a flat history, by less than 0.6 rows, and never into the threshold row.
    @Test func idleRipplesStaySmall() {
        let flat = MinuteHistory(values: Array(repeating: 2000, count: 60), top: 64000, threshold: 40000)
        let tide = TideHistogram()
        let row = TideScale.thresholdRow(rows: 24, threshold: 40000, top: 64000)
        var biggest = 0.0, moved = 0
        for frame in 0...(60 * 30) {
            let f = tide.frame(flat, at: Double(frame) / 60, still: false)
            let offset = zip(tide.shownHeights, tide.heights).map { abs($0 - $1) }.max()!
            biggest = max(biggest, offset)
            if offset > 0.05 { moved += 1 }
            for c in 0..<84 { for r in 0...row { #expect(f.level[r * 84 + c] < 0.05) } }
        }
        print("idle ripple: biggest \(biggest) rows, moving in \(moved) of 1801 frames")
        #expect(biggest < 0.6)
        #expect(moved > 1200) // alive, not a drop now and then (slow drops: about 3 frames in 4)
    }

    /// A column just under the threshold row is never lifted into it by a ripple.
    @Test func ripplesNeverLiftIntoTheThresholdRow() {
        let row = TideScale.thresholdRow(rows: 24, threshold: 40000, top: 64000)
        let line = Double(24 - row)
        let history = MinuteHistory(values: Array(repeating: 37000, count: 60), top: 64000, threshold: 40000)
        let tide = TideHistogram()
        for frame in 0...(60 * 20) {
            _ = tide.frame(history, at: Double(frame) / 60, still: false)
            for (shown, h) in zip(tide.shownHeights, tide.heights) { #expect(shown <= max(h, line - 1) + 1e-9) }
        }
    }

    /// Dot motion task 1: the chart's body is brighter (34–70%), the island's tank keeps 20–52%.
    @Test func chartBodyIsBrighterThanTheTank() {
        let history = MinuteHistory(values: Array(repeating: 16000, count: 60), top: 64000, threshold: 40000)
        let f = TideHistogram().frame(history, at: 0, still: true)
        // 12 rows of liquid in column 40: the dot 3 rows under the meniscus (row 12)
        #expect(f.level[15 * 84 + 40] >= 0.5)
        #expect(abs(LiquidBody.tank.light(depth: 3.5) - (0.2 + 0.32 * exp(-3.5 / 4))) < 1e-12)
        #expect(LiquidBody.chart.light(depth: 0) == 0)
        #expect(LiquidBody.chart.light(depth: 100) > 0.34)
    }

    /// Dot motion task 2: body dots rise and sink with the ripple above them, at most 0.5 pt, less with depth.
    @Test func bodyDotsFollowTheRippleByAtMostHalfAPoint() {
        let history = MinuteHistory(values: Array(repeating: 16000, count: 60), top: 64000, threshold: 40000)
        let tide = TideHistogram()
        var biggest: Float = 0, deepMoved: Float = 0
        for frame in 0...(60 * 20) {
            let f = tide.frame(history, at: Double(frame) / 60, still: false)
            biggest = max(biggest, f.lift.map(abs).max()!)
            for c in 0..<84 {
                let shallow = abs(f.lift[13 * 84 + c]), deep = abs(f.lift[23 * 84 + c])
                #expect(deep <= shallow + 1e-6)
                deepMoved = max(deepMoved, deep)
                #expect(f.lift[0 * 84 + c] == 0) // dry dots stay put
            }
        }
        print("body lift: biggest \(biggest) pt, deepest row \(deepMoved) pt")
        #expect(biggest <= 0.5 + 1e-6)
        #expect(biggest > 0.15)
        #expect(deepMoved > 0) // the ripple reaches the bottom row too
        let still = tide.frame(history, at: 20.1, still: true)
        #expect(still.lift.allSatisfy { $0 == 0 })
    }

    /// 10m moves like the island: more ripple than the long ranges, more with the rate, still bounded and never into
    /// the threshold row.
    @Test func tenMinutesRipplesLikeTheIsland() {
        let row = TideScale.thresholdRow(rows: 24, threshold: 40000, top: 64000)
        let line = Double(24 - row)
        func run(_ rate: Double, _ motion: TideHistogram.Motion) -> Double {
            let history = MinuteHistory(values: Array(repeating: rate, count: 10), top: 64000, threshold: 40000)
            let tide = TideHistogram()
            var total = 0.0
            for frame in 0...(60 * 20) {
                _ = tide.frame(history, at: Double(frame) / 60, still: false, motion: motion)
                let moved = zip(tide.shownHeights, tide.heights).map { abs($0 - $1) }
                #expect(moved.max()! <= 1.2 + 1e-9)
                for (shown, h) in zip(tide.shownHeights, tide.heights) { #expect(shown <= max(h, line - 1) + 1e-9) }
                total += moved.reduce(0, +)
            }
            return total
        }
        let calm = run(2000, .calm), quiet = run(2000, .lively), busy = run(30000, .lively)
        print("ripple totals: calm \(calm), lively quiet \(quiet), lively busy \(busy)")
        #expect(quiet > calm * 2)
        #expect(busy > quiet)
    }

    @Test func stillHistogramHasNoRipples() {
        let flat = MinuteHistory(values: Array(repeating: 2000, count: 60), top: 64000, threshold: 40000)
        let tide = TideHistogram()
        for frame in 0...600 { _ = tide.frame(flat, at: Double(frame) / 60, still: false) }
        _ = tide.frame(flat, at: 10.1, still: true)
        #expect(tide.shownHeights == tide.heights)
        #expect(tide.heights == TideScale.targets(flat.values, cols: 84, rows: 24, top: 64000))
    }

    @Test func histogramIsRedOnlyAboveTheThreshold() {
        let history = MinuteHistory(values: Array(repeating: 64000, count: 60), top: 64000, threshold: 40000)
        let f = TideHistogram().frame(history, at: 0, still: true)
        let row = TideScale.thresholdRow(rows: 24, threshold: 40000, top: 64000)
        #expect(f.hot[0])
        #expect(!f.hot[row * 84])
    }

    /// A past minute over the threshold is coral even when the rate now is calm; the calm minutes stay pearl.
    @Test func pastMinutesOverTheThresholdAreCoral() {
        var minutes = Array(repeating: 2000.0, count: 60)
        for m in 20...29 { minutes[m] = 60000 }
        let f = TideHistogram().frame(MinuteHistory(values: minutes, top: 64000, threshold: 40000), at: 0, still: true)
        let row = TideScale.thresholdRow(rows: 24, threshold: 40000, top: 64000)
        let spikeCol = 25 * 84 / 60, calmCol = 70
        #expect((0..<row).contains { f.hot[$0 * 84 + spikeCol] })
        #expect(!(0..<24).contains { f.hot[$0 * 84 + calmCol] })
        #expect(!(row..<24).contains { f.hot[$0 * 84 + spikeCol] })
    }

    /// A spring overshooting the threshold row is not a real value: it stays pearl.
    @Test func overshootNeverTurnsCoral() {
        var minutes = Array(repeating: 2000.0, count: 60)
        for m in 26...34 { minutes[m] = 37000 } // just under the 40000 threshold
        let tide = TideHistogram()
        _ = tide.frame(MinuteHistory(values: Array(repeating: 2000, count: 60), top: 64000, threshold: 40000), at: 0, still: false)
        var sawOvershoot = false
        let row = TideScale.thresholdRow(rows: 24, threshold: 40000, top: 64000)
        for frame in 1...120 {
            // every few frames the spike moves one minute left, like minutes closing quickly
            let k = frame / 20
            let shifted = Array(minutes.dropFirst(k)) + Array(repeating: 2000, count: k)
            let f = tide.frame(MinuteHistory(values: shifted, top: 64000, threshold: 40000), at: Double(frame) / 60, still: false)
            if tide.heights.contains(where: { $0 > Double(24 - row) }) { sawOvershoot = true }
            #expect(!f.hot.contains(true))
        }
        #expect(sawOvershoot) // the test means something only if a spring did overshoot
    }

    @Test func meniscusIsTheBrightestRow() {
        let history = MinuteHistory(values: Array(repeating: 16000, count: 60), top: 64000, threshold: 40000)
        let f = TideHistogram().frame(history, at: 0, still: true)
        let column = (0..<24).map { f.level[$0 * 84 + 40] } // 12 rows of liquid
        let brightest = column.indices.max { column[$0] < column[$1] }!
        #expect(brightest == 24 - 12)
        #expect(column[0] == 0) // dry above
        #expect(column[23] > 0.2 && column[23] < column[brightest]) // the body below is dimmer
    }

    @Test func islandLevelPutsTheThresholdAtThreeQuarters() {
        #expect(TideIsland.level(rate: 40000, threshold: 40000) == 0.75)
        #expect(TideIsland.level(rate: 400_000, threshold: 40000) == 1)
        #expect(TideIsland.level(rate: 5, threshold: 0) == 0)
        #expect(TideIsland.thresholdRow == 3)
    }

    /// The polish criterion: the drops never lift a calm surface over the threshold row.
    @Test func calmDropsStayBelowTheThreshold() {
        let tank = TideIsland()
        let level = TideIsland.level(rate: 11000, threshold: 40000)
        for frame in 0...(60 * 30) {
            let f = tank.frame(at: Double(frame) / 60, level: level, urgent: false, still: false)
            for c in 0..<TideIsland.cols {
                for r in 0..<TideIsland.thresholdRow { #expect(f.level[r * TideIsland.cols + c] < 0.05) }
            }
        }
    }

    @Test func stillIslandIsFlat() {
        let f = TideIsland().frame(at: 3, level: 0.5, urgent: false, still: true)
        let rows = (0..<TideIsland.rows).map { r in (0..<TideIsland.cols).map { f.level[r * TideIsland.cols + $0] } }
        #expect(rows.allSatisfy { Set($0).count == 1 })
    }

    @Test func shadeFadesTheRimAndCapsTheGlow() {
        var f = LiquidField(cols: 36, rows: 1)
        f.level = Array(repeating: 1, count: 36)
        f.fresh = Array(repeating: 1, count: 36)
        let edge = LiquidShade.shade(f, at: 0, floor: 0.06, rimCols: 3)
        let middle = LiquidShade.shade(f, at: 18, floor: 0.06, rimCols: 3)
        #expect(edge.alpha < middle.alpha)
        #expect(middle.alpha == 1 && middle.glow == 0.8 && middle.radiusScale == 1)
        f.level[18] = 0
        let off = LiquidShade.shade(f, at: 18, floor: 0.06, rimCols: 3)
        #expect(off.alpha == 0.06 && off.glow == 0 && abs(off.radiusScale - 0.72) < 1e-6)
    }
}
