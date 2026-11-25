//! The menu bar item of "Show Cooked in › Menu bar": an `NSStatusItem` whose button shows the label as an image,
//! the dot wave of the last 9 minutes and the 5-hour percent, drawn with CoreGraphics and CoreText. A left click
//! and a right click go to `on_click`; the menu pops up under the button.
//!
//! The image draws in AppKit's drawing handler, in the menu bar's appearance at that moment: light ink on a dark
//! menu bar, dark ink on a light one, drawn again by AppKit when the menu bar changes. A new label (data) sets a new
//! image; an equal one does nothing. Main thread only, except the drawing itself (`draw`, `render_rgba`).

use std::cell::RefCell;
use std::ptr::NonNull;
use std::rc::Rc;

use block2::RcBlock;
use cooked_model::island_placement::Rect;
use cooked_model::menu_bar::{COLS, ItemClick, MenuBarLabel};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, NSObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication, NSEventType,
    NSGraphicsContext, NSImage, NSMenu, NSMenuItem, NSStatusBar, NSStatusBarButton, NSStatusItem,
};
use objc2_core_foundation::{CFData, CFRetained, CGAffineTransform, CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGContext;
use objc2_core_text::{CTFont, CTFontManagerCreateFontDescriptorFromData, CTFontOrientation};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize, NSString};
use rune::{PopupEntry, PopupMenu};

use crate::menu_bar::MenuHost;
use crate::theme::{PlexFont, TextStyle};

/// The label's geometry, points: 6 pt in from each side and between wave and percent;
/// dots of radius 1.2 every 3.4 pt, 9 columns of 5.
const PAD: f64 = 6.0;
const SPACING: f64 = 6.0;
const PITCH: f64 = 3.4;
const DOT_RADIUS: f64 = 1.2;
const ROWS: usize = 5;
/// The image's height; the button centres it in the menu bar.
const HEIGHT: f64 = 22.0;
/// Off dots: the ink at 18%.
const OFF_ALPHA: f64 = 0.18;

/// `NSEventMask` for the button's action: left and right mouse up.
const MOUSE_UPS: u64 = (1 << 2) | (1 << 4);

/// The colours for one menu bar appearance, sRGB with alpha.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ink {
    /// `.primary`: AppKit's label colour.
    pub primary: [f64; 4],
    /// Red text on a light menu bar needs a darker red to reach 4.5:1.
    pub red: [f64; 4],
}

impl Ink {
    pub fn for_bar(dark: bool) -> Self {
        if dark {
            Self {
                primary: [1.0, 1.0, 1.0, 0.85],
                red: [1.0, 69.0 / 255.0, 58.0 / 255.0, 1.0],
            }
        } else {
            Self {
                primary: [0.0, 0.0, 0.0, 0.85],
                red: [215.0 / 255.0, 0.0, 21.0 / 255.0, 1.0],
            }
        }
    }
}

thread_local! {
    /// Plex Mono Medium at the number-emphasis size, made once per thread from the bundled font.
    static FONT: Option<CFRetained<CTFont>> = mono_font();
}

fn mono_font() -> Option<CFRetained<CTFont>> {
    let style = TextStyle::NumberEmphasis;
    let data = PlexFont::FILES
        .iter()
        .find(|f| font_has_name(f, style.font_name()))?;
    let data = CFData::from_static_bytes(data);
    // SAFETY: CoreText reads a font from bytes it copies; the descriptor and font are new CF objects.
    unsafe {
        let descriptor = CTFontManagerCreateFontDescriptorFromData(&data)?;
        Some(CTFont::with_font_descriptor(
            &descriptor,
            style.size() as f64,
            std::ptr::null(),
        ))
    }
}

/// The face's file: its PostScript name is in its name table as plain bytes.
fn font_has_name(file: &[u8], name: &str) -> bool {
    file.windows(name.len()).any(|w| w == name.as_bytes())
}

/// The percent's glyphs and their advances.
fn glyphs(font: &CTFont, text: &str) -> (Vec<u16>, Vec<CGSize>) {
    let chars: Vec<u16> = text.encode_utf16().collect();
    let mut glyphs = vec![0u16; chars.len()];
    let mut advances = vec![CGSize::new(0.0, 0.0); chars.len()];
    if chars.is_empty() {
        return (glyphs, advances);
    }
    // SAFETY: both buffers hold `chars.len()` elements.
    unsafe {
        font.glyphs_for_characters(
            NonNull::new_unchecked(chars.as_ptr().cast_mut()),
            NonNull::new_unchecked(glyphs.as_mut_ptr()),
            chars.len() as isize,
        );
        font.advances_for_glyphs(
            CTFontOrientation::Horizontal,
            NonNull::new_unchecked(glyphs.as_mut_ptr()),
            advances.as_mut_ptr(),
            glyphs.len() as isize,
        );
    }
    (glyphs, advances)
}

