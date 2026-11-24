//! The Setup and Settings controls, as `Views/SetupControls.swift` draws them in the approved look:
//! 30 pt capsule buttons, a 16 pt checkbox, a 38 x 22
//! switch, the account's name field, a disclosure with a chevron that turns, and the motion of steps and the sheet.

use rune::{
    Color, Cursor, El, Icon, Shadow, Spring, SpringPair, TextEdit, Transition, col, el, icon, row,
    text, text_input,
};

use crate::theme::{Motion, Palette, Rgb, TextStyle};

/// A step or tab enters 12 pt from the side it comes from (Back: from the left), fading in, in 200 ms; the old one
/// goes at once. Reduce Motion: no motion.
pub const STEP_SLIDE: f32 = 12.0;
pub const STEP_SECONDS: f32 = 0.2;
/// The confirmation sheet drops 8 pt as it fades in, and leaves faster (150 ms).
pub const SHEET_DROP: f32 = 8.0;
pub const SHEET_OUT_SECONDS: f32 = 0.15;
/// The disclosure's chevron turns in 150 ms.
pub const CHEVRON_SECONDS: f32 = 0.15;

/// The dark ink on white controls (`#111`).
pub fn dark_ink() -> Color {
    Rgb::hex(0x111111).color()
}

/// A recessed control's face (`#0d0d0d`, an inner shadow at the top).
fn recessed(e: El, ring: f32) -> El {
    e.fill(Rgb::hex(0x0D0D0D).color())
        .box_shadow(Shadow {
            y: 1.0,
            blur: 2.0,
            color: Color::BLACK.alpha(0.6),
            inset: true,
            ..Shadow::default()
        })
        .border(0.5, Color::WHITE.alpha(ring))
}

pub fn step_transition(forward: bool, reduce_motion: bool) -> Transition {
    if reduce_motion {
        return Transition::none();
    }
    let x = if forward { STEP_SLIDE } else { -STEP_SLIDE };
    Transition::offset(x, 0.0)
        .combined(Transition::opacity())
        .springs(SpringPair::new(
            Spring::new(STEP_SECONDS, 0.0),
            Spring::NONE,
        ))
}

pub fn sheet_transition(reduce_motion: bool) -> Transition {
    if reduce_motion {
        return Transition::none();
    }
    Transition::offset(0.0, -SHEET_DROP)
        .combined(Transition::opacity())
        .springs(SpringPair::new(
            Spring::new(STEP_SECONDS, 0.0),
            Spring::new(SHEET_OUT_SECONDS, 0.0),
        ))
}

pub fn fade_transition(reduce_motion: bool) -> Transition {
    if reduce_motion {
        return Transition::none();
    }
    Transition::opacity().springs(SpringPair::new(
        Spring::new(STEP_SECONDS, 0.0),
        Spring::new(SHEET_OUT_SECONDS, 0.0),
    ))
}

// MARK: - Icons (the mockup's 16 x 16 strokes)

macro_rules! stroke_icon {
    ($name:ident, $id:literal, $width:literal, $paths:literal) => {
        pub static $name: Icon = Icon {
            name: $id,
            view: 16.0,
            body: concat!(
                r#"<g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width=""#,
                $width,
                r#"">"#,
                $paths,
                "</g>"
            ),
        };
    };
}

stroke_icon!(
    CHECK,
    "cooked-check",
    "2",
    r#"<path d="M4 8.4l2.6 2.6L12 5.4"/>"#
);
stroke_icon!(
    TICK,
    "cooked-tick",
    "1.5",
    r#"<path d="M3.5 8.5l3 3 6-7"/>"#
);
stroke_icon!(
    TICK_BOLD,
    "cooked-tick-bold",
    "2",
    r#"<path d="M3.5 8.5l3 3 6-7"/>"#
);
stroke_icon!(DASH, "cooked-dash", "2", r#"<path d="M4 8h8"/>"#);
stroke_icon!(
    WARN,
    "cooked-warn",
    "1.5",
    r#"<path d="M8 2.5l6 11H2z"/><path d="M8 6.5v3.2"/><path d="M8 11.6v.1"/>"#
);
stroke_icon!(
    LINK,
    "cooked-link",
    "1.5",
    r#"<path d="M6.5 9.5l3-3"/><path d="M7 4.5l1.2-1.2a2.6 2.6 0 013.7 3.7L10.7 8.2"/><path d="M9 11.5l-1.2 1.2a2.6 2.6 0 01-3.7-3.7L5.3 7.8"/>"#
);
stroke_icon!(
    FILE,
    "cooked-file",
    "1.5",
    r#"<path d="M4 2.5h5l3 3v8H4z"/><path d="M9 2.5v3h3"/>"#
);
stroke_icon!(
    PEOPLE,
    "cooked-people",
    "1.5",
    r#"<circle cx="6" cy="5.5" r="2.3"/><path d="M2 13c.4-2.3 2-3.6 4-3.6s3.6 1.3 4 3.6"/><path d="M10.5 3.4a2.2 2.2 0 010 4.2"/><path d="M12 9.6c1.2.5 1.9 1.7 2.1 3.4"/>"#
);
stroke_icon!(
    PERCENT,
    "cooked-percent",
    "1.5",
    r#"<path d="M12.5 3.5l-9 9"/><circle cx="4.5" cy="4.5" r="1.6"/><circle cx="11.5" cy="11.5" r="1.6"/>"#
);
stroke_icon!(
    POWER,
    "cooked-power",
    "1.5",
    r#"<path d="M8 2v5.5"/><path d="M4.6 4.2a5 5 0 106.8 0"/>"#
);
stroke_icon!(
    CHEVRON,
    "cooked-chevron",
    "1.5",
    r#"<path d="M6 3.5L10.5 8 6 12.5"/>"#
);

// MARK: - Buttons

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// White, dark text: the step's or the state's one action.
    Primary,
    /// `#222`, a quiet ring: Back, Cancel, Disconnect.
    Secondary,
    /// No fill, 72% text: Skip, "Remove Cooked's changes…".
    Ghost,
}

