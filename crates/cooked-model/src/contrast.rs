//! WCAG 2.x contrast. Text with opacity is composited over its background (sRGB), then the ratio of relative luminances is taken.

/// An sRGB colour as 0...1 components.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgb {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

impl Rgb {
    pub const fn new(r: f64, g: f64, b: f64) -> Self {
        Self { r, g, b }
    }

    /// `0xRRGGBB`.
    pub const fn hex(hex: u32) -> Self {
        Self {
            r: ((hex >> 16) & 0xFF) as f64 / 255.0,
            g: ((hex >> 8) & 0xFF) as f64 / 255.0,
            b: (hex & 0xFF) as f64 / 255.0,
        }
    }
}

/// A colour with opacity, for text drawn over a surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub rgb: Rgb,
    pub alpha: f64,
}

impl Rgba {
    pub const fn new(rgb: Rgb, alpha: f64) -> Self {
        Self { rgb, alpha }
    }

    pub const fn opaque(rgb: Rgb) -> Self {
        Self { rgb, alpha: 1.0 }
    }
}

/// WCAG relative luminance (`Contrast.luminance`).
pub fn relative_luminance(c: Rgb) -> f64 {
    fn channel(v: f64) -> f64 {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
}

/// WCAG contrast ratio of two opaque colours, 1...21, in either order.
pub fn contrast_ratio(a: Rgb, b: Rgb) -> f64 {
    let (l1, l2) = (relative_luminance(a), relative_luminance(b));
    (l1.max(l2) + 0.05) / (l1.min(l2) + 0.05)
}

/// `Contrast.ratio(_:over:)`: `fg` composited over `bg`, then the ratio against `bg`.
pub fn ratio(fg: Rgba, bg: Rgb) -> f64 {
    let a = fg.alpha;
    let mix = |f: f64, b: f64| f * a + b * (1.0 - a);
    let shown = Rgb::new(
        mix(fg.rgb.r, bg.r),
        mix(fg.rgb.g, bg.g),
        mix(fg.rgb.b, bg.b),
    );
    contrast_ratio(shown, bg)
}