fn text_width(text: &str) -> f64 {
    FONT.with(|font| {
        font.as_ref()
            .map_or(0.0, |f| glyphs(f, text).1.iter().map(|a| a.width).sum())
    })
}

/// The label's size, points: as wide as its text.
pub fn label_size(label: &MenuBarLabel) -> (f64, f64) {
    let wave = COLS as f64 * PITCH;
    (
        PAD + wave + SPACING + text_width(&label.percent) + PAD,
        HEIGHT,
    )
}

fn fill(ctx: &CGContext, [r, g, b, a]: [f64; 4]) {
    CGContext::set_rgb_fill_color(Some(ctx), r, g, b, a);
}

/// Draws `label` in a context whose origin is the image's bottom-left, `height` points tall.
pub fn draw(ctx: &CGContext, label: &MenuBarLabel, ink: Ink, height: f64) {
    let lit = if label.urgent { ink.red } else { ink.primary };
    let mut off = ink.primary;
    off[3] *= OFF_ALPHA;
    let mid = height / 2.0;
    for (c, h) in label.heights.iter().enumerate() {
        let x = PAD + (c as f64 + 0.5) * PITCH;
        for k in -2i32..=2 {
            let on = k.unsigned_abs() <= u32::from(*h);
            fill(ctx, if on { lit } else { off });
            let y = mid + f64::from(k) * PITCH;
            let r = DOT_RADIUS;
            CGContext::fill_ellipse_in_rect(
                Some(ctx),
                CGRect::new(CGPoint::new(x - r, y - r), CGSize::new(2.0 * r, 2.0 * r)),
            );
        }
    }
    debug_assert_eq!(ROWS, 5);
    FONT.with(|font| {
        let Some(font) = font.as_ref() else { return };
        let (glyphs, advances) = glyphs(font, &label.percent);
        if glyphs.is_empty() {
            return;
        }
        // SAFETY: CoreText metrics on a live font.
        let (ascent, descent) = unsafe { (font.ascent(), font.descent()) };
        let baseline = (height - (ascent + descent)) / 2.0 + descent;
        let mut x = PAD + COLS as f64 * PITCH + SPACING;
        let positions: Vec<CGPoint> = advances
            .iter()
            .map(|a| {
                let p = CGPoint::new(x, baseline);
                x += a.width;
                p
            })
            .collect();
        fill(ctx, if label.urgent { ink.red } else { ink.primary });
        CGContext::set_text_matrix(
            Some(ctx),
            CGAffineTransform {
                a: 1.0,
                b: 0.0,
                c: 0.0,
                d: 1.0,
                tx: 0.0,
                ty: 0.0,
            },
        );
        // SAFETY: `glyphs` and `positions` hold the same count; the context is live.
        unsafe {
            font.draw_glyphs(
                NonNull::new_unchecked(glyphs.as_ptr().cast_mut()),
                NonNull::new_unchecked(positions.as_ptr().cast_mut()),
                glyphs.len(),
                ctx,
            );
        }
    });
}

/// The label as an image AppKit draws again in each appearance.
fn image(label: &MenuBarLabel) -> Retained<NSImage> {
    let (w, h) = label_size(label);
    let label = label.clone();
    let handler = RcBlock::new(move |_: NSRect| -> Bool {
        let Some(context) = NSGraphicsContext::currentContext() else {
            return Bool::NO;
        };
        draw(
            &context.CGContext(),
            &label,
            Ink::for_bar(drawing_dark()),
            h,
        );
        Bool::YES
    });
    NSImage::imageWithSize_flipped_drawingHandler(NSSize::new(w, h), false, &handler)
}

/// The appearance AppKit is drawing in now is a dark one.
fn drawing_dark() -> bool {
    let appearance = NSAppearance::currentDrawingAppearance();
    // SAFETY: the appearance names are AppKit statics.
    let names = unsafe { NSArray::from_slice(&[NSAppearanceNameAqua, NSAppearanceNameDarkAqua]) };
    appearance
        .bestMatchFromAppearancesWithNames(&names)
        .is_some_and(|n| &*n == unsafe { NSAppearanceNameDarkAqua })
}

/// What a click on the button calls, with the button to pop the menu up under.
pub type OnClick = Box<dyn Fn(ItemClick, &dyn MenuHost)>;

pub struct ClickIvars {
    on_click: OnClick,
    button: RefCell<Option<Retained<NSStatusBarButton>>>,
}

