//! The look from `design/DESIGN.md`, as `Style/Theme.swift`, `Views/Surfaces.swift` and `Views/Motion.swift` set
//! it: warm white ink at three levels, the urgency red, hex's dark surfaces, IBM Plex at two weights, the glass
//! styles' veils, and the springs.
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the island uses part of the scale; the popover, Setup and Settings (Tasks 13-15) the rest"
    )
)]

use cooked_model::island_model::GlassStyle;
use rune::{Color, Curve, Easing, Family, Shadow, Spring, SpringPair, Transition, Vec2};

/// An sRGB color as 0...1 components. Shared by the palette and the contrast check.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgb {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

impl Rgb {
    pub const fn hex(hex: u32) -> Self {
        Self {
            r: ((hex >> 16) & 0xFF) as f64 / 255.0,
            g: ((hex >> 8) & 0xFF) as f64 / 255.0,
            b: (hex & 0xFF) as f64 / 255.0,
        }
    }

    /// Rune's color (linear light) for these sRGB components.
    pub fn color(self) -> Color {
        let byte = |v: f64| (v * 255.0).round() as u8;
        let hex = format!(
            "#{:02x}{:02x}{:02x}",
            byte(self.r),
            byte(self.g),
            byte(self.b)
        );
        rune::color::hex(&hex).unwrap_or(Color::WHITE)
    }
}

/// Colors from DESIGN.md: warm white ink at three levels, one hairline, the urgency red, hex's dark surfaces.
pub struct Palette;

impl Palette {
    pub const INK: Rgb = Rgb::hex(0xF4F1EA);
    pub const RED: Rgb = Rgb::hex(0xFF453A);
    pub const BG_DEEP: Rgb = Rgb::hex(0x080808);
    pub const BG_BASE: Rgb = Rgb::hex(0x161616);
    /// The bottom of a well's gradient.
    pub const WELL_BOTTOM: Rgb = Rgb::hex(0x1B1B1B);
    pub const INK2_OPACITY: f32 = 0.72;
    pub const INK3_OPACITY: f32 = 0.55;
    pub const LINE_OPACITY: f32 = 0.08;

    pub fn ink() -> Color {
        Self::INK.color()
    }
    pub fn ink2() -> Color {
        Self::ink().alpha(Self::INK2_OPACITY)
    }
    pub fn ink3() -> Color {
        Self::ink().alpha(Self::INK3_OPACITY)
    }
    pub fn red() -> Color {
        Self::RED.color()
    }
    pub fn bg_deep() -> Color {
        Self::BG_DEEP.color()
    }
    pub fn bg_base() -> Color {
        Self::BG_BASE.color()
    }
}

/// IBM Plex, the official static files (SIL OFL, `assets/fonts/OFL.txt`): TTFs converted losslessly from the
/// Swift app's WOFFs. These are their PostScript names; Rune finds them by their family names.
pub struct PlexFont;

impl PlexFont {
    pub const SANS_REGULAR: &'static str = "IBMPlexSans";
    pub const SANS_MEDIUM: &'static str = "IBMPlexSans-Medm";
    pub const MONO_REGULAR: &'static str = "IBMPlexMono";
    pub const MONO_MEDIUM: &'static str = "IBMPlexMono-Medm";
    pub const SANS_FAMILY: &'static str = "IBM Plex Sans";
    pub const MONO_FAMILY: &'static str = "IBM Plex Mono";

    /// Every face, for `AppSpec::fonts`: they replace Rune's built-in Inter.
    pub const FILES: &'static [&'static [u8]] = &[
        include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf"),
        include_bytes!("../assets/fonts/IBMPlexSans-Medium.ttf"),
        include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf"),
        include_bytes!("../assets/fonts/IBMPlexMono-Medium.ttf"),
    ];
    /// `AppSpec::families`: the sans and mono slots.
    pub const FAMILIES: (&'static str, &'static str) = (Self::SANS_FAMILY, Self::MONO_FAMILY);
}

/// The type scale: 12 label, 13 body, 32 hero. Two weights only, 400 and 500. Numbers in Plex Mono.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextStyle {
    Label,
    LabelNumber,
    Body,
    Emphasis,
    Number,
    NumberEmphasis,
    Hero,
}

