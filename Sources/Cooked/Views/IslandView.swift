import SwiftUI

/// The island under the menu bar: a 44 pt capsule with the wave, the active account's 5h % left and the rate.
/// In urgency it keeps the same shape: the percentage and the hairline turn red, the details are in the popover.
struct IslandView: View {
    let model: IslandModel
    var namespace: Namespace.ID
    @Environment(\.self) private var env
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.solidSurfaces) private var solidSurfaces

    /// The rate sits on the glass: ink3 there falls to 4.2:1 over a white window, so one step brighter.
    private var rateInk: Color { reduceTransparency || solidSurfaces ? Palette.ink3 : Palette.ink2 }

    var body: some View {
        let snapshot = model.snapshot
        let shape = Capsule(style: .continuous)
        calm(snapshot)
            .clipShape(shape)
            .modifier(IslandSurface(shape: shape, solidFill: Palette.bgBase, urgent: snapshot.isUrgent,
                                    glassID: "island", namespace: namespace))
    }

    private func calm(_ s: UsageSnapshot) -> some View {
        HStack(spacing: 12) {
            TideIslandChart(rate: Double(model.rateNow), threshold: Double(s.threshold), urgent: s.isUrgent)
                .padding(.horizontal, 8)
                .frame(height: 36)
                .wellSurface(Capsule(), innerBlur: 2.5)
            // Urgency: the capsule's percentage and ring turn red. Red 13 pt needs 4.5:1, which no glass
            // reaches (#2d2d2d at best, 4.0:1): there it sits in a well, red on #1b1b1b is 5.1:1. Same layout.
            Text("\(s.active?.fiveHourLeft ?? 100)%").textStyle(.numberEmphasis)
                .foregroundStyle(s.isUrgent ? Palette.red : Palette.ink)
                .background {
                    if s.isUrgent {
                        Color.clear.wellSurface(Capsule(), innerBlur: 2)
                            .padding(.horizontal, -6).frame(height: 24)
                    }
                }
            Text(s.active?.short ?? "").textStyle(.body).foregroundStyle(Palette.ink2)
            Text("\(TokenFormat.short(Double(model.rateNow)))/min").textStyle(.number).foregroundStyle(rateInk)
                .contentTransition(.numericText(value: Double(model.rateNow)))
                .animation(Motion.animation(Motion.control, reduceMotion: env.reducesMotion), value: model.rateNow)
        }
        .lineLimit(1)
        .fixedSize()
        .padding(.leading, 4)
        .padding(.trailing, 16)
        .frame(height: 44)
    }

    /// nil brings T3 Code to the front.
    static func peakThread(_ s: UsageSnapshot) -> String? { peakSource(s)?.threadID }

    /// The top source of the account that makes the island urgent, else the top source.
    static func peakSource(_ s: UsageSnapshot) -> TokenSource? {
        UsageSnapshot.peakSource(s.sources, account: s.active?.short)
    }
}