/// The item's button as the place its menu pops up.
struct ItemButton<'a> {
    button: &'a NSStatusBarButton,
    mtm: MainThreadMarker,
}

impl MenuHost for ItemButton<'_> {
    /// Under the button; returns once the menu has closed.
    fn pop_up(&self, popup: &PopupMenu) -> Option<String> {
        let native = NativeMenu::new(self.mtm, popup);
        let height = self.button.bounds().size.height;
        native.menu.popUpMenuPositioningItem_atLocation_inView(
            None,
            NSPoint::new(0.0, height + 4.0),
            Some(self.button),
        );
        native.take_picked()
    }
}

define_class!(
    /// The button's target: `clicked:` tells the left click from the right one.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = ClickIvars]
    struct ClickTarget;

    impl ClickTarget {
        #[unsafe(method(clicked:))]
        fn clicked(&self, _sender: Option<&AnyObject>) {
            let mtm = self.mtm();
            let app = NSApplication::sharedApplication(mtm);
            let right = app
                .currentEvent()
                .is_some_and(|e| e.r#type() == NSEventType::RightMouseUp);
            let click = if right { ItemClick::Secondary } else { ItemClick::Primary };
            let Some(button) = self.ivars().button.borrow().clone() else { return };
            (self.ivars().on_click)(click, &ItemButton { button: &button, mtm });
        }
    }
);

impl ClickTarget {
    fn new(mtm: MainThreadMarker, on_click: OnClick) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ClickIvars {
            on_click,
            button: RefCell::new(None),
        });
        // SAFETY: NSObject's init.
        unsafe { msg_send![super(this), init] }
    }
}

pub struct MenuIvars {
    id: String,
    picked: Rc<RefCell<Option<String>>>,
}

define_class!(
    /// A menu item's target: notes the item's id. The pick runs once the menu has closed (`StatusItem::pop_up`
    /// returns it); `fire:` is a selector of this class, so the runtime knows it (c45b3db: a selector it did not
    /// know made every item do nothing).
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = MenuIvars]
    struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(fire:))]
        fn fire(&self, _sender: Option<&AnyObject>) {
            let ivars = self.ivars();
            *ivars.picked.borrow_mut() = Some(ivars.id.clone());
        }
    }
);

impl MenuTarget {
    fn new(mtm: MainThreadMarker, id: &str, picked: Rc<RefCell<Option<String>>>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(MenuIvars {
            id: id.to_owned(),
            picked,
        });
        // SAFETY: NSObject's init.
        unsafe { msg_send![super(this), init] }
    }
}

/// An `NSMenu` built from a Rune `PopupMenu`, and the targets its items need alive.
pub struct NativeMenu {
    pub menu: Retained<NSMenu>,
    /// Alive as long as the menu: an item's target is not retained by it.
    _targets: Vec<Retained<MenuTarget>>,
    picked: Rc<RefCell<Option<String>>>,
}

impl NativeMenu {
    pub fn new(mtm: MainThreadMarker, popup: &PopupMenu) -> Self {
        let picked = Rc::new(RefCell::new(None));
        let mut targets = Vec::new();
        let menu = build(mtm, popup, &picked, &mut targets);
        Self {
            menu,
            _targets: targets,
            picked,
        }
    }

    /// The id of the item picked, once.
    pub fn take_picked(&self) -> Option<String> {
        self.picked.borrow_mut().take()
    }

    #[cfg(test)]
    fn target_count(&self) -> usize {
        self._targets.len()
    }
}

fn build(
    mtm: MainThreadMarker,
    popup: &PopupMenu,
    picked: &Rc<RefCell<Option<String>>>,
    targets: &mut Vec<Retained<MenuTarget>>,
) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    // items keep the enabled state given here
    menu.setAutoenablesItems(false);
    for entry in popup.entries() {
        match entry {
            PopupEntry::Item(item) => {
                let title = NSString::from_str(item.title());
                let empty = NSString::from_str("");
                // SAFETY: a new menu item with the target's `fire:`.
                let native = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &title,
                        Some(sel!(fire:)),
                        &empty,
                    )
                };
                let target = MenuTarget::new(mtm, item.id(), picked.clone());
                // SAFETY: the target lives in `targets` as long as the menu.
                unsafe { native.setTarget(Some(&target)) };
                native.setEnabled(item.is_enabled());
                native.setState(if item.is_checked() { 1 } else { 0 });
                targets.push(target);
                menu.addItem(&native);
            }
            PopupEntry::Separator => menu.addItem(&NSMenuItem::separatorItem(mtm)),
            PopupEntry::Submenu {
                title,
                menu: sub,
                enabled,
            } => {
                let title = NSString::from_str(title);
                let empty = NSString::from_str("");
                // SAFETY: a new menu item with no action; its submenu holds the choices.
                let parent = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &title,
                        None,
                        &empty,
                    )
                };
                let submenu = build(mtm, sub, picked, targets);
                submenu.setTitle(&title);
                parent.setSubmenu(Some(&submenu));
                parent.setEnabled(*enabled);
                menu.addItem(&parent);
            }
            _ => {}
        }
    }
    menu
}