impl TextStyle {
    pub fn size(self) -> f32 {
        match self {
            Self::Label | Self::LabelNumber => 12.0,
            Self::Body | Self::Emphasis | Self::Number | Self::NumberEmphasis => 13.0,
            Self::Hero => 32.0,
        }
    }

    /// The face's PostScript name.
    pub fn font_name(self) -> &'static str {
        match self {
            Self::Label | Self::Body => PlexFont::SANS_REGULAR,
            Self::Emphasis => PlexFont::SANS_MEDIUM,
            Self::LabelNumber | Self::Number => PlexFont::MONO_REGULAR,
            Self::NumberEmphasis | Self::Hero => PlexFont::MONO_MEDIUM,
        }
    }

    fn mono(self) -> bool {
        matches!(
            self,
            Self::LabelNumber | Self::Number | Self::NumberEmphasis | Self::Hero
        )
    }

    pub fn weight(self) -> f32 {
        match self {
            Self::Emphasis | Self::NumberEmphasis | Self::Hero => 500.0,
            _ => 400.0,
        }
    }

    /// −0.03 em on the hero number.
    pub fn tracking(self) -> f32 {
        if self == Self::Hero {
            -0.03 * self.size()
        } else {
            0.0
        }
    }

    pub fn line_height(self) -> f32 {
        match self {
            Self::Label | Self::LabelNumber => 16.0,
            Self::Hero => 32.0,
            _ => 18.0,
        }
    }

    /// Rune's style: the face by family and weight, tabular figures.
    pub fn font(self) -> rune::TextStyle {
        rune::TextStyle {
            family: if self.mono() {
                Family::Mono
            } else {
                Family::Sans
            },
            line_height: self.line_height(),
            tracking: self.tracking(),
            features: "'tnum' 1",
            ..rune::TextStyle::new(self.size(), self.weight())
        }
    }
}

/// How each glass style draws on top of Liquid Glass. System and Clear are macOS's own glass, so they follow
/// System Settings, under a dark veil that keeps the text legible; Darker adds the mockup's dark tint and rim.
pub trait GlassLook {
    /// The fixed dark layer (`#161616`) over the glass. Liquid Glass sets its own brightness from what is under
    /// it: alone over a white window, System went #c1c1c1 and Clear #d8d8d8, and white text fell to 1.2–1.6:1.
    /// Measured on the real screen over white (the worst case), the veil brings 72% ink to 4.5:1 or more. Red text
    /// never sits on the glass: the island's red percentage is in a well.
    fn veil(self) -> f32;
    /// Darker's tint under the veil (`#101010` at 52%, Swift's `.regular.tint`): Rune can't tint the glass view,
    /// so it is one more layer over it.
    fn tint(self) -> Option<(Rgb, f32)>;
    /// The mockup's thin white rim belongs to its tinted glass; the system glass draws its own edge.
    fn draws_rim(self) -> bool;
    /// `NSGlassEffectView.style`: 0 regular, 1 clear.
    fn native_style(self) -> isize;
}

impl GlassLook for GlassStyle {
    fn veil(self) -> f32 {
        match self {
            GlassStyle::System | GlassStyle::Darker => 0.8,
            GlassStyle::Clear => 0.85,
        }
    }

    fn tint(self) -> Option<(Rgb, f32)> {
        (self == GlassStyle::Darker).then_some((Rgb::hex(0x101010), 0.52))
    }

    fn draws_rim(self) -> bool {
        self == GlassStyle::Darker
    }

    fn native_style(self) -> isize {
        match self {
            GlassStyle::Clear => 1,
            GlassStyle::System | GlassStyle::Darker => 0,
        }
    }
}

/// A well recessed into the surface, lit from above so its top edge falls into shadow (hex `key_tray`):
/// `#080808` → `#1b1b1b`, an inner shadow at the top, a 7% hairline.
pub fn well(el: rune::El, radius: f32) -> rune::El {
    el.radius(radius)
        .fill(Palette::bg_deep())
        .gradient(Palette::bg_deep(), Palette::WELL_BOTTOM.color())
        .box_shadow(Shadow {
            y: 1.0,
            blur: 2.0,
            color: Color::BLACK.alpha(0.8),
            inset: true,
            ..Shadow::default()
        })
        .border(0.5, Color::WHITE.alpha(0.07))
}

