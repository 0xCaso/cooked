import AppKit
import SwiftUI

/// Motion of the Setup and Settings windows.
enum SetupMotion {
    /// A step or tab enters: 200 ms, Emil Kowalski's strong ease-out.
    static let enter = Animation.timingCurve(0.23, 1, 0.32, 1, duration: 0.2)
    /// The confirmation sheet leaves faster than it came.
    static let leave = Animation.easeOut(duration: 0.15)
    static let slide: CGFloat = 12
    static let drop: CGFloat = 8

    /// A step enters 12 pt from the side it comes from (Back: from the left), fading in; the old one goes at once.
    /// Reduce Motion: the fade only.
    static func step(forward: Bool, reduceMotion: Bool) -> AnyTransition {
        let insertion: AnyTransition = reduceMotion ? .opacity
            : .offset(x: forward ? slide : -slide).combined(with: .opacity)
        return .asymmetric(insertion: insertion, removal: .identity)
    }

    /// Runs a change with the enter animation, or with none when Enter pressed the button or Reduce Motion is on
    /// and the change only moves things. Keyboard actions repeat; they must feel instant.
    @MainActor
    static func run(reduceMotion: Bool = false, _ change: () -> Void) {
        if NSApp.currentEvent?.type == .keyDown {
            var t = Transaction()
            t.disablesAnimations = true
            withTransaction(t, change)
        } else {
            withAnimation(enter, change)
        }
    }
}

/// The quiet text button (Skip, Remove Cooked's changes…): no fill, ink2, sinks on press.
struct GhostButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(TextStyle.body.font)
            .foregroundStyle(configuration.isPressed ? Palette.ink : Palette.ink2)
            .lineLimit(1)
            .fixedSize()
            .padding(.horizontal, 10)
            .frame(height: 28)
            .contentShape(Capsule())
            .pointingHand()
            .modifier(PressScale(pressed: configuration.isPressed))
    }
}

/// A 16 pt checkbox: a recessed square, white with a dark tick when on.
struct SetupCheckbox: View {
    @Binding var isOn: Bool
    var label: String

    var body: some View {
        Button { isOn.toggle() } label: {
            let shape = RoundedRectangle(cornerRadius: 5, style: .continuous)
            ZStack {
                if isOn {
                    shape.fill(Palette.ink).shadow(color: .black.opacity(0.4), radius: 1, y: 1)
                    Image(systemName: "checkmark").font(.system(size: 9, weight: .bold)).foregroundStyle(Color(white: 0.07))
                } else {
                    shape.fill(Color(white: 0.05).shadow(.inner(color: .black.opacity(0.6), radius: 1, y: 1)))
                    shape.inset(by: -0.25).stroke(.white.opacity(0.22), lineWidth: 0.5)
                }
            }
            .frame(width: 16, height: 16)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .pointingHand()
        .accessibilityLabel(label)
        .accessibilityValue(isOn ? "on" : "off")
        .accessibilityAddTraits(.isToggle)
    }
}

/// A 38 × 22 switch: the knob slides 16 pt in 200 ms ease-out.
struct SetupSwitch: View {
    @Binding var isOn: Bool
    var label: String
    @Environment(\.self) private var env

    var body: some View {
        Button { isOn.toggle() } label: {
            ZStack(alignment: .leading) {
                Capsule().fill(isOn ? Palette.ink : Color(white: 0.05))
                    .overlay { if !isOn { Capsule().inset(by: -0.25).stroke(.white.opacity(0.18), lineWidth: 0.5) } }
                Circle().fill(isOn ? Palette.bgBase : Color(white: 0.35))
                    .frame(width: 18, height: 18)
                    .padding(2)
                    .offset(x: isOn ? 16 : 0)
            }
            .frame(width: 38, height: 22)
            .animation(Motion.animation(.easeOut(duration: 0.2), reduceMotion: env.reducesMotion), value: isOn)
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .pointingHand()
        .accessibilityLabel(label)
        .accessibilityValue(isOn ? "on" : "off")
        .accessibilityAddTraits(.isToggle)
    }
}

/// The account's short name, in a recessed field. Snapshots draw it as text (AppKit fields do not render off screen).
struct NameField: View {
    @Binding var text: String
    var enabled: Bool
    var onCommit: () -> Void = {}
    @FocusState private var focused: Bool
    @Environment(\.solidSurfaces) private var still

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: 7, style: .continuous)
        Group {
            if still {
                Text(text).lineLimit(1).frame(maxWidth: .infinity, alignment: .leading)
            } else {
                TextField("Name", text: $text)
                    .textFieldStyle(.plain)
                    .focused($focused)
                    .onSubmit(onCommit)
                    .onChange(of: focused) { if !focused { onCommit() } }
            }
        }
        .font((enabled ? TextStyle.emphasis : TextStyle.body).font)
        .foregroundStyle(enabled ? Palette.ink : Palette.ink3)
        .padding(.horizontal, 8)
        .frame(height: 26)
        .background {
            shape.fill(Color(white: 0.05).shadow(.inner(color: .black.opacity(0.55), radius: 1, y: 1)))
            shape.inset(by: -0.25).stroke(.white.opacity(focused ? 0.6 : 0.12), lineWidth: focused ? 1 : 0.5)
        }
        .disabled(!enabled)
        .accessibilityLabel("Name")
    }
}