/// The status item: removed from the menu bar when dropped.
pub struct StatusItem {
    item: Retained<NSStatusItem>,
    _target: Retained<ClickTarget>,
    label: Option<MenuBarLabel>,
    mtm: MainThreadMarker,
}

impl StatusItem {
    /// Adds the item to the menu bar. `None` off the main thread.
    pub fn new(on_click: OnClick) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(-1.0);
        let target = ClickTarget::new(mtm, on_click);
        if let Some(button) = item.button(mtm) {
            *target.ivars().button.borrow_mut() = Some(button.clone());
            // SAFETY: the target lives as long as the item (`_target`); `clicked:` is its method.
            unsafe {
                button.setTarget(Some(&target));
                button.setAction(Some(sel!(clicked:)));
                let _: isize = msg_send![&*button, sendActionOn: MOUSE_UPS];
                let label = NSString::from_str("Cooked");
                let _: () = msg_send![&*button, setAccessibilityLabel: &*label];
                // NSImageOnly
                let _: () = msg_send![&*button, setImagePosition: 1isize];
            }
        }
        Some(Self {
            item,
            _target: target,
            label: None,
            mtm,
        })
    }

    /// Shows `label`; an equal label keeps the image it has.
    pub fn set_label(&mut self, label: &MenuBarLabel) {
        if self.label.as_ref() == Some(label) {
            return;
        }
        self.label = Some(label.clone());
        let image = image(label);
        self.item.setLength(image.size().width);
        if let Some(button) = self.item.button(self.mtm) {
            button.setImage(Some(&image));
        }
    }

    /// The item's frame on screen (AppKit coordinates). `None` when its window is on no screen: macOS hid it, a full
    /// menu bar parks it at (0, -height).
    pub fn frame(&self) -> Option<Rect> {
        let window = self.item.button(self.mtm)?.window()?;
        window.screen()?;
        let f = window.frame();
        Some(Rect::new(
            f.origin.x,
            f.origin.y,
            f.size.width,
            f.size.height,
        ))
    }
}