/// A raised card on the popover (mockup B `.pop.tiles .card`): `#1a1b1d` → `#141416`, a short drop shadow, a 16%
/// hairline and a 6% light on the top edge.
pub fn card(el: rune::El, radius: f32) -> rune::El {
    el.radius(radius)
        .fill(Rgb::hex(0x1A1B1D).color())
        .gradient(Rgb::hex(0x1A1B1D).color(), Rgb::hex(0x141416).color())
        .box_shadow(Shadow {
            y: 1.0,
            blur: 2.0,
            color: Color::BLACK.alpha(0.5),
            ..Shadow::default()
        })
        .box_shadow(Shadow {
            y: 1.0,
            color: Color::WHITE.alpha(0.06),
            inset: true,
            ..Shadow::default()
        })
        .border(0.5, Color::WHITE.alpha(0.16))
}

/// hex's springs (`Spring::new(duration, bounce 0)`): no bounce, durations are settle times (mockup `look.html`).
pub struct Motion;

impl Motion {
    pub const PRESS_SECONDS: f32 = 0.08;
    pub const RELEASE_SECONDS: f32 = 0.18;
    pub const CONTROL_SECONDS: f32 = 0.2;
    pub const POPOVER_IN_SECONDS: f32 = 0.24;
    pub const POPOVER_OUT_SECONDS: f32 = 0.2;

    pub const BUTTON_SCALE: f32 = 0.97;
    pub const ISLAND_SCALE: f32 = 0.98;
    pub const POPOVER_FROM_SCALE: f32 = 0.96;

    /// Pressed: sinks fast.
    pub const PRESS: Curve = Curve::tween(Self::PRESS_SECONDS, Easing::EASE_OUT);
    /// Released: comes back on the spring.
    pub const RELEASE: Curve = Curve::Spring(Spring::new(Self::RELEASE_SECONDS, 0.0));
    pub const CONTROL: Curve = Curve::Spring(Spring::new(Self::CONTROL_SECONDS, 0.0));
    pub const POPOVER_IN: Curve = Curve::Spring(Spring::new(Self::POPOVER_IN_SECONDS, 0.0));
    pub const POPOVER_OUT: Curve = Curve::tween(Self::POPOVER_OUT_SECONDS, Easing::EASE_OUT);

    /// The popover's entrance and exit: from `scale(.96)` and transparent, about its top centre; out a little
    /// faster. Under Reduce Motion it shows and goes at once.
    pub fn popover(reduce_motion: bool) -> Transition {
        if reduce_motion {
            return Transition::none();
        }
        Transition::scale_anchor(Self::POPOVER_FROM_SCALE, Vec2::new(0.5, 0.0)).springs(
            SpringPair::new(
                Spring::new(Self::POPOVER_IN_SECONDS, 0.0),
                Spring::new(Self::POPOVER_OUT_SECONDS, 0.0),
            ),
        )
    }

