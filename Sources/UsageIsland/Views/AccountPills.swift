import SwiftUI

/// The light wave the popover's dots share: lit dots
/// brighten and dim between 100% and 48%, in a wave that travels left to right, period 2π × 0.65 s ≈ 4.1 s.
enum DotWave {
    /// Radians per point across the popover: the mockup's 0.32 per chart column of about 6.3 pt.
    static let phasePerPoint = 0.05
    static let secondsPerRadian = 0.65

    /// 0.48...1, the share of a lit dot's light at `x` points from the chart's left edge.
    static func factor(t: Double, x: Double) -> Double {
        0.74 + 0.26 * sin(t / secondsPerRadian - x * phasePerPoint)
    }
}

/// One window of one account: three rows of the chart's dots, lit up to the used %, and the even-pace line where the
/// window's time has gone by. Lit dots past the line are red when the account fills before reset.
struct PaceStrip: View {
    let percent: Int
    /// 0...1; nil hides the line.
    let elapsed: Double?
    let hot: Bool
    /// Accounts not in use: their lit dots at 60%.
    var dim = false

    static let rows = 3
    static let dot: CGFloat = 2.3
    static let pitchX: CGFloat = 3
    static let pitchY: CGFloat = 4
    /// The line is 2.5 pt longer than the dots above and below.
    static let lineOut: CGFloat = 2.5
    static let lineWidth: CGFloat = 1.6
    static let lineInk = Palette.ink.opacity(0.72)
    static var dotsHeight: CGFloat { dot + CGFloat(rows - 1) * pitchY }
    static var height: CGFloat { dotsHeight + 2 * lineOut }

    static func columns(width: CGFloat) -> Int { max(6, Int((width - dot) / pitchX) + 1) }

    /// Lit columns, the line's column (no dots there) and the first column ahead of even pace.
    static func layout(percent: Int, elapsed: Double?, cols: Int) -> (lit: Int, line: Int?, ahead: Double) {
        let lit = Int((Double(min(max(percent, 0), 100)) / 100 * Double(cols)).rounded())
        guard let elapsed else { return (lit, nil, .infinity) }
        return (lit, min(cols - 1, Int((elapsed * Double(cols - 1)).rounded())), elapsed * Double(cols))
    }

    var body: some View {
        ChartTimeline { t, still in
            Canvas { ctx, size in
                let cols = Self.columns(width: size.width)
                let gapX = cols > 1 ? (size.width - Self.dot) / CGFloat(cols - 1) : 0
                let (lit, line, ahead) = Self.layout(percent: percent, elapsed: elapsed, cols: cols)
                let top = Self.lineOut
                for c in 0..<cols where c != line {
                    let x = CGFloat(c) * gapX
                    let on = c < lit
                    let red = on && hot && Double(c) >= ahead
                    let wave = on && !still ? DotWave.factor(t: t, x: Double(x)) : 1
                    let alpha = on ? (dim && !red ? 0.6 : 1) * wave : 0.07
                    for r in 0..<Self.rows {
                        let rect = CGRect(x: x, y: top + CGFloat(r) * Self.pitchY, width: Self.dot, height: Self.dot)
                        ctx.fill(Path(ellipseIn: rect), with: .color((red ? Palette.red : Palette.ink).opacity(alpha)))
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
