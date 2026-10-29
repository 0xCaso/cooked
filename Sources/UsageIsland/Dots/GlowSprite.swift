import CoreGraphics
import SwiftUI

/// One lit dot with its glow, drawn once per size and colour and reused for every lit dot (the mockup's sprite).
@MainActor
enum GlowSprite {
    private static var cache: [String: CGImage] = [:]
    static let scale: CGFloat = 2

    static func cgImage(radius: CGFloat, pitch: CGFloat, hot: Bool) -> CGImage {
        let key = "\(radius)|\(pitch)|\(hot)"
        if let hit = cache[key] { return hit }
        let side = Int(ceil(pitch * 3) * scale)
        let space = CGColorSpace(name: CGColorSpace.sRGB)!
        let ctx = CGContext(data: nil, width: side, height: side, bitsPerComponent: 8, bytesPerRow: 0,
                            space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        let glow = hot ? CGColor(srgbRed: 1, green: 69 / 255, blue: 58 / 255, alpha: 0.55)
                       : CGColor(srgbRed: 1, green: 246 / 255, blue: 230 / 255, alpha: 0.45)
        let fill = hot ? Palette.redRGB : Palette.inkRGB
        ctx.setShadow(offset: .zero, blur: pitch * 1.1 * scale, color: glow)
        ctx.setFillColor(CGColor(srgbRed: fill.r, green: fill.g, blue: fill.b, alpha: 1))
        let r = radius * scale, c = CGFloat(side) / 2
        ctx.fillEllipse(in: CGRect(x: c - r, y: c - r, width: 2 * r, height: 2 * r))
        let image = ctx.makeImage()!
        cache[key] = image
        return image
    }

    static func image(radius: CGFloat, pitch: CGFloat, hot: Bool) -> Image {
        Image(decorative: cgImage(radius: radius, pitch: pitch, hot: hot), scale: scale)
    }
}