impl Drop for StatusItem {
    fn drop(&mut self) {
        NSStatusBar::systemStatusBar().removeStatusItem(&self.item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cooked_model::demo_data;
    use objc2_core_graphics::{CGBitmapContextCreate, CGColorSpace, CGImageAlphaInfo};
    use rune::PopupItem;
    use std::ffi::c_void;

    /// The label on an opaque `background`, `scale` pixels per point: RGBA rows from the top, for look checks and tests.
    fn render_rgba(
        label: &MenuBarLabel,
        dark: bool,
        scale: f64,
        background: [f64; 4],
    ) -> (u32, u32, Vec<u8>) {
        let (w, h) = label_size(label);
        let (pw, ph) = ((w * scale).ceil() as usize, (h * scale).ceil() as usize);
        let mut pixels = vec![0u8; pw * ph * 4];
        let space =
            CGColorSpace::with_name(Some(unsafe { objc2_core_graphics::kCGColorSpaceSRGB }));
        // SAFETY: `pixels` holds `ph` rows of `pw * 4` bytes and outlives the context.
        let ctx = unsafe {
            CGBitmapContextCreate(
                pixels.as_mut_ptr().cast::<c_void>(),
                pw,
                ph,
                8,
                pw * 4,
                space.as_deref(),
                CGImageAlphaInfo::PremultipliedLast.0,
            )
        };
        if let Some(ctx) = ctx {
            fill(&ctx, background);
            CGContext::fill_rect(
                Some(&ctx),
                CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(pw as f64, ph as f64)),
            );
            CGContext::scale_ctm(Some(&ctx), scale, scale);
            draw(&ctx, label, Ink::for_bar(dark), h);
        }
        (pw as u32, ph as u32, pixels)
    }

    fn calm() -> MenuBarLabel {
        MenuBarLabel::of(&demo_data::calm())
    }

    #[test]
    fn plex_mono_medium_is_found_and_measures_the_percent() {
        assert!(FONT.with(|f| f.is_some()));
        // monospaced: "73%" is three equal advances
        let w = text_width("73%");
        assert!((w - 3.0 * text_width("7")).abs() < 0.01, "{w}");
        let (lw, lh) = label_size(&calm());
        assert_eq!(lh, HEIGHT);
        assert!(
            lw > PAD + COLS as f64 * PITCH + SPACING + PAD + 20.0,
            "{lw}"
        );
    }

    /// Pixels of `rgba` close to `c` (8-bit).
    fn count(rgba: &[u8], c: [u8; 3]) -> usize {
        rgba.chunks(4)
            .filter(|p| (0..3).all(|i| p[i].abs_diff(c[i]) <= 24))
            .count()
    }

    #[test]
    fn calm_label_is_ink_and_peak_label_is_red_on_both_bars() {
        let peak = MenuBarLabel::of(&demo_data::peak());
        for dark in [false, true] {
            let bg = if dark {
                [0.16, 0.16, 0.16, 1.0]
            } else {
                [0.92, 0.92, 0.92, 1.0]
            };
            let ink = Ink::for_bar(dark);
            let red = ink.red.map(|v| (v * 255.0).round() as u8);
            let (_, _, calm_px) = render_rgba(&calm(), dark, 2.0, bg);
            let (_, _, peak_px) = render_rgba(&peak, dark, 2.0, bg);
            let ink_px = if dark { [217, 217, 217] } else { [35, 35, 35] };
            assert!(count(&calm_px, ink_px) > 50, "dark {dark}: calm ink");
            assert_eq!(
                count(&calm_px, [red[0], red[1], red[2]]),
                0,
                "dark {dark}: calm has no red"
            );
            assert!(
                count(&peak_px, [red[0], red[1], red[2]]) > 50,
                "dark {dark}: peak red"
            );
        }
    }

    /// The label PNGs for look checks: `COOKED_LABEL_PNG_DIR=<dir> cargo test ... label_pngs`.
    #[test]
    fn label_pngs() {
        let Some(dir) = std::env::var_os("COOKED_LABEL_PNG_DIR") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).expect("png dir");
        let labels = [
            ("calm", calm()),
            ("peak", MenuBarLabel::of(&demo_data::peak())),
            (
                "empty",
                MenuBarLabel::of(&cooked_model::usage_snapshot::UsageSnapshot::empty("12:00")),
            ),
        ];
        for (name, label) in &labels {
            for (bar, dark, bg) in [
                ("light", false, [0.92, 0.92, 0.92, 1.0]),
                ("dark", true, [0.16, 0.16, 0.16, 1.0]),
            ] {
                let (w, h, px) = render_rgba(label, dark, 2.0, bg);
                let file = std::fs::File::create(dir.join(format!("menubar-{name}-{bar}.png")))
                    .expect("png file");
                let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
                enc.set_color(png::ColorType::Rgba);
                enc.set_depth(png::BitDepth::Eight);
                enc.write_header()
                    .and_then(|mut w| w.write_image_data(&px))
                    .expect("png");
            }
        }
    }

    /// Regression c45b3db: a menu item whose target is `MenuTarget` runs `fire:` (once "unrecognized selector",
    /// which AppKit swallowed, so every item did nothing).
    #[test]
    fn menu_items_run_their_action() {
        // SAFETY: an NSMenu off screen, never shown, used on this thread only.
        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let popup = PopupMenu::new()
            .item(PopupItem::new("data", "Data").enabled(false))
            .separator()
            .submenu(
                "Show in",
                PopupMenu::new().item(PopupItem::new("show:island", "Island").checked(true)),
            )
            .item(PopupItem::new("quit", "Quit Cooked"));
        // menu actions go through NSApp
        let _ = NSApplication::sharedApplication(mtm);
        let native = NativeMenu::new(mtm, &popup);
        assert_eq!(native.target_count(), 3);
        let menu = &native.menu;
        assert_eq!(menu.numberOfItems(), 4);
        // SAFETY: indices inside the menu.
        unsafe {
            let quit = menu.itemAtIndex(3).expect("quit item");
            let target = quit.target().expect("target");
            let answers: Bool = msg_send![&*target, respondsToSelector: sel!(fire:)];
            assert!(answers.as_bool());
            assert!(!menu.itemAtIndex(0).expect("data").isEnabled());
            let sub = menu
                .itemAtIndex(2)
                .and_then(|i| i.submenu())
                .expect("submenu");
            assert_eq!(sub.itemAtIndex(0).expect("island").state(), 1);
        }
        assert_eq!(native.take_picked(), None);
        menu.performActionForItemAtIndex(3);
        assert_eq!(native.take_picked().as_deref(), Some("quit"));
        assert_eq!(native.take_picked(), None);
    }
}
