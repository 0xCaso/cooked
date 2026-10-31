import Foundation

/// A colour with opacity, for text drawn over a surface.
struct RGBA: Equatable {
    let rgb: RGB
    let alpha: Double

    init(_ rgb: RGB, alpha: Double = 1) { (self.rgb, self.alpha) = (rgb, alpha) }
}

/// WCAG 2.x contrast: the text composited over its background (sRGB), then the ratio of relative luminances.
enum Contrast {
    static func ratio(_ fg: RGBA, over bg: RGB) -> Double {
        let a = fg.alpha
        let shown = RGB(r: fg.rgb.r * a + bg.r * (1 - a), g: fg.rgb.g * a + bg.g * (1 - a), b: fg.rgb.b * a + bg.b * (1 - a))
        let l1 = luminance(shown), l2 = luminance(bg)
        return (max(l1, l2) + 0.05) / (min(l1, l2) + 0.05)
    }

    static func luminance(_ c: RGB) -> Double {
        func channel(_ v: Double) -> Double { v <= 0.04045 ? v / 12.92 : pow((v + 0.055) / 1.055, 2.4) }
        return 0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
    }
}
