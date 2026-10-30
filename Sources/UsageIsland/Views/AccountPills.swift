import SwiftUI

/// The fronts of a strip's three rows of dots, each on its own spring, kicked by slow drops and settling again, so the
/// rows never line up exactly.
@MainActor final class PaceFronts {
    static let rows = 3
    /// The springs run at this share of real time, like the chart's ripple.
    static let slow = 0.6
    /// Kicks per second of spring time.
    static let dropRate = 0.5
    /// A front never sways further than this from the used %, in columns.
    static let maxSway = 0.7

    /// Each strip draws its own drops: a new seed per strip.
    private static var nextSeed: UInt64 = 11
    private var water = Water(cols: rows, k: 18, c: 60, d: 3.2)
    private var stepper = FixedStepper()
    private var drip: Double
    private var seed: UInt64

    init() {
        seed = Self.nextSeed
        Self.nextSeed = Self.nextSeed % 2_147_483_000 + 7919
        drip = 0
        drip = random() * 0.8
    }

    /// Each row's sway at `t`, in columns, within ±`maxSway`; all 0 when still.
    func sway(at t: Double, still: Bool) -> [Double] {
        if still {
            water = Water(cols: Self.rows, k: water.k, c: water.c, d: water.d)
            stepper.advance(to: t) { _ in }
            return Array(repeating: 0, count: Self.rows)
        }
        stepper.advance(to: t, step)
        return water.height.map { max(-Self.maxSway, min(Self.maxSway, $0)) }
    }

    private func step(_ dt: Double) {
        let slowed = dt * Self.slow
        drip += slowed * Self.dropRate
        while drip > 1 {
            drip -= 1
            water.velocity[min(Self.rows - 1, Int(random() * Double(Self.rows)))] += (random() - 0.5) * 9
        }
        water.step(slowed, target: Array(repeating: 0, count: Self.rows))
    }

    /// Park–Miller, like the chart.
    private func random() -> Double {
        seed = (seed * 16807) % 2_147_483_647
        return Double(seed) / 2_147_483_647
    }
}

/// One window of one account: three rows of the chart's dots, lit up to the used %, and the even-pace line where the
/// window's time has gone by. Lit dots past the line are red when the account fills before reset. Each row's front
/// sways on a spring (`PaceFronts`); the dot at the front is partly lit, and the lit dots behind it move with it,
/// less the further back they are.
struct PaceStrip: View {
    let percent: Int
    /// 0...1; nil hides the line.
    let elapsed: Double?
    let hot: Bool
    /// Accounts not in use: their lit dots at 60%.
    var dim = false
    @State private var fronts = PaceFronts()

    static let rows = PaceFronts.rows
    static let dot: CGFloat = 2.3
    static let pitchX: CGFloat = 3
    static let pitchY: CGFloat = 4
    /// The line is 2.5 pt longer than the dots above and below.
    static let lineOut: CGFloat = 2.5
    static let lineWidth: CGFloat = 1.6
    static let lineInk = Palette.ink.opacity(0.72)
    static var dotsHeight: CGFloat { dot + CGFloat(rows - 1) * pitchY }
    static var height: CGFloat { dotsHeight + 2 * lineOut }
    /// A lit dot never moves further than this from its grid place, in points.
    static let maxShift = 0.5
    /// Columns behind the front over which a dot's movement fades by e.
    static let shiftReach = 3.0

    static func columns(width: CGFloat) -> Int { max(6, Int((width - dot) / pitchX) + 1) }

    /// The fill in columns, the line's column (no dots there) and the first column ahead of even pace.
    static func layout(percent: Int, elapsed: Double?, cols: Int) -> (fill: Double, line: Int?, ahead: Double) {
        let fill = Double(min(max(percent, 0), 100)) / 100 * Double(cols)
        guard let elapsed else { return (fill, nil, .infinity) }
        return (fill, min(cols - 1, Int((elapsed * Double(cols - 1)).rounded())), elapsed * Double(cols))
    }

    /// 0...1, how lit column `c` is when the row's front sits at `fill + sway` columns. A nearly empty strip does not sway.
    static func cover(column c: Int, fill: Double, sway: Double) -> Double {
        clamp01((fill > 0.2 ? fill + sway : fill) - Double(c))
    }