    /// No animation under Reduce Motion (the system's, or `--reduce-motion`).
    pub fn animation(curve: Curve, reduce_motion: bool) -> Option<Curve> {
        (!reduce_motion).then_some(curve)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cooked_model::defaults::MemoryDefaults;
    use cooked_model::demo_data;
    use cooked_model::island_model::IslandState;

    // ---- ThemeTests ----

    /// The name table's PostScript name (id 6) of every face in a TTF; a collection would hold more than one.
    fn faces(bytes: &[u8]) -> Vec<String> {
        let u16_at = |o: usize| u16::from_be_bytes([bytes[o], bytes[o + 1]]) as usize;
        let u32_at = |o: usize| {
            u32::from_be_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]) as usize
        };
        if &bytes[0..4] == b"ttcf" {
            return vec!["collection".into(); u32_at(8)];
        }
        let tables = u16_at(4);
        let name = (0..tables)
            .map(|i| 12 + 16 * i)
            .find(|&r| &bytes[r..r + 4] == b"name")
            .map(|r| u32_at(r + 8))
            .expect("name table");
        let (count, strings) = (u16_at(name + 2), name + u16_at(name + 4));
        (0..count)
            .map(|i| name + 6 + 12 * i)
            .filter(|&r| u16_at(r) == 3 && u16_at(r + 6) == 6)
            .map(|r| {
                let (len, off) = (u16_at(r + 8), u16_at(r + 10));
                let utf16: Vec<u16> = bytes[strings + off..strings + off + len]
                    .chunks(2)
                    .map(|c| u16::from_be_bytes([c[0], c[1]]))
                    .collect();
                String::from_utf16_lossy(&utf16)
            })
            .take(1)
            .collect()
    }

    #[test]
    fn font_files_are_the_four_static_faces() {
        let mut names: Vec<String> = PlexFont::FILES
            .iter()
            .map(|file| {
                let faces = faces(file);
                assert_eq!(faces.len(), 1, "one face per file: {faces:?}");
                faces[0].clone()
            })
            .collect();
        names.sort();
        let mut want = [
            PlexFont::SANS_REGULAR,
            PlexFont::SANS_MEDIUM,
            PlexFont::MONO_REGULAR,
            PlexFont::MONO_MEDIUM,
        ];
        want.sort();
        assert_eq!(names, want);
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fonts");
        let files = std::fs::read_dir(dir)
            .expect("fonts dir")
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                [".ttf", ".otf", ".woff", ".woff2"]
                    .iter()
                    .any(|x| name.ends_with(x))
            })
            .count();
        assert_eq!(files, 4);
        assert!(std::path::Path::new(dir).join("OFL.txt").exists());
    }

    /// Swift checked the faces resolve by name in Core Text; here Rune's own text system must find them by family
    /// and weight: Plex Mono's advance is 600/1000 em, and Sans Medium is wider than Sans Regular.
    #[test]
    fn registered_fonts_resolve_by_family() {
        let _fonts = rune::text::with_fonts(PlexFont::FILES, PlexFont::FAMILIES);
        for style in [TextStyle::Number, TextStyle::NumberEmphasis] {
            let (w, _) = rune::text::measure("0000000000", &[], style.font(), None);
            assert!((w - 78.0).abs() < 0.1, "{style:?}: {w}");
        }
        let (regular, _) = rune::text::measure("personal", &[], TextStyle::Body.font(), None);
        let (medium, _) = rune::text::measure("personal", &[], TextStyle::Emphasis.font(), None);
        let (inter, _) = {
            let _builtin = rune::text::with_fonts(&[], ("Inter", "ui-monospace"));
            rune::text::measure("personal", &[], TextStyle::Body.font(), None)
        };
        assert!(medium > regular, "medium {medium} regular {regular}");
        assert!((regular - inter).abs() > 0.1, "Plex, not Inter: {regular}");
    }

    #[test]
    fn text_style_sizes() {
        assert_eq!(TextStyle::Label.size(), 12.0);
        assert_eq!(TextStyle::Body.size(), 13.0);
        assert_eq!(TextStyle::Emphasis.size(), 13.0);
        assert_eq!(TextStyle::Number.size(), 13.0);
        assert_eq!(TextStyle::NumberEmphasis.size(), 13.0);
        assert_eq!(TextStyle::Hero.size(), 32.0);
        assert_eq!(TextStyle::LabelNumber.size(), 12.0);
        assert_eq!(TextStyle::LabelNumber.line_height(), 16.0);
        assert!((TextStyle::Hero.tracking() - -0.96).abs() < 0.001);
        assert_eq!(TextStyle::Body.tracking(), 0.0);
    }

    #[test]
    fn text_style_faces() {
        assert_eq!(TextStyle::Label.font_name(), PlexFont::SANS_REGULAR);
        assert_eq!(TextStyle::Body.font_name(), PlexFont::SANS_REGULAR);
        assert_eq!(TextStyle::Emphasis.font_name(), PlexFont::SANS_MEDIUM);
        assert_eq!(TextStyle::Number.font_name(), PlexFont::MONO_REGULAR);
        assert_eq!(TextStyle::NumberEmphasis.font_name(), PlexFont::MONO_MEDIUM);
        assert_eq!(TextStyle::Hero.font_name(), PlexFont::MONO_MEDIUM);
        assert_eq!(TextStyle::LabelNumber.font_name(), PlexFont::MONO_REGULAR);
        // the Rune style picks the same face: family slot and weight
        for style in [
            TextStyle::Label,
            TextStyle::LabelNumber,
            TextStyle::Body,
            TextStyle::Emphasis,
            TextStyle::Number,
            TextStyle::NumberEmphasis,
            TextStyle::Hero,
        ] {
            let font = style.font();
            let mono = style.font_name().starts_with("IBMPlexMono");
            assert_eq!(font.family == Family::Mono, mono, "{style:?}");
            assert_eq!(
                font.weight == 500.0,
                style.font_name().ends_with("-Medm"),
                "{style:?}"
            );
        }
    }

    #[test]
    fn palette_values() {
        assert_eq!(Palette::RED, Rgb::hex(0xFF453A));
        assert_eq!(Palette::INK, Rgb::hex(0xF4F1EA));
        assert_eq!(Palette::BG_DEEP, Rgb::hex(0x080808));
        assert_eq!(Palette::BG_BASE, Rgb::hex(0x161616));
        assert_eq!(Palette::INK2_OPACITY, 0.72);
        assert_eq!(Palette::INK3_OPACITY, 0.55);
        assert_eq!(Palette::LINE_OPACITY, 0.08);
        assert_eq!(Palette::red().hex().to_lowercase(), "#ff453aff");
    }

    // ---- GlassStyleTests ----

    #[test]
    fn glass_default_is_darker() {
        let model = IslandState::new(demo_data::calm(), None, MemoryDefaults::default(), 0.0);
        assert_eq!(model.glass_style(), GlassStyle::Darker);
    }

    #[test]
    fn glass_choice_is_kept() {
        let mut model = IslandState::new(demo_data::calm(), None, MemoryDefaults::default(), 0.0);
        model.set_glass_style(GlassStyle::Clear);
        let model = IslandState::new(demo_data::calm(), None, model.into_defaults(), 0.0);
        assert_eq!(model.glass_style(), GlassStyle::Clear);
    }

    #[test]
    fn glass_menu_titles() {
        let titles: Vec<_> = GlassStyle::ALL.iter().map(|g| g.title()).collect();
        assert_eq!(titles, ["System", "Clear", "Darker"]);
    }

    #[test]
    fn only_darker_adds_the_mockup_rim() {
        let rims: Vec<_> = GlassStyle::ALL
            .into_iter()
            .filter(|g| g.draws_rim())
            .collect();
        assert_eq!(rims, [GlassStyle::Darker]);
    }

    #[test]
    fn veils_and_native_styles() {
        assert_eq!(GlassStyle::System.veil(), 0.8);
        assert_eq!(GlassStyle::Darker.veil(), 0.8);
        assert_eq!(GlassStyle::Clear.veil(), 0.85);
        assert_eq!(GlassStyle::Clear.native_style(), 1);
        assert_eq!(GlassStyle::System.native_style(), 0);
        assert_eq!(GlassStyle::Darker.native_style(), 0);
        assert!(GlassStyle::Darker.tint().is_some());
        assert!(GlassStyle::System.tint().is_none() && GlassStyle::Clear.tint().is_none());
    }

    // ---- MotionTests ----

    #[test]
    fn reduce_motion_disables_animations() {
        assert_eq!(Motion::animation(Motion::CONTROL, true), None);
        assert_eq!(Motion::animation(Motion::POPOVER_IN, true), None);
        assert_eq!(
            Motion::animation(Motion::CONTROL, false),
            Some(Motion::CONTROL)
        );
    }

    #[test]
    fn durations() {
        // hex's springs: settle times from the mockup's look.html
        assert_eq!(Motion::PRESS_SECONDS, 0.08);
        assert_eq!(Motion::RELEASE_SECONDS, 0.18);
        assert_eq!(Motion::CONTROL_SECONDS, 0.2);
        assert_eq!(Motion::POPOVER_IN_SECONDS, 0.24);
        assert_eq!(Motion::POPOVER_OUT_SECONDS, 0.2);
        assert_eq!(Motion::BUTTON_SCALE, 0.97);
        assert_eq!(Motion::ISLAND_SCALE, 0.98);
        assert_eq!(Motion::POPOVER_FROM_SCALE, 0.96);
        assert_eq!(Motion::PRESS.logical_duration(), 0.08);
        assert_eq!(Motion::RELEASE.logical_duration(), 0.18);
        assert_eq!(Motion::POPOVER_OUT.logical_duration(), 0.2);
    }
}
