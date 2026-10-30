import SwiftUI

/// The island under the menu bar. Calm: a 44 pt capsule with the wave, the active account and the rate.
/// Peak: a 404 pt card with the 5h percentage in red, when the window fills up, and a button to its top thread.
struct IslandView: View {
    let model: IslandModel
    var namespace: Namespace.ID
    @Environment(\.self) private var env
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.solidSurfaces) private var solidSurfaces

    /// The rate sits on the glass: ink3 there falls to 4.2:1 over a white window, so one step brighter.
    private var rateInk: Color { reduceTransparency || solidSurfaces ? Palette.ink3 : Palette.ink2 }

    /// The island's equaliser: the last 18 minutes (20 in the peak card) of the popover's series.
    /// One surface for both states, so its shape grows from the capsule (radius 22 = half of 44) to the card.
    var body: some View {
        let snapshot = model.snapshot
        let card = model.showsPeakCard
        let shape = RoundedRectangle(cornerRadius: card ? 24 : 22, style: .continuous)
        ZStack {
            if card, let urgency = snapshot.urgency {
                peak(snapshot, urgency)
                    .frame(width: IslandLayout.popoverWidth, alignment: .leading)
                    .transition(.blurIn(reduceMotion: env.reducesMotion))
            } else {
                calm(snapshot)
                    .transition(.blurIn(reduceMotion: env.reducesMotion))
            }
        }
        .clipShape(shape)
        .modifier(IslandSurface(shape: shape, solidFill: Palette.bgBase, urgent: snapshot.isUrgent,
                                redHero: card,
                                glassID: "island", namespace: namespace))
    }

    private func calm(_ s: UsageSnapshot) -> some View {
        HStack(spacing: 12) {
            TideIslandChart(rate: Double(model.rateNow), threshold: Double(s.threshold), urgent: s.isUrgent)
                .padding(.horizontal, 8)
                .frame(height: 36)
                .wellSurface(Capsule(), innerBlur: 2.5)
            // Minimized peak: the capsule keeps the red percentage and ring. Red 13 pt needs 4.5:1, which no glass
            // reaches (#2d2d2d at best, 4.0:1): there it sits in a well, red on #1b1b1b is 5.1:1. Same layout.
            Text("\(s.active?.fiveHourPercent ?? 0)%").textStyle(.numberEmphasis)
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

    private func peak(_ s: UsageSnapshot, _ u: Urgency) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 16) {
                Text("\(u.percent)%").textStyle(.hero).foregroundStyle(Palette.red)
                VStack(alignment: .leading, spacing: 0) {
                    Text("\(Text(u.lead).styled(.emphasis))\(Text(u.fullAt).styled(.number))")
                        .foregroundStyle(Palette.ink)
                        .frame(height: TextStyle.emphasis.lineHeight)
                    Text(u.detail).textStyle(.body).foregroundStyle(Palette.ink2)
                        .truncationMode(.tail)
                }
                .lineLimit(1)
                Spacer(minLength: 0)
                TideIslandChart(rate: Double(model.rateNow), threshold: Double(s.threshold), urgent: true)
                    .padding(.horizontal, 6)
                    .wellSurface(RoundedRectangle(cornerRadius: 8, style: .continuous))
            }
            HStack(spacing: 12) {
                Text(u.source).textStyle(.body).foregroundStyle(Palette.ink2)
                    .lineLimit(1).truncationMode(.tail)
                Spacer(minLength: 0)
                Button("Minimize") { model.peakMinimized = true }
                    .buttonStyle(SecondaryButtonStyle())
                // The top source of the account that fills up, else the top source: what to check or stop.
                Button("Open thread ↗") { T3Code.open(thread: Self.peakThread(s)) }
                    .buttonStyle(PrimaryButtonStyle())
                    .pointingHand()
            }
        }
        .padding(16)
    }

    /// nil brings T3 Code to the front.
    static func peakThread(_ s: UsageSnapshot) -> String? { peakSource(s)?.threadID }

    /// The top source of the account that makes the island urgent, else the top source.
    static func peakSource(_ s: UsageSnapshot) -> TokenSource? {
        UsageSnapshot.peakSource(s.sources, account: s.active?.short)
    }
}
