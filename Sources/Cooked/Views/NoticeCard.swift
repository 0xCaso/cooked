import SwiftUI

/// The popover's notice: what is missing, in one sentence, and its one fix.
struct NoticeCard: View {
    let notice: Notice
    var act: (Notice) -> Void = { _ in }
    var dismiss: (Notice) -> Void = { _ in }
    /// "Copy details" says "Copied" for a moment: the click did something you cannot see.
    @State private var copied = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // both wrap: `textStyle` holds one line, and a sentence cut short says nothing
            Text(notice.title).font(TextStyle.emphasis.font).lineSpacing(3).foregroundStyle(Palette.ink)
                .fixedSize(horizontal: false, vertical: true)
            Text(notice.body).font(TextStyle.body.font).lineSpacing(3).foregroundStyle(Palette.ink2)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.top, 2)
            buttons.padding(.top, 10)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(EdgeInsets(top: 12, leading: 14, bottom: 12, trailing: 14))
        .cardSurface()
        .accessibilityElement(children: .contain)
    }

    @ViewBuilder private var buttons: some View {
        HStack(spacing: 8) {
            switch notice {
            case .noAccounts:
                Button("Set up…") { act(notice) }.buttonStyle(PrimaryButtonStyle())
            case .formatChanged:
                Button(copied ? "Copied" : "Copy details") {
                    act(notice)
                    copied = true
                    Task { try? await Task.sleep(for: .seconds(1.5)); copied = false }
                }
                .buttonStyle(SecondaryButtonStyle())
            case .paused:
                Button("Connect again") { act(notice) }.buttonStyle(PrimaryButtonStyle())
            case .noPercent:
                Button("Connect") { act(notice) }.buttonStyle(PrimaryButtonStyle())
                Button("Not now") { dismiss(notice) }.buttonStyle(SecondaryButtonStyle())
            case .apiKey:
                Button("OK") { act(notice) }.buttonStyle(SecondaryButtonStyle())
            }
        }
    }
}