/// A 30 pt capsule (mockup `.btn`); press sinks to 0.97 (DESIGN.md).
pub fn button(key: &'static str, title: &str, kind: ButtonKind) -> El {
    let b = row()
        .key(key)
        .h(30.0)
        .center_y()
        .radius(15.0)
        .hit()
        .cursor(Cursor::Pointer)
        .press_scale(Motion::BUTTON_SCALE)
        .label(title.to_owned());
    let label = |style: TextStyle, color: Color| {
        text(title.to_owned())
            .key("label")
            .font(style.font())
            .color(color)
    };
    match kind {
        ButtonKind::Primary => b
            .px(14.0)
            .fill(Palette::ink())
            .box_shadow(Shadow {
                y: 1.0,
                blur: 2.0,
                color: Color::BLACK.alpha(0.4),
                ..Shadow::default()
            })
            .border(0.5, Color::WHITE.alpha(0.3))
            .child(label(TextStyle::Emphasis, dark_ink())),
        ButtonKind::Secondary => b
            .px(14.0)
            .fill(Rgb::hex(0x222222).color())
            .hover(Color::WHITE.alpha(0.04))
            .box_shadow(Shadow {
                y: 1.0,
                blur: 2.0,
                color: Color::BLACK.alpha(0.4),
                ..Shadow::default()
            })
            .border(0.5, Color::WHITE.alpha(0.16))
            .child(label(TextStyle::Emphasis, Palette::ink())),
        ButtonKind::Ghost => b
            .px(10.0)
            .hover(Color::WHITE.alpha(0.04))
            .child(label(TextStyle::Body, Palette::ink2())),
    }
}

// MARK: - Checkbox and switch

/// A 16 pt checkbox: recessed when off, white with a dark tick when on.
pub fn checkbox(key: impl Into<String>, on: bool, label: &str) -> El {
    let b = el()
        .key(key)
        .size(16.0, 16.0)
        .radius(5.0)
        .center()
        .hit()
        .cursor(Cursor::Pointer)
        .role("checkbox")
        .checked(on)
        .label(label.to_owned());
    if on {
        b.fill(Palette::ink())
            .box_shadow(Shadow {
                y: 1.0,
                blur: 2.0,
                color: Color::BLACK.alpha(0.4),
                ..Shadow::default()
            })
            .child(icon(&CHECK, 12.0).key("tick").color(dark_ink()))
    } else {
        recessed(b, 0.22)
    }
}

/// A 38 x 22 switch: the knob slides 16 pt in 200 ms.
pub fn switch(key: &'static str, on: bool, label: &str, reduce_motion: bool) -> El {
    let track = el()
        .key(key)
        .size(38.0, 22.0)
        .radius(11.0)
        .hit()
        .cursor(Cursor::Pointer)
        .role("switch")
        .checked(on)
        .label(label.to_owned())
        .items_start()
        .center_y();
    let knob = el()
        .key("knob")
        .size(18.0, 18.0)
        .radius(9.0)
        .ml(2.0)
        .fill(if on {
            Palette::bg_base()
        } else {
            Rgb::hex(0x5A5A5A).color()
        })
        .offset(if on { 16.0 } else { 0.0 }, 0.0);
    let knob = match Motion::animation(Motion::CONTROL, reduce_motion) {
        Some(curve) => knob.offset_motion(curve),
        None => knob,
    };
    let track = if on {
        track.fill(Palette::ink())
    } else {
        recessed(track, 0.18)
    };
    track.child(knob)
}

