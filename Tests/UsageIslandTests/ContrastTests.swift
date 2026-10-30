import Testing
@testable import UsageIsland

struct ContrastTests {
    let ink = Palette.inkRGB

    @Test func knownRatios() {
        #expect(abs(Contrast.ratio(RGBA(RGB(r: 1, g: 1, b: 1)), over: RGB(r: 0, g: 0, b: 0)) - 21) < 0.001)
        #expect(abs(Contrast.ratio(RGBA(ink, alpha: 0.55), over: RGB(hex: 0x0B0B0C)) - 5.7) < 0.1)
        #expect(abs(Contrast.ratio(RGBA(ink, alpha: 0.72), over: RGB(hex: 0x0B0B0C)) - 9.2) < 0.1)
    }

    @Test(arguments: [0x161616, 0x080808, 0x1B1B1B] as [UInt32])
    func opaqueSurfaces(bg: UInt32) {
        #expect(Contrast.ratio(RGBA(ink, alpha: Palette.ink2Opacity), over: RGB(hex: bg)) >= 4.5)
        #expect(Contrast.ratio(RGBA(ink, alpha: Palette.ink3Opacity), over: RGB(hex: bg)) >= 4.5)
    }

    @Test func redOnCard() {
        #expect(Contrast.ratio(RGBA(Palette.redRGB), over: Palette.bgBaseRGB) >= 4.5)
    }

    /// The lightest glass measured on screen behind each text, with every style over a white window.
    /// If a veil changes, measure again and update these.
    @Test func textOnTheVeiledGlass() {
        // calm island and popover labels, 72% ink: Clear's popover over white
        #expect(Contrast.ratio(RGBA(ink, alpha: Palette.ink2Opacity), over: RGB(hex: 0x494949)) >= 4.5)
        // red 32 pt on the peak card: Clear with the popover open over white
        #expect(Contrast.ratio(RGBA(Palette.redRGB), over: RGB(hex: 0x383838)) >= 3)
        // red 13 pt of the minimized peak, in its well (the well's light bottom)
        #expect(Contrast.ratio(RGBA(Palette.redRGB), over: RGB(hex: 0x1B1B1B)) >= 4.5)
        // the reason for that well: the darkest glass measured, #2d2d2d, is not enough for red 13 pt
        #expect(Contrast.ratio(RGBA(Palette.redRGB), over: RGB(hex: 0x2D2D2D)) < 4.5)
    }

    @Test func veilsAreDarkerWhereRedSits() {
        for style in GlassStyle.allCases { #expect(style.veil(redHero: true) >= style.veil(redHero: false)) }
    }
}
