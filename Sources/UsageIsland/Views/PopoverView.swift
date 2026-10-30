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
        // The top label's text starts 16 pt below the edge: the last card ends as far from the bottom edge.
        .padding(EdgeInsets(top: 8, leading: 8, bottom: 16, trailing: 8))
        .frame(width: IslandLayout.popoverWidth)
        .modifier(IslandSurface(shape: RoundedRectangle(cornerRadius: 24, style: .continuous), solidFill: Palette.bgDeep,
                                glassID: "popover", namespace: namespace))
    }

    /// Only the section label: the two windows are named over their columns inside the card.
    private var accountsHeader: some View {
        Text("Usage")
            .lineLimit(1)
            .textStyle(.label)
            .foregroundStyle(sectionInk)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// The week pill's width; the 5-hour pill, with the account name, takes the rest of the card's 364 pt.
    static let weekColumn: CGFloat = 128
    static let rowSpacing: CGFloat = 8

    private func rateCard(_ s: UsageSnapshot) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(TokenFormat.short(Double(model.chartRate))).textStyle(.hero)
                    .foregroundStyle(s.isUrgent ? Palette.red : Palette.ink)
                    .contentTransition(.numericText(value: Double(model.chartRate)))
                    .animation(Motion.animation(Motion.control, reduceMotion: env.reducesMotion), value: model.chartRate)
                Text("now").textStyle(.body).foregroundStyle(Palette.ink2)
                Spacer(minLength: 0)
                Text("\(Text("alert at "))\(Text(TokenFormat.short(Double(s.chartThreshold))).styled(.labelNumber))/min")
                    .styled(.label)
                    .frame(height: TextStyle.label.lineHeight)
                    .foregroundStyle(Palette.ink3)
            }
            .lineLimit(1)
            .padding(.bottom, 12)
            TideHistogramChart(source: histogram, range: model.chartRange)
                .padding(.top, 8)
                .padding(.bottom, 6)
                .frame(maxWidth: .infinity)
                .wellSurface(RoundedRectangle(cornerRadius: 8, style: .continuous))
                .padding(.horizontal, -4)
            HStack {
                RangeControl(selection: Bindable(model).chartRange)
                Spacer()
                Text("now").textStyle(.label).foregroundStyle(Palette.ink3)
            }
            .padding(.top, 6)
        }
        .padding(12)
        .cardSurface()
    }

    private func sourcesCard(_ sources: [TokenSource]) -> some View {
        SourcesCard(sources: sources, hovered: nil, hot: model.snapshot.isUrgent ? IslandView.peakSource(model.snapshot)?.id : nil)
    }

    private func accountsCard(_ s: UsageSnapshot) -> some View {
        VStack(spacing: 6) {
            HStack(spacing: Self.rowSpacing) {
                Text("5 hours").frame(maxWidth: .infinity, alignment: .leading)
                Text("week").frame(width: Self.weekColumn, alignment: .leading)
            }
            .padding(.leading, 9)
            .textStyle(.label)
            .foregroundStyle(Palette.ink3)
            .accessibilityHidden(true)
            ForEach(s.rows) { a in accountRow(a, urgent: s.isUrgent && a.id == s.activeID) }
        }
        .padding(.horizontal, 12)
        .padding(.top, 6)
        .padding(.bottom, 12)
        .cardSurface()
    }

    /// A pill per window, counted as what is left (like T3 Code). The 5-hour pill opens with the account name and turns
    /// red, "full HH:MM" before the reset, when the window fills before it resets; a blocked account is red up to its
    /// reset; the account that makes the island urgent is red too.
    private func accountRow(_ a: Account, urgent: Bool) -> some View {
        let fills = urgent || a.isBlocked || a.fullAt != nil
        return HStack(spacing: Self.rowSpacing) {
            UsagePill(name: a.short, left: a.fiveHourLeft,
                      timeLeft: a.fiveHourElapsed.map { 1 - $0 }, hot: fills, live: a.isLive) {
                PillTimes(full: a.isBlocked ? nil : a.fullAt, reset: a.fiveHourReset, red: a.isBlocked)
            }
            .frame(maxWidth: .infinity)
            UsagePill(left: a.weekPercent.map(Self.left), timeLeft: a.weekElapsed.map { 1 - $0 }, hot: false, live: false) {
                PillTimes(reset: a.weekReset ?? "")
            }
            .frame(width: Self.weekColumn)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(accessibilityText(a))
    }

    static func left(_ used: Int) -> Int { Account.left(used) }

    private func accessibilityText(_ a: Account) -> String {
        var parts = ["\(a.short): 5 hours \(Self.left(a.fiveHourPercent))% left"]
        if a.isBlocked { parts.append("blocked until \(a.fiveHourReset)") }
        else if let full = a.fullAt { parts.append("full at \(full), before the reset at \(a.fiveHourReset)") }
        else { parts.append("resets \(a.fiveHourReset)") }
        if let w = a.weekPercent { parts.append("week \(Self.left(w))% left" + (a.weekReset.map { ", resets \($0)" } ?? "")) }
        return parts.joined(separator: ", ")
    }
}

