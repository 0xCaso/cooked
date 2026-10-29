import SwiftUI

/// The island under the menu bar. Calm: a 44 pt capsule with the wave, the active account and the rate.
/// Peak: a 404 pt card with the 5h percentage in red, when the window fills up, and the switch button.
struct IslandView: View {
    let model: IslandModel
    var namespace: Namespace.ID
    @Environment(\.self) private var env

    private static let calmWave = DemoData.series(count: 18, peak: false)
    private static let peakWave = DemoData.series(count: 20, peak: true)

    /// One surface for both states, so its shape grows from the capsule (radius 22 = half of 44) to the card.
    var body: some View {
        let snapshot = model.snapshot
        let shape = RoundedRectangle(cornerRadius: snapshot.isUrgent ? 24 : 22, style: .continuous)
        ZStack {
            if let urgency = snapshot.urgency {
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
                                glassID: "island", namespace: namespace))
    }

    private func calm(_ s: UsageSnapshot) -> some View {
        HStack(spacing: 12) {
            WaveChart(values: Self.calmWave, threshold: Double(s.threshold), urgent: false)
                .padding(.horizontal, 8)
                .frame(height: 36)
                .wellSurface(Capsule(), innerBlur: 2.5)
            Text("\(s.active?.fiveHourPercent ?? 0)%").textStyle(.numberEmphasis).foregroundStyle(Palette.ink)
            Text(s.active?.short ?? "").textStyle(.body).foregroundStyle(Palette.ink2)
            Text("\(TokenFormat.short(Double(model.rateNow)))/min").textStyle(.number).foregroundStyle(Palette.ink3)
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
                    Text("\(Text("100% at ").styled(.emphasis))\(Text(u.fullAt).styled(.number))")
                        .foregroundStyle(Palette.ink)
                        .frame(height: TextStyle.emphasis.lineHeight)
                    Text(u.detail).textStyle(.body).foregroundStyle(Palette.ink2)
                        .truncationMode(.tail)
                }
                .lineLimit(1)
                Spacer(minLength: 0)
                WaveChart(values: Self.peakWave, threshold: Double(s.threshold), urgent: true)
                    .padding(.horizontal, 6)
                    .wellSurface(RoundedRectangle(cornerRadius: 8, style: .continuous))
            }
            HStack(spacing: 12) {
                Text(u.source).textStyle(.body).foregroundStyle(Palette.ink2)
                    .lineLimit(1).truncationMode(.tail)
                Spacer(minLength: 0)
                // Switching accounts is outside this plan: the button does nothing yet.
                Button(u.switchLabel) {}
                    .buttonStyle(PrimaryButtonStyle())
            }
        }
        .padding(16)
    }
}
