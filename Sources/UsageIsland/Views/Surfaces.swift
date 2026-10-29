import SwiftUI

extension EnvironmentValues {
    /// Opaque surfaces instead of glass: for snapshots (glass does not render off screen). Reduce Transparency does the same.
    @Entry var solidSurfaces: Bool = false
    @Entry var glassStyle: GlassStyle = .darker
}

// Surfaces from the mockup's look.html (hex's dark tokens). CSS shadow blur b → SwiftUI radius b / 2.

extension View {
    /// A raised card on the popover: #161616 with a 2% white sheen, two soft shadows, a 0.5 white ring, light on the top edge.
    func cardSurface() -> some View {
        let shape = RoundedRectangle(cornerRadius: 16, style: .continuous)
        return background {
            ZStack {
                shape.fill(.white.opacity(0.06)).offset(y: -0.5)
                shape.fill(Palette.bgBase)
                    .shadow(color: .black.opacity(0.3), radius: 2, y: 2)
                    .shadow(color: .black.opacity(0.3), radius: 1, y: 1)
                shape.fill(LinearGradient(colors: [.white.opacity(0.02), .white.opacity(0)], startPoint: .top, endPoint: .bottom))
                shape.inset(by: -0.25).stroke(.white.opacity(0.16), lineWidth: 0.5)
            }
        }
    }

    /// A well recessed into the surface, lit from above so its top edge falls into shadow (hex key_tray).
    func wellSurface<S: InsettableShape>(_ shape: S, innerBlur: CGFloat = 2.5) -> some View {
        background {
            ZStack {
                shape.fill(LinearGradient(colors: [Palette.bgDeep, Color(red: 27 / 255, green: 27 / 255, blue: 27 / 255)],
                                          startPoint: .top, endPoint: .bottom)
                    .shadow(.inner(color: .black.opacity(0.6), radius: innerBlur / 2, y: 1)))
                shape.inset(by: -0.25).stroke(.white.opacity(0.07), lineWidth: 0.5)
            }
        }
    }
}

/// The island's and popover's material: Liquid Glass, or the opaque "marcata" surface with Reduce Transparency.
struct IslandSurface<S: InsettableShape>: ViewModifier {
    let shape: S
    /// Opaque fill: #161616 for the island, #080808 for the popover.
    let solidFill: Color
    var urgent = false
    var glassID: String
    var namespace: Namespace.ID
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.solidSurfaces) private var solidSurfaces
    @Environment(\.glassStyle) private var glassStyle

    func body(content: Content) -> some View {
        let ring = urgent ? Palette.red.opacity(0.55) : .white.opacity(solid ? 0.16 : 0.14)
        let showsRing = urgent || solid || glassStyle.drawsRim
        if solid {
            content
                .background {
                    shape.fill(solidFill.opacity(0.98))
                        .shadow(color: .black.opacity(0.3), radius: 16, y: 16)
                        .shadow(color: .black.opacity(0.3), radius: 8, y: 8)
                }
                .overlay(shape.inset(by: -0.25).stroke(ring, lineWidth: 0.5))
        } else {
            content
                .background { if glassStyle.veil > 0 { shape.fill(Palette.bgBase.opacity(glassStyle.veil)) } }
                .glassEffect(glassStyle.glass, in: shape)
                .glassEffectID(glassID, in: namespace)
                .overlay { if showsRing { shape.inset(by: -0.25).stroke(ring, lineWidth: 0.5) } }
        }
    }

    private var solid: Bool { reduceTransparency || solidSurfaces }
}

/// The glass look. System and Clear are macOS's own Liquid Glass, so they follow System Settings
/// (Appearance, Reduce Transparency). Darker adds the mockup's dark tint, for legibility over light windows.
enum GlassStyle: String, CaseIterable {
    case system, clear, darker

    static let defaultsKey = "glassStyle"

    var title: String {
        switch self {
        case .system: "System"
        case .clear: "Clear"
        case .darker: "Darker"
        }
    }

    var glass: Glass {
        switch self {
        case .system: .regular
        case .clear: .clear
        case .darker: .regular.tint(Color(white: 16 / 255).opacity(0.52))
        }
    }

    /// A fixed dark layer over the glass. Liquid Glass sets its own brightness from what is under and around it:
    /// alone over a white window the island turned mid-grey, next to the open popover it stayed dark. The veil
    /// keeps both near the same dark grey.
    var veil: Double { self == .darker ? 0.8 : 0 }

    /// The mockup's thin white rim belongs to its tinted glass; the system glass draws its own edge.
    var drawsRim: Bool { self == .darker }
}

/// The quiet button next to the white one: 28 high, capsule, no fill, ink text and a thin ring.
struct SecondaryButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        SecondaryButtonBody(configuration: configuration)
    }

    private struct SecondaryButtonBody: View {
        let configuration: ButtonStyleConfiguration
        @Environment(\.isFocused) private var isFocused

        var body: some View {
            configuration.label
                .font(TextStyle.emphasis.font)
                .foregroundStyle(Palette.ink)
                .lineLimit(1)
                .fixedSize()
                .padding(.horizontal, 12)
                .frame(height: 28)
                .background {
                    Capsule().fill(.white.opacity(configuration.isPressed ? 0.12 : 0.06))
                    Capsule().inset(by: 0.25).stroke(.white.opacity(0.16), lineWidth: 0.5)
                }
                .overlay {
                    if isFocused { Capsule().inset(by: -3).stroke(Palette.ink2, lineWidth: 2) }
                }
                .contentShape(Capsule())
                .pointerStyle(.link)
                .modifier(PressScale(pressed: configuration.isPressed))
        }
    }
}

/// The white button: 28 high, capsule, white → ink, dark text, a crisp ring and a lit top edge.
struct PrimaryButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        PrimaryButtonBody(configuration: configuration)
    }

    private struct PrimaryButtonBody: View {
        let configuration: ButtonStyleConfiguration
        @Environment(\.isFocused) private var isFocused

        var body: some View {
            configuration.label
                .font(TextStyle.emphasis.font)
                .foregroundStyle(Color(red: 17 / 255, green: 17 / 255, blue: 17 / 255))
                .lineLimit(1)
                .fixedSize()
                .padding(.horizontal, 12)
                .frame(height: 28)
                .background {
                    ZStack {
                        Capsule().fill(.white.opacity(0.3)).offset(y: -0.5)
                        Capsule().fill(LinearGradient(colors: [.white, Palette.ink], startPoint: .top, endPoint: .bottom))
                            .shadow(color: .black.opacity(0.4), radius: 1, y: 1)
                        Capsule().inset(by: -0.25).stroke(.white.opacity(0.4), lineWidth: 0.5)
                    }
                }
                .overlay {
                    if isFocused { Capsule().inset(by: -3).stroke(Palette.ink2, lineWidth: 2) }
                }
                .contentShape(Capsule())
                .pointerStyle(.link)
                .modifier(PressScale(pressed: configuration.isPressed))
        }
    }
}