// MARK: - Name field

/// The account's short name in a recessed field (mockup `.fld`): 500 and full ink when ticked, 400 at 55% when not.
pub fn name_field(edit: &TextEdit, enabled: bool, configure: impl FnOnce(El) -> El) -> El {
    let style = if enabled {
        TextStyle::Emphasis
    } else {
        TextStyle::Body
    };
    let color = if enabled {
        Palette::ink()
    } else {
        Palette::ink3()
    };
    let field = text_input(edit, style.font())
        .key("input")
        .single_line()
        .w_full()
        .color(color)
        .caret(Palette::ink(), Palette::ink().alpha(0.25))
        .placeholder("Name", Palette::ink3())
        .label("Name");
    let field = configure(field.disabled(!enabled));
    recessed(
        row()
            .key("field")
            .w_full()
            .h(26.0)
            .px(8.0)
            .radius(7.0)
            .center_y()
            .focus_ring(1.0, 0.0, Palette::ink().alpha(0.6)),
        0.12,
    )
    .child(field)
}

// MARK: - Disclosure

/// A closed-by-default disclosure (mockup `.disc`): a chevron that turns 90° in 150 ms, the title at 72%, and the
/// body 18 pt in under it.
pub fn disclosure(key: &'static str, title: &str, open: bool, reduce_motion: bool) -> El {
    let chevron = icon(&CHEVRON, 12.0)
        .key("chevron")
        .color(Palette::ink2())
        .origin(0.5, 0.5)
        .rotate(if open {
            std::f32::consts::FRAC_PI_2
        } else {
            0.0
        });
    let chevron = match Motion::animation(
        rune::Curve::tween(CHEVRON_SECONDS, rune::Easing::EASE_OUT),
        reduce_motion,
    ) {
        Some(curve) => chevron.rotate_motion(curve),
        None => chevron,
    };
    row()
        .key(key)
        .gap(6.0)
        .py(4.0)
        .center_y()
        .self_start()
        .hit()
        .cursor(Cursor::Pointer)
        .role("button")
        .label(title.to_owned())
        .child(chevron)
        .child(
            text(title.to_owned())
                .key("title")
                .font(TextStyle::Body.font())
                .color(Palette::ink2()),
        )
}

/// The disclosure's body, under its title.
pub fn disclosure_body(key: &'static str, reduce_motion: bool) -> El {
    col()
        .key(key)
        .w_full()
        .pt(6.0)
        .pl(18.0)
        .transition(fade_transition(reduce_motion))
}

/// The segmented control's track (Swift `Segments`): recessed; its options are `segment`s.
pub fn segments(key: &'static str) -> El {
    row()
        .key(key)
        .gap(2.0)
        .padding(2.0)
        .radius(8.0)
        .fill(Rgb::hex(0x0D0D0D).color())
        .border(0.5, Color::WHITE.alpha(0.08))
}

/// One option of `segments`: the chosen one raised.
pub fn segment(key: &'static str, title: &str, on: bool) -> El {
    let option = row()
        .key(key)
        .h(24.0)
        .px(10.0)
        .radius(6.0)
        .center_y()
        .hit()
        .cursor(Cursor::Pointer)
        .role("radio")
        .checked(on)
        .label(title.to_owned())
        .child(
            text(title.to_owned())
                .key("label")
                .font(TextStyle::Label.font())
                .color(if on { Palette::ink() } else { Palette::ink3() }),
        );
    if on {
        option
            .fill(Rgb::hex(0x2A2A2A).color())
            .border(0.5, Color::WHITE.alpha(0.14))
    } else {
        option
    }
}

/// A 22 pt status glyph (mockup `.sglyph`): filled with a tick, a ring with a dash, or the warning triangle alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Connected,
    Off,
    Warn,
}

pub fn glyph(kind: Glyph) -> El {
    let b = el().key("glyph").size(22.0, 22.0).radius(11.0).center();
    match kind {
        Glyph::Connected => b
            .fill(Palette::ink())
            .child(icon(&TICK_BOLD, 13.0).color(dark_ink())),
        Glyph::Off => b
            .border(1.0, Palette::ink().alpha(0.35))
            .child(icon(&DASH, 13.0).color(Palette::ink2())),
        Glyph::Warn => b.child(icon(&WARN, 18.0).color(Palette::ink())),
    }
}

/// The 1 pt rule between the rail's steps.
pub fn rail_line() -> El {
    el().key("line")
        .h(1.0)
        .grow(1.0)
        .min_w(12.0)
        .fill(Palette::ink().alpha(0.14))
}

/// A hairline between rows and sections (mockup `--muted`).
pub fn line() -> Color {
    Color::WHITE.alpha(0.08)
}