extension EnvironmentValues {
    /// Disclosures start open: for snapshots of what they hold.
    @Entry var disclosuresOpen = false
}

/// A closed-by-default disclosure: a chevron that turns 90° in 150 ms, and its body below.
struct SetupDisclosure<Content: View>: View {
    let title: String
    @State private var toggled = false
    @ViewBuilder var content: Content
    @Environment(\.self) private var env

    init(_ title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    private var open: Bool { toggled != env.disclosuresOpen }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button {
                withAnimation(Motion.animation(.easeOut(duration: 0.15), reduceMotion: env.reducesMotion)) { toggled.toggle() }
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "chevron.right").font(.system(size: 9, weight: .semibold))
                        .rotationEffect(.degrees(open ? 90 : 0))
                    Text(title).textStyle(.body)
                }
                .foregroundStyle(Palette.ink2)
                .padding(.vertical, 4)
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .pointingHand()
            .accessibilityValue(open ? "open" : "closed")
            if open {
                content.padding(.leading, 18).transition(.opacity)
            }
        }
    }
}

/// Short facts in a column, 12 pt ink2.
struct FactList: View {
    let facts: [Text]

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            ForEach(facts.indices, id: \.self) { facts[$0] }
        }
        .font(TextStyle.label.font)
        .foregroundStyle(Palette.ink2)
        .lineSpacing(2)
        .fixedSize(horizontal: false, vertical: true)
    }
}

/// Which accounts read each settings file: "the file" or "link", so one Connect covering several is no surprise.
struct SettingsFilesTree: View {
    let files: [SetupModel.SettingsFile]

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(files, id: \.real) { file in
                VStack(alignment: .leading, spacing: 0) {
                    if file.readers.count > 1 {
                        HStack(spacing: 12) {
                            Text("One file for \(file.readers.count) accounts").foregroundStyle(Palette.ink2)
                            Spacer(minLength: 0)
                            Text("connecting once covers all of them").foregroundStyle(Palette.ink3)
                        }
                        .textStyle(.label)
                        .lineLimit(1)
                        .padding(.bottom, 6)
                    }
                    ForEach(Array(file.readers.enumerated()), id: \.offset) { i, reader in
                        HStack(spacing: 10) {
                            Text(reader.name).foregroundStyle(Palette.ink).frame(width: 72, alignment: .leading)
                            Text(reader.path).font(TextStyle.labelNumber.font).foregroundStyle(Palette.ink3)
                                .truncationMode(.middle).frame(maxWidth: .infinity, alignment: .leading)
                            Label(reader.isLink ? "link" : "the file", systemImage: reader.isLink ? "link" : "doc")
                                .labelStyle(TightLabel()).foregroundStyle(Palette.ink3)
                        }
                        .font(TextStyle.label.font)
                        .lineLimit(1)
                        .frame(minHeight: 22)
                        .overlay(alignment: .top) { if i > 0 { Rectangle().fill(Palette.line).frame(height: 0.5) } }
                    }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .wellSurface(RoundedRectangle(cornerRadius: 8, style: .continuous))
            }
        }
    }
}

struct TightLabel: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 5) {
            configuration.icon.font(.system(size: 10))
            configuration.title
        }
    }
}
