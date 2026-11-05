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

    /// `seed` fixes the drops, for tests: tests run in parallel, so the shared next seed differs from run to run.
    init(seed: UInt64? = nil) {
        if let seed {
            self.seed = seed
        } else {
            self.seed = Self.nextSeed
            Self.nextSeed = Self.nextSeed % 2_147_483_000 + 7919
        }
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

/// One window of one account, counted like T3 Code's Usage page: three rows of the chart's dots lit for the % left,
/// and a line at the share of time left to the reset. Left of the line, the dark dots are the overspend (less left than
/// time left): red when the account fills before reset. Each row's front sways on a spring (`PaceFronts`); the dot at
/// the front is partly lit, and the lit dots behind it move with it, less the further back they are.
struct PaceStrip: View {
    /// % left, 0...100.
    let left: Int
    /// Share of the window still to go, 0...1; nil hides the line.
    let timeLeft: Double?
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
    /// The overspend's dark dots.
    static let overspendAlpha = 0.32
    static var dotsHeight: CGFloat { dot + CGFloat(rows - 1) * pitchY }
    static var height: CGFloat { dotsHeight + 2 * lineOut }
    /// A lit dot never moves further than this from its grid place, in points.
    static let maxShift = 0.5
    /// Columns behind the front over which a dot's movement fades by e.
    static let shiftReach = 3.0

    static func columns(width: CGFloat) -> Int { max(6, Int((width - dot) / pitchX) + 1) }

    /// The fill in columns and the line's column (no dots there).
    static func layout(left: Int, timeLeft: Double?, cols: Int) -> (fill: Double, line: Int?) {
        let fill = Double(min(max(left, 0), 100)) / 100 * Double(cols)
        guard let timeLeft else { return (fill, nil) }
        return (fill, min(cols - 1, Int((min(max(timeLeft, 0), 1) * Double(cols - 1)).rounded())))
    }

    /// True for a dark dot of a hot strip between the front and the line.
    static func overspend(column c: Int, cover: Double, line: Int?, hot: Bool) -> Bool {
        guard hot, let line else { return false }
        return cover == 0 && c < line
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
                let (fill, line) = Self.layout(left: left, timeLeft: timeLeft, cols: cols)
                for r in 0..<Self.rows {
                    let y = Self.lineOut + CGFloat(r) * Self.pitchY
                    for c in 0..<cols where c != line {
                        let cover = Self.cover(column: c, fill: fill, sway: sway[r])
                        let over = Self.overspend(column: c, cover: cover, line: line, hot: hot)
                        let lit = (dim ? 0.6 : 1) * cover
                        let x = CGFloat(c) * gapX + CGFloat(Self.shift(column: c, fill: fill, sway: sway[r]))
                        let rect = CGRect(x: x, y: y, width: Self.dot, height: Self.dot)
                        let color = over ? Palette.red.opacity(Self.overspendAlpha) : Palette.ink.opacity(0.07 + lit * 0.93)
                        ctx.fill(Path(ellipseIn: rect), with: .color(color))
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

/// A window's pill: a top line with the account
/// name (5-hour pill only), the % left and the times on the right; the strip under it, the pill's full width.
struct UsagePill<Times: View>: View {
    var name: String? = nil
    /// % left; nil shows "—".
    let left: Int?
    let timeLeft: Double?
    let hot: Bool
    let live: Bool
    @ViewBuilder var times: Times

    static var height: CGFloat { 5 + TextStyle.label.lineHeight + 1 + PaceStrip.height + 4 }

    var body: some View {
        VStack(spacing: 1) {
            HStack(spacing: 8) {
                if let name {
                    Text(name).textStyle(live ? .emphasis : .body).foregroundStyle(live ? Palette.ink : Palette.ink2)
                        .frame(height: TextStyle.label.lineHeight)
                }
                Text(left.map { "\($0)%" } ?? "—").textStyle(.labelNumber)
                    .foregroundStyle(hot ? Palette.red : live ? Palette.ink : Palette.ink2)
                Spacer(minLength: 4)
                // the times never truncate: the name gives way first
                times.fixedSize().layoutPriority(1)
            }
            .lineLimit(1)
            PaceStrip(left: left ?? 0, timeLeft: left == nil ? nil : timeLeft, hot: hot, dim: !live)
        }
        .padding(EdgeInsets(top: 5, leading: 9, bottom: 4, trailing: 9))
        .frame(height: Self.height)
        .wellSurface(RoundedRectangle(cornerRadius: 10, style: .continuous), innerBlur: 2)
    }
}

/// The times on a pill's top line: "full 13:42" in red then the reset, or "resets 14:30".
struct PillTimes: View {
    var full: String? = nil
    let reset: String
    var red = false

    var body: some View {
        HStack(spacing: 8) {
            if let full {
                Text("\(Text("full "))\(Text(full).styled(.labelNumber))").styled(.label).foregroundStyle(Palette.red)
                Text(reset).textStyle(.labelNumber).foregroundStyle(Palette.ink3)
            } else if reset.isEmpty {
                // nothing to say: the pill's own "—" already says there is no number
                EmptyView()
            } else if reset == "—" {
                Text("—").textStyle(.labelNumber).foregroundStyle(Palette.ink3)
            } else {
                Text("\(Text("resets "))\(Text(reset).styled(.labelNumber))").styled(.label)
                    .foregroundStyle(red ? Palette.red : Palette.ink3)
            }
        }
        .frame(height: TextStyle.label.lineHeight)
    }
}

/// The chart's time ranges under the chart: a small
/// segmented control, the chosen range lifted on a lighter key.
struct RangeControl: View {
    @Binding var selection: ChartRange
    @Environment(\.self) private var env

    var body: some View {
        HStack(spacing: 2) {
            ForEach(ChartRange.allCases) { range in
                let on = range == selection
                Button { selection = range } label: {
                    Text(range.rawValue)
                        .font(.custom(PlexFont.monoMedium, size: 11))
                        .foregroundStyle(on ? Palette.ink : Palette.ink3)
                        .padding(.horizontal, 6)
                        .frame(height: 19)
                        .background {
                            if on {
                                RoundedRectangle(cornerRadius: 6, style: .continuous)
                                    .fill(Color(white: 0.165))
                                    .overlay(RoundedRectangle(cornerRadius: 6, style: .continuous)
                                        .strokeBorder(.white.opacity(0.14), lineWidth: 0.5))
                            }
                        }
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .pointingHand()
                .accessibilityAddTraits(on ? .isSelected : [])
            }
        }
        .padding(2)
        .background(RoundedRectangle(cornerRadius: 8, style: .continuous).fill(Color(white: 0.05))
            .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).strokeBorder(.white.opacity(0.08), lineWidth: 0.5)))
        .animation(Motion.animation(Motion.control, reduceMotion: env.reducesMotion), value: selection)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Chart range")
    }
}
