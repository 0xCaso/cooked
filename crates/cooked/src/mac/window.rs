//! The island panel's AppKit side: the displays (`NSScreen`), the pointer, Reduce Transparency, and the Clear
//! glass style on Rune's `NSGlassEffectView` (step 0, spike 1c). Main thread only.

use cooked_model::island_placement::{Point, Rect, ScreenInfo};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{msg_send, sel};
use objc2_app_kit::{NSApplication, NSEvent, NSScreen, NSView, NSWorkspace};
use objc2_foundation::{NSRect, NSString};

/// The displays as AppKit sees them now.
#[derive(Debug, Clone, PartialEq)]
pub struct Displays {
    /// `NSScreen.screens`, the primary (menu bar) display first.
    pub screens: Vec<ScreenInfo>,
    /// `NSScreen.main`, else the first.
    pub main: ScreenInfo,
    /// The primary display's full height: Rune places overlays from its top-left, AppKit from its bottom-left.
    pub primary_height: f64,
}

impl Displays {
    /// `None` off the main thread or with no display.
    pub fn current() -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let all = NSScreen::screens(mtm);
        let primary_height = all.iter().next()?.frame().size.height;
        let screens: Vec<ScreenInfo> = all.iter().map(|s| info(&s)).collect();
        let main = NSScreen::mainScreen(mtm).map_or(screens[0], |s| info(&s));
        Some(Self {
            screens,
            main,
            primary_height,
        })
    }
}

fn rect(r: NSRect) -> Rect {
    Rect::new(r.origin.x, r.origin.y, r.size.width, r.size.height)
}

fn info(screen: &NSScreen) -> ScreenInfo {
    let desc = screen.deviceDescription();
    let key = NSString::from_str("NSScreenNumber");
    // SAFETY: `deviceDescription` is an NSDictionary; its "NSScreenNumber" is an NSNumber (or missing: nil).
    let id = unsafe {
        let number: *mut AnyObject = msg_send![&*desc, objectForKey: &*key];
        if number.is_null() {
            0
        } else {
            msg_send![number, unsignedIntValue]
        }
    };
    ScreenInfo {
        id,
        visible_frame: rect(screen.visibleFrame()),
    }
}

/// The frame of this app's window `number` (AppKit screen coordinates), where the window really is.
pub fn window_frame(number: u32) -> Option<Rect> {
    let mtm = MainThreadMarker::new()?;
    let app = NSApplication::sharedApplication(mtm);
    // SAFETY: `windowWithWindowNumber:` returns this app's window with that number, or nil.
    let window: Option<Retained<objc2_app_kit::NSWindow>> =
        unsafe { msg_send![&*app, windowWithWindowNumber: number as isize] };
    window.map(|w| rect(w.frame()))
}

/// The pointer in AppKit screen coordinates (`NSEvent.mouseLocation`).
pub fn mouse_location() -> Point {
    let p = NSEvent::mouseLocation();
    Point::new(p.x, p.y)
}

/// System Settings › Accessibility › Display › Reduce transparency.
pub fn reduce_transparency() -> bool {
    let workspace = NSWorkspace::sharedWorkspace();
    // SAFETY: `accessibilityDisplayShouldReduceTransparency` is a BOOL property of NSWorkspace (macOS 10.10+).
    let reduced: objc2::runtime::Bool =
        unsafe { msg_send![&*workspace, accessibilityDisplayShouldReduceTransparency] };
    reduced.as_bool()
}

/// Sets `style` (0 regular, 1 clear) on every Liquid Glass view Rune made in the window `number`, and returns how
/// many it changed. Rune's views sit at `contentView > RuneMaterialHost > NSGlassEffectView`; the walk stops at a
/// glass view and never enters its private subviews (an Objective-C exception there aborts the process).
pub fn set_glass_style(number: u32, style: isize) -> usize {
    let Some(mtm) = MainThreadMarker::new() else {
        return 0;
    };
    let app = NSApplication::sharedApplication(mtm);
    // SAFETY: `windowWithWindowNumber:` returns this app's window with that number, or nil.
    let window: Option<Retained<objc2_app_kit::NSWindow>> =
        unsafe { msg_send![&*app, windowWithWindowNumber: number as isize] };
    let Some(content) = window.and_then(|w| w.contentView()) else {
        return 0;
    };
    walk(&content, style, 0)
}

const MAX_DEPTH: usize = 3;

fn walk(view: &NSView, style: isize, depth: usize) -> usize {
    // AppKit may swap in a KVO subclass (`NSKVONotifying_NSGlassEffectView`): ask for the kind, not the name.
    let Some(glass) = AnyClass::get(c"NSGlassEffectView") else {
        return 0;
    };
    // SAFETY: `isKindOfClass:` on a live NSView, main thread.
    let is_glass: objc2::runtime::Bool = unsafe { msg_send![view, isKindOfClass: glass] };
    if is_glass.as_bool() {
        // SAFETY: NSGlassEffectView (macOS 26) answers `style` and `setStyle:` with an NSInteger.
        unsafe {
            let answers: objc2::runtime::Bool =
                msg_send![view, respondsToSelector: sel!(setStyle:)];
            if !answers.as_bool() {
                return 0;
            }
            let now: isize = msg_send![view, style];
            if now == style {
                return 0;
            }
            let _: () = msg_send![view, setStyle: style];
        }
        return 1;
    }
    if depth >= MAX_DEPTH {
        return 0;
    }
    view.subviews()
        .iter()
        .map(|sub| walk(&sub, style, depth + 1))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_the_main_thread_nothing_happens() {
        // cargo runs tests on worker threads: no displays, no windows, no crash
        assert_eq!(Displays::current(), None);
        assert_eq!(set_glass_style(1, 1), 0);
        assert_eq!(window_frame(1), None);
    }
}
