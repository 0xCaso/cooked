import SwiftUI

/// The popover under the island: tokens per minute over the last hour, the 5h window and week of every account.
/// Glass outside, opaque cards and wells inside, so the data never sits on glass.
struct PopoverView: View {
    let model: IslandModel
    let histogram: any HistogramSource
    var namespace: Namespace.ID
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.solidSurfaces) private var solidSurfaces
    @Environment(\.self) private var env

    /// Section labels sit on the glass, over a moving backdrop: one step brighter there.
    private var sectionInk: Color { reduceTransparency || solidSurfaces ? Palette.ink3 : Palette.ink2 }

    var body: some View {
        let s = model.snapshot
        VStack(alignment: .leading, spacing: 0) {
            Text("Tokens per minute")
                .textStyle(.label)
                .foregroundStyle(sectionInk)
                .padding(EdgeInsets(top: 8, leading: 12, bottom: 8, trailing: 12))
            rateCard(s)
            if !s.sources.isEmpty {
                HStack {
                    Text("What burns tokens")
                    Spacer(minLength: 0)
                    Text("last 15 min")
                }
                .lineLimit(1)
                .textStyle(.label)
                .foregroundStyle(sectionInk)
                .padding(EdgeInsets(top: 12, leading: 12, bottom: 8, trailing: 12))
                sourcesCard(s.sources)
            }
            accountsHeader
                .padding(EdgeInsets(top: 12, leading: 12, bottom: 8, trailing: 12))
            accountsCard(s)
        }
        .padding(8)
        .frame(width: IslandLayout.popoverWidth)
        .modifier(IslandSurface(shape: RoundedRectangle(cornerRadius: 24, style: .continuous), solidFill: Palette.bgDeep,
                                glassID: "popover", namespace: namespace))
    }

    /// The section label, with the two time columns of the rows below as headings.
    private var accountsHeader: some View {
        HStack(spacing: Self.rowSpacing) {
            Text("5-hour window")
            Spacer(minLength: 0)
            Text("full at").frame(width: Self.timeColumn, alignment: .trailing)
            Text("resets").frame(width: Self.timeColumn, alignment: .trailing)
            Text("week").frame(width: Self.percentColumn, alignment: .trailing)
        }
        .lineLimit(1)
        .textStyle(.label)
        .foregroundStyle(sectionInk)
    }

    /// Width of the "full at" and "resets" columns: "00:00" in Plex Mono 13 is 39 pt.
    static let timeColumn: CGFloat = 40
    /// Width of the 5h and week % columns: "100%" in Plex Mono 13 is 31.2 pt.
    static let percentColumn: CGFloat = 32
    static let nameColumn: CGFloat = 60
    /// Row: name 60 + bar 126 + two % 64 + two times 80 + 5 gaps of 6 = 360 of the card's 364 pt.
    static let rowSpacing: CGFloat = 6

    private func rateCard(_ s: UsageSnapshot) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(TokenFormat.short(Double(model.rateNow))).textStyle(.hero)
                    .foregroundStyle(s.isUrgent ? Palette.red : Palette.ink)
                    .contentTransition(.numericText(value: Double(model.rateNow)))
                    .animation(Motion.animation(Motion.control, reduceMotion: env.reducesMotion), value: model.rateNow)
                Text("now").textStyle(.body).foregroundStyle(Palette.ink2)
                Spacer(minLength: 0)
                Text("\(Text("alert at "))\(Text(TokenFormat.short(Double(s.threshold))).styled(.labelNumber))/min")
                    .styled(.label)
                    .foregroundStyle(Palette.ink3)
                    .frame(height: TextStyle.label.lineHeight)
            }
            .lineLimit(1)
            .padding(.bottom, 12)
            TideHistogramChart(source: histogram, urgent: s.isUrgent)
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

    private func sourcesCard(_ sources: [TokenSource]) -> some View {
        SourcesCard(sources: sources, hovered: nil)
    }

    private func accountsCard(_ s: UsageSnapshot) -> some View {
        VStack(spacing: 0) {
            ForEach(Array(s.rows.enumerated()), id: \.element.id) { i, a in
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
        return HStack(spacing: Self.rowSpacing) {
            Text(a.short).textStyle(a.isLive ? .emphasis : .body)
                .foregroundStyle(ink)
                .frame(width: Self.nameColumn, alignment: .leading)
            if a.isBlocked {
                // the bar would be full and "full at" would say "now": the reset is what matters
                Text("\(Text("blocked until "))\(Text(a.fiveHourReset).styled(a.isLive ? .numberEmphasis : .number))")
                    .styled(a.isLive ? .emphasis : .body)
                    .foregroundStyle(hot ? Palette.red : ink)
                    .frame(height: TextStyle.body.lineHeight)
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                AccountBarChart(percent: a.fiveHourPercent, hot: hot)
                    .padding(3)
                    .wellSurface(RoundedRectangle(cornerRadius: 6, style: .continuous), innerBlur: 2)
                    .frame(maxWidth: .infinity, alignment: .leading)
                Text("\(a.fiveHourPercent)%").textStyle(a.isLive ? .numberEmphasis : .number)
                    .foregroundStyle(hot ? Palette.red : ink)
                    .frame(width: Self.percentColumn, alignment: .trailing)
                Text(a.fullAt ?? "").textStyle(.number)
                    .foregroundStyle(hot ? Palette.red : ink)
                    .frame(width: Self.timeColumn, alignment: .trailing)
                Text(a.fiveHourReset).textStyle(.number)
                    .foregroundStyle(Palette.ink3)
                    .frame(width: Self.timeColumn, alignment: .trailing)
            }
            Text(a.weekPercent.map { "\($0)%" } ?? "").textStyle(.number)
                .foregroundStyle(ink)
                .frame(width: Self.percentColumn, alignment: .trailing)
        }
        .lineLimit(1)
        .frame(height: 30)
    }
}

/// The sources, each a button. The hovered line lifts out of the card (mockup `.click .src:hover`): its own fill,
/// a shadow, 1 pt up, and the hairlines on both sides fade.
struct SourcesCard: View {
    let sources: [TokenSource]
    @State var hovered: String?
    @Environment(\.self) private var env

    var body: some View {
        VStack(spacing: 0) {
            ForEach(Array(sources.enumerated()), id: \.element.id) { i, source in
                let lifted = hovered == source.id
                let lineHidden = lifted || (i > 0 && hovered == sources[i - 1].id)
                SourceRow(source: source, lifted: lifted)
                    .onHover { inside in
                        if inside { hovered = source.id } else if hovered == source.id { hovered = nil }
                    }
                    .overlay(alignment: .top) {
                        if i > 0 {
                            Rectangle().fill(Palette.line).frame(height: 0.5).offset(y: -0.25)
                                .padding(.horizontal, 8)
                                .opacity(lineHidden ? 0 : 1)
                        }
                    }
                    .zIndex(lifted ? 1 : 0)
            }
        }
        .padding(4)
        .animation(Motion.animation(.spring(duration: 0.2, bounce: 0.15), reduceMotion: env.reducesMotion), value: hovered)
        .cardSurface()
    }
}

/// One source: project and tokens/min, then the thread and who works on it. A click opens its T3 Code thread;
/// lifted (hovered), the agent count gives way to the hint.
private struct SourceRow: View {
    let source: TokenSource
    let lifted: Bool
    @Environment(\.self) private var env

    var body: some View {
        Button { T3Code.open(thread: source.threadID) } label: {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 8) {
                    Text(source.project).textStyle(.emphasis).foregroundStyle(Palette.ink)
                        .truncationMode(.middle)
                    Spacer(minLength: 0)
                    Text("\(Text(TokenFormat.short(source.rate)).styled(.number))\(Text("/min").styled(.number).foregroundStyle(Palette.ink3))")
                        .foregroundStyle(Palette.ink)
                        .layoutPriority(1)
                }
                HStack(spacing: 8) {
                    Text(source.detail).textStyle(.body).foregroundStyle(Palette.ink2)
                        .truncationMode(.tail)
                    Spacer(minLength: 0)
                    Group {
                        if lifted { Text(source.threadID == nil ? "Open T3 Code ↗" : "Open in T3 Code ↗").foregroundStyle(Palette.ink) }
                        else if let agents = source.agents { Text(agents).foregroundStyle(Palette.ink2) }
                    }
                    .textStyle(.body)
                    .layoutPriority(1)
                }
            }
            .lineLimit(1)
            .padding(.horizontal, 8)
            .padding(.vertical, 7)
            .background {
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(Color(white: 0x20 / 255.0))
                    .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .strokeBorder(Color.white.opacity(0.22), lineWidth: 0.5))
                    .shadow(color: .black.opacity(0.45), radius: 7, y: 6)
                    .shadow(color: .black.opacity(0.35), radius: 2, y: 2)
                    .opacity(lifted ? 1 : 0)
            }
            .offset(y: lifted && !env.reducesMotion ? -1 : 0)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .pointerStyle(.link)
        .accessibilityLabel("\(source.project), \(TokenFormat.short(source.rate)) per minute, \(source.detail)")
        .accessibilityHint(source.threadID == nil ? "Opens T3 Code" : "Opens the thread in T3 Code")
    }
}