/// The sources, each a button. The hovered line lifts out of the card (mockup `.click .src:hover`): its own fill,
/// a shadow, 1 pt up, and the hairlines on both sides fade. The lines sit `gap` apart, with the hairline in the gap:
/// the lifted fill and its shadow stay in that gap and never cover the line above or below.
struct SourcesCard: View {
    /// Between two lines: the lifted line rises 1 pt into it and its shadow ends 3 pt below it, both inside the gap.
    static let gap: CGFloat = 4
    /// Lines shown until "N more" opens the rest.
    static let collapsed = 3

    let sources: [TokenSource]
    @State var hovered: String?
    /// The full list is open. Lives with the popover: closing it folds the list again.
    @State var expanded = false
    /// The source the peak card's "Open thread" opens: its rate in red.
    var hot: String? = nil
    @Environment(\.self) private var env

    private var shown: [TokenSource] { expanded ? sources : Array(sources.prefix(Self.collapsed)) }
    private var hidden: Int { sources.count - Self.collapsed }

    var body: some View {
        VStack(spacing: Self.gap) {
            ForEach(Array(shown.enumerated()), id: \.element.id) { i, source in
                let lifted = hovered == source.id
                let lineHidden = lifted || (i > 0 && hovered == shown[i - 1].id)
                SourceRow(source: source, lifted: lifted, hot: hot == source.id)
                    .onHover { inside in
                        if inside { hovered = source.id } else if hovered == source.id { hovered = nil }
                    }
                    .overlay(alignment: .top) {
                        if i > 0 { hairline.opacity(lineHidden ? 0 : 1) }
                    }
                    .zIndex(lifted ? 1 : 0)
                    .transition(.opacity.combined(with: .offset(y: env.reducesMotion ? 0 : -4)))
            }
            if hidden > 0 {
                MoreRow(title: expanded ? "Show less" : "\(hidden) more") { expanded.toggle() }
                    .overlay(alignment: .top) { hairline.opacity(hovered == shown.last?.id ? 0 : 1) }
            }
        }
        .padding(4)
        .animation(Motion.animation(.spring(duration: 0.2, bounce: 0.15), reduceMotion: env.reducesMotion), value: hovered)
        .animation(Motion.animation(.timingCurve(0.23, 1, 0.32, 1, duration: 0.22), reduceMotion: env.reducesMotion), value: expanded)
        .cardSurface()
    }

    private var hairline: some View {
        Rectangle().fill(Palette.line).frame(height: 0.5).offset(y: -(Self.gap + 0.5) / 2).padding(.horizontal, 8)
    }
}

/// "2 more" under the first lines, "Show less" once open: quieter than a source, brighter on hover.
private struct MoreRow: View {
    let title: String
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            Text(title).textStyle(.label).foregroundStyle(hovering ? Palette.ink : Palette.ink3)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 8)
                .padding(.vertical, 6)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .pointingHand()
        .onHover { hovering = $0 }
        .animation(.easeOut(duration: 0.15), value: hovering)
    }
}

/// One source: project and tokens/min, then the thread and its account. A click opens its T3 Code thread;
/// lifted (hovered), the account gives way to the hint and the agent count shows by the rate.
private struct SourceRow: View {
    let source: TokenSource
    let lifted: Bool
    var hot = false
    @Environment(\.self) private var env

    var body: some View {
        Button { T3Code.open(thread: source.threadID) } label: {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 8) {
                    Text(source.project).textStyle(.emphasis).foregroundStyle(Palette.ink)
                        .truncationMode(.middle)
                    Spacer(minLength: 0)
                    if lifted, let agents = source.agents {
                        Text(agents).textStyle(.body).foregroundStyle(Palette.ink3)
                    }
                    Text("\(Text(TokenFormat.short(source.rate)).styled(.number))\(Text("/min").styled(.number).foregroundStyle(hot ? Palette.red : Palette.ink3))")
                        .foregroundStyle(hot ? Palette.red : Palette.ink)
                        .layoutPriority(1)
                }
                HStack(spacing: 8) {
                    Text(source.detail).textStyle(.body).foregroundStyle(Palette.ink2)
                        .truncationMode(.tail)
                    Spacer(minLength: 0)
                    Text(lifted ? (source.threadID == nil ? "Open T3 Code ↗" : "Open in T3 Code ↗") : source.account)
                        .textStyle(.body)
                        .foregroundStyle(lifted ? Palette.ink : Palette.ink2)
                        .layoutPriority(1)
                }
            }
            .lineLimit(1)
            .padding(.horizontal, 8)
            // 6 + 6 and the 4 pt gap keep the 52 pt pitch of the old 7 + 7 lines
            .padding(.vertical, 6)
            .background {
                // Short shadows: the mockup's 6 pt drop fell over the next line's text and past the card.
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(Color(white: 0x20 / 255.0))
                    .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .strokeBorder(Color.white.opacity(0.22), lineWidth: 0.5))
                    .shadow(color: .black.opacity(0.45), radius: 2, y: 2)
                    .shadow(color: .black.opacity(0.35), radius: 1, y: 1)
                    .opacity(lifted ? 1 : 0)
            }
            .offset(y: lifted && !env.reducesMotion ? -1 : 0)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .pointingHand()
        .accessibilityLabel("\(source.project), \(TokenFormat.short(source.rate)) per minute, \(source.detail), \(source.account)")
        .accessibilityHint(source.threadID == nil ? "Opens T3 Code" : "Opens the thread in T3 Code")
    }
}