    /// Points a lit dot moves right (negative: left) when its row's front sways by `sway` columns.
    static func shift(column c: Int, fill: Double, sway: Double) -> Double {
        guard fill > 0.2, Double(c) < fill + sway else { return 0 }
        return sway / PaceFronts.maxSway * maxShift * exp(-max(0, fill - 1 - Double(c)) / shiftReach)
    }

    var body: some View {
        ChartTimeline { t, still in
            let sway = fronts.sway(at: t, still: still)
            Canvas { ctx, size in
                let cols = Self.columns(width: size.width)
                let gapX = cols > 1 ? (size.width - Self.dot) / CGFloat(cols - 1) : 0
                let (fill, line, ahead) = Self.layout(percent: percent, elapsed: elapsed, cols: cols)
                for r in 0..<Self.rows {
                    let y = Self.lineOut + CGFloat(r) * Self.pitchY
                    for c in 0..<cols where c != line {
                        let cover = Self.cover(column: c, fill: fill, sway: sway[r])
                        let red = cover > 0 && hot && Double(c) >= ahead
                        let lit = (dim && !red ? 0.6 : 1) * cover
                        let x = CGFloat(c) * gapX + CGFloat(Self.shift(column: c, fill: fill, sway: sway[r]))
                        let rect = CGRect(x: x, y: y, width: Self.dot, height: Self.dot)
                        ctx.fill(Path(ellipseIn: rect), with: .color((red ? Palette.red : Palette.ink).opacity(0.07 + lit * 0.93)))
                    }
                }
                if let line {
                    let x = CGFloat(line) * gapX + (Self.dot - Self.lineWidth) / 2
                    let rect = CGRect(x: x, y: 0, width: Self.lineWidth, height: Self.height)
                    ctx.fill(Path(roundedRect: rect, cornerRadius: Self.lineWidth / 2), with: .color(Self.lineInk))
                }
            }
        }
        .frame(height: Self.height)
        .accessibilityHidden(true)
    }
}

/// A T3-style pill: the % on the left, the pace strip, the reset (or "full at" in red) on the right.
struct AccountPill: View {
    let percent: Int?
    let elapsed: Double?
    /// "14:30", "Mon", or the "full at" time when `hot`.
    let trailing: String
    let hot: Bool
    let live: Bool

    static let height: CGFloat = 28
    static let percentColumn: CGFloat = 30
    /// "00:00" in Plex Mono 12 is 36 pt.
    static let trailingColumn: CGFloat = 38

    var body: some View {
        HStack(spacing: 6) {
            Text(percent.map { "\($0)%" } ?? "—").textStyle(.labelNumber)
                .foregroundStyle(hot ? Palette.red : live ? Palette.ink : Palette.ink2)
                .frame(width: Self.percentColumn, alignment: .trailing)
            PaceStrip(percent: percent ?? 0, elapsed: percent == nil ? nil : elapsed, hot: hot, dim: !live)
                .frame(maxWidth: .infinity)
            Text(trailing).textStyle(.labelNumber)
                .foregroundStyle(hot ? Palette.red : Palette.ink3)
                .frame(width: Self.trailingColumn, alignment: .trailing)
        }
        .lineLimit(1)
        .padding(.horizontal, 8)
        .frame(height: Self.height)
        .wellSurface(RoundedRectangle(cornerRadius: 9, style: .continuous), innerBlur: 2)
    }
}

/// The "even pace" key in the Usage header: the same line as in the strips.
struct EvenPaceKey: View {
    var body: some View {
        HStack(spacing: 5) {
            RoundedRectangle(cornerRadius: PaceStrip.lineWidth / 2)
                .fill(PaceStrip.lineInk)
                .frame(width: PaceStrip.lineWidth, height: 12)
            Text("even pace")
        }
        .accessibilityElement(children: .combine)
    }
}

/// The alert key in the chart header: a few dots, as the chart draws its threshold row.
struct AlertKey: View {
    var body: some View {
        HStack(spacing: 2.2) {
            ForEach(0..<3, id: \.self) { _ in
                Circle().fill(Palette.ink.opacity(0.45)).frame(width: 2.3, height: 2.3)
            }
        }
        .accessibilityHidden(true)
    }
}
