import CoreText
import Foundation
import SwiftUI
import Testing
@testable import Cooked

struct ThemeTests {
    static let fontsDir = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        .appendingPathComponent("Resources/Fonts")

    @Test func fontFilesAreTheFourStaticFaces() throws {
        let files = try FileManager.default.contentsOfDirectory(at: Self.fontsDir, includingPropertiesForKeys: nil)
            .filter { ["ttf", "otf", "woff", "woff2"].contains($0.pathExtension) }
        var names: Set<String> = []
        for file in files {
            let descriptors = CTFontManagerCreateFontDescriptorsFromURL(file as CFURL) as? [CTFontDescriptor] ?? []
            #expect(descriptors.count == 1, "\(file.lastPathComponent) must hold one face")
            for d in descriptors {
                names.insert(CTFontDescriptorCopyAttribute(d, kCTFontNameAttribute) as? String ?? "?")
            }
        }
        #expect(files.count == 4)
        #expect(names == [PlexFont.sansRegular, PlexFont.sansMedium, PlexFont.monoRegular, PlexFont.monoMedium])
    }

    @Test func registeredFontsResolveByName() {
        PlexFont.registerBundled(in: Self.fontsDir)
        for name in [PlexFont.sansRegular, PlexFont.sansMedium, PlexFont.monoRegular, PlexFont.monoMedium] {
            let font = CTFontCreateWithName(name as CFString, 13, nil)
            #expect(CTFontCopyPostScriptName(font) as String == name)
        }
    }

    @Test func textStyleSizes() {
        #expect(TextStyle.label.size == 12)
        #expect(TextStyle.body.size == 13)
        #expect(TextStyle.emphasis.size == 13)
        #expect(TextStyle.number.size == 13)
        #expect(TextStyle.numberEmphasis.size == 13)
        #expect(TextStyle.hero.size == 32)
        #expect(TextStyle.labelNumber.size == 12)
        #expect(TextStyle.labelNumber.lineHeight == 16)
        #expect(abs(TextStyle.hero.tracking - -0.96) < 0.001)
        #expect(TextStyle.body.tracking == 0)
    }

    @Test func textStyleFaces() {
        #expect(TextStyle.label.fontName == PlexFont.sansRegular)
        #expect(TextStyle.body.fontName == PlexFont.sansRegular)
        #expect(TextStyle.emphasis.fontName == PlexFont.sansMedium)
        #expect(TextStyle.number.fontName == PlexFont.monoRegular)
        #expect(TextStyle.numberEmphasis.fontName == PlexFont.monoMedium)
        #expect(TextStyle.hero.fontName == PlexFont.monoMedium)
        #expect(TextStyle.labelNumber.fontName == PlexFont.monoRegular)
    }

    @Test func paletteValues() {
        #expect(Palette.redRGB == RGB(hex: 0xFF453A))
        #expect(Palette.inkRGB == RGB(hex: 0xF4F1EA))
        #expect(Palette.bgDeepRGB == RGB(hex: 0x080808))
        #expect(Palette.bgBaseRGB == RGB(hex: 0x161616))
        #expect(Palette.ink2Opacity == 0.72)
        #expect(Palette.ink3Opacity == 0.55)
        #expect(Palette.lineOpacity == 0.08)
    }
}
