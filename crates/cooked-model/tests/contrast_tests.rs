//! `ContrastTests.swift`. The palette values are copied from `Style/Theme.swift` (`Palette`), which is ported with the
//! island view.

use cooked_model::contrast::{Rgb, Rgba, contrast_ratio, ratio, relative_luminance};

const INK: Rgb = Rgb::hex(0xF4F1EA);
const RED: Rgb = Rgb::hex(0xFF453A);
const BG_BASE: Rgb = Rgb::hex(0x161616);
const INK2_OPACITY: f64 = 0.72;
const INK3_OPACITY: f64 = 0.55;

#[test]
fn known_ratios() {
    assert!(
        (ratio(
            Rgba::opaque(Rgb::new(1.0, 1.0, 1.0)),
            Rgb::new(0.0, 0.0, 0.0)
        ) - 21.0)
            .abs()
            < 0.001
    );
    assert!((ratio(Rgba::new(INK, 0.55), Rgb::hex(0x0B0B0C)) - 5.7).abs() < 0.1);
    assert!((ratio(Rgba::new(INK, 0.72), Rgb::hex(0x0B0B0C)) - 9.2).abs() < 0.1);
}

#[test]
fn opaque_surfaces() {
    for bg in [0x161616, 0x080808, 0x1B1B1B] {
        assert!(
            ratio(Rgba::new(INK, INK2_OPACITY), Rgb::hex(bg)) >= 4.5,
            "{bg:06x}"
        );
        assert!(
            ratio(Rgba::new(INK, INK3_OPACITY), Rgb::hex(bg)) >= 4.5,
            "{bg:06x}"
        );
    }
}

#[test]
fn red_on_card() {
    assert!(ratio(Rgba::opaque(RED), BG_BASE) >= 4.5);
}

/// The lightest glass measured on screen behind each text, with every style over a white window.
/// If a veil changes, measure again and update these.
#[test]
fn text_on_the_veiled_glass() {
    // calm island and popover labels, 72% ink: Clear's popover over white
    assert!(ratio(Rgba::new(INK, INK2_OPACITY), Rgb::hex(0x494949)) >= 4.5);
    // red 13 pt of the urgent island, in its well (the well's light bottom)
    assert!(ratio(Rgba::opaque(RED), Rgb::hex(0x1B1B1B)) >= 4.5);
    // the reason for that well: the darkest glass measured, #2d2d2d, is not enough for red 13 pt
    assert!(ratio(Rgba::opaque(RED), Rgb::hex(0x2D2D2D)) < 4.5);
}

#[test]
fn luminance_and_ratio_of_opaque_colours() {
    assert_eq!(relative_luminance(Rgb::new(0.0, 0.0, 0.0)), 0.0);
    assert!((relative_luminance(Rgb::new(1.0, 1.0, 1.0)) - 1.0).abs() < 1e-12);
    assert_eq!(
        contrast_ratio(Rgb::hex(0xFFFFFF), Rgb::hex(0x000000)),
        contrast_ratio(Rgb::hex(0x000000), Rgb::hex(0xFFFFFF))
    );
    assert_eq!(
        ratio(Rgba::opaque(RED), BG_BASE),
        contrast_ratio(RED, BG_BASE)
    );
}
