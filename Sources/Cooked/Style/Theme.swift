import CoreText
import SwiftUI

/// An sRGB color as 0...1 components. Shared by the palette and the contrast check.
struct RGB: Equatable {
    let r, g, b: Double

    init(r: Double, g: Double, b: Double) { (self.r, self.g, self.b) = (r, g, b) }

    init(hex: UInt32) {
        r = Double((hex >> 16) & 0xFF) / 255
        g = Double((hex >> 8) & 0xFF) / 255
        b = Double(hex & 0xFF) / 255
    }

    var color: Color { Color(.sRGB, red: r, green: g, blue: b) }
}

/// Colors from design/DESIGN.md: warm white ink at three levels, one hairline, the urgency red, hex's dark surfaces.
enum Palette {
    static let inkRGB = RGB(hex: 0xF4F1EA)
    static let redRGB = RGB(hex: 0xFF453A)
    static let bgDeepRGB = RGB(hex: 0x080808)
    static let bgBaseRGB = RGB(hex: 0x161616)
    static let ink2Opacity = 0.72
    static let ink3Opacity = 0.55
    static let lineOpacity = 0.08

    static let ink = inkRGB.color
    static let ink2 = ink.opacity(ink2Opacity)
    static let ink3 = ink.opacity(ink3Opacity)
    static let line = Color.white.opacity(lineOpacity)
    static let red = redRGB.color
    static let bgDeep = bgDeepRGB.color
    static let bgBase = bgBaseRGB.color
}

/// IBM Plex, the official static files (SIL OFL) in Resources/Fonts. These are their PostScript names.
enum PlexFont {
    static let sansRegular = "IBMPlexSans"
    static let sansMedium = "IBMPlexSans-Medm"
    static let monoRegular = "IBMPlexMono"
    static let monoMedium = "IBMPlexMono-Medm"

    /// For `swift run` and tests. The app bundle registers the fonts itself (ATSApplicationFontsPath).
    static func registerBundled(in dir: URL) {
        let files = (try? FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil)) ?? []
        for file in files where file.pathExtension == "woff" {
            CTFontManagerRegisterFontsForURL(file as CFURL, .process, nil)
        }
    }
}

/// The type scale: 12 label, 13 body, 32 hero. Two weights only, 400 and 500. Numbers in Plex Mono.
enum TextStyle {
    case label, labelNumber, body, emphasis, number, numberEmphasis, hero

    var size: CGFloat {
        switch self {
        case .label, .labelNumber: 12
        case .body, .emphasis, .number, .numberEmphasis: 13
        case .hero: 32
        }
    }

    var fontName: String {
        switch self {
        case .label, .body: PlexFont.sansRegular
        case .emphasis: PlexFont.sansMedium
        case .labelNumber, .number: PlexFont.monoRegular
        case .numberEmphasis, .hero: PlexFont.monoMedium
        }
    }

    /// −0.03 em on the hero number.
    var tracking: CGFloat { self == .hero ? -0.03 * size : 0 }

    var lineHeight: CGFloat {
        switch self {
        case .label, .labelNumber: 16
        case .hero: 32
        default: 18
        }
    }

    var font: Font { .custom(fontName, fixedSize: size) }
}

extension View {
    /// Font, tracking and the line box of the style (one line: the hero's 32 pt box is shorter than its glyphs, as in CSS).
    func textStyle(_ style: TextStyle) -> some View {
        font(style.font).tracking(style.tracking).monospacedDigit().frame(height: style.lineHeight)
    }
}

extension Text {
    /// The style on a run of text, for lines that mix Sans and Mono. No line box: set it on the whole line.
    func styled(_ style: TextStyle) -> Text {
        font(style.font).tracking(style.tracking).monospacedDigit()
    }
}
