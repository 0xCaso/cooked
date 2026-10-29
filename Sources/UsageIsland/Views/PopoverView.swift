import SwiftUI

/// The popover under the island: tokens per minute over the last hour, the 5h window of every account, the week.
/// Glass outside, opaque cards and wells inside, so the data never sits on glass.
struct PopoverView: View {
    let model: IslandModel
    let histogram: HistogramClock
    var namespace: Namespace.ID
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.solidSurfaces) private var solidSurfaces
    @Environment(\.self) private var env

    /// Section labels sit on the glass, over a moving backdrop: one step brighter there.
    private var sectionInk: Color { reduceTransparency || solidSurfaces ? Palette.ink3 : Palette.ink2 }

    var body: some View {
        let s = model.snapshot
        VStack(alignment: .leading, spacing: 0) {
            section("Tokens per minute", "last 60 min")
                .padding(EdgeInsets(top: 8, leading: 12, bottom: 8, trailing: 12))
            rateCard(s)
            section("5-hour window", "resets")
                .padding(EdgeInsets(top: 12, leading: 12, bottom: 8, trailing: 12))
            accountsCard(s)
            Text("\(Text("Week: \(s.week.short) "))\(Text("\(s.week.percent)%").styled(.labelNumber)) · resets \(s.week.reset)")
                .styled(.label)
                .foregroundStyle(sectionInk)
                .frame(height: TextStyle.label.lineHeight)
                .padding(EdgeInsets(top: 12, leading: 12, bottom: 8, trailing: 12))
        }
        .padding(8)
        .frame(width: IslandLayout.popoverWidth)
        .modifier(IslandSurface(shape: RoundedRectangle(cornerRadius: 24, style: .continuous), solidFill: Palette.bgDeep,
                                glassID: "popover", namespace: namespace))
    }

    private func section(_ title: String, _ trailing: String) -> some View {
        HStack {
            Text(title)
            Spacer()
            Text(trailing)
        }
        .textStyle(.label)
        .foregroundStyle(sectionInk)
    }

    private func rateCard(_ s: UsageSnapshot) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(TokenFormat.short(Double(model.rateNow))).textStyle(.hero)
                    .foregroundStyle(s.isUrgent ? Palette.red : Palette.ink)
                    .contentTransition(.numericText(value: Double(model.rateNow)))
                    .animation(Motion.animation(Motion.control, reduceMotion: env.reducesMotion), value: model.rateNow)
                Text("now").textStyle(.body).foregroundStyle(Palette.ink2)
                Spacer(minLength: 0)
                Text("\(Text("p99 threshold · "))\(Text(TokenFormat.short(Double(s.threshold))).styled(.labelNumber))/min")
                    .styled(.label)
                    .foregroundStyle(Palette.ink3)
                    .frame(height: TextStyle.label.lineHeight)
            }
            .lineLimit(1)
            .padding(.bottom, 12)
            HistogramChart(clock: histogram, urgent: s.isUrgent)
                .padding(.top, 8)
                .padding(.bottom, 6)
                .frame(maxWidth: .infinity)
                .wellSurface(RoundedRectangle(cornerRadius: 8, style: .continuous))
                .padding(.horizontal, -4)
            HStack {
                Text(s.historyStart).textStyle(.labelNumber)
                Spacer()
                Text("now").textStyle(.label)
            }
            .foregroundStyle(Palette.ink3)
            .padding(.top, 6)
        }
        .padding(12)
        .cardSurface()
    }

    private func accountsCard(_ s: UsageSnapshot) -> some View {
        VStack(spacing: 0) {
            ForEach(Array(s.accounts.enumerated()), id: \.element.id) { i, a in
                accountRow(a, hot: s.isUrgent && a.id == s.activeID)
                    .overlay(alignment: .top) {
                        if i > 0 { Rectangle().fill(Palette.line).frame(height: 0.5).offset(y: -0.25) }
                    }
            }
        }
        .padding(.horizontal, 12)
        .cardSurface()
    }

    private func accountRow(_ a: Account, hot: Bool) -> some View {
        let ink = a.isLive ? Palette.ink : Palette.ink2
        return HStack(spacing: 12) {
            Text(a.short).textStyle(a.isLive ? .emphasis : .body)
                .foregroundStyle(ink)
                .frame(width: 76, alignment: .leading)
            AccountBarChart(percent: a.fiveHourPercent, hot: hot)
                .padding(3)
                .wellSurface(RoundedRectangle(cornerRadius: 6, style: .continuous), innerBlur: 2)
                .frame(maxWidth: .infinity, alignment: .leading)
            Text("\(a.fiveHourPercent)%").textStyle(a.isLive ? .numberEmphasis : .number)
                .foregroundStyle(hot ? Palette.red : ink)
                .frame(width: 36, alignment: .trailing)
            Text(a.fiveHourReset).textStyle(.number)
                .foregroundStyle(Palette.ink3)
                .frame(width: 44, alignment: .trailing)
        }
        .lineLimit(1)
        .frame(height: 30)
    }
}
