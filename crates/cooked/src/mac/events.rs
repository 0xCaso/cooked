//! Clicks in other apps while the popover is open (`NSEvent.addGlobalMonitorForEvents`). Main thread only.

use objc2::rc::Retained;
use objc2::{ClassType, MainThreadMarker, msg_send};
use objc2_app_kit::NSEvent;
use objc2_foundation::NSObject;

/// `NSEventMask`: left, right and other mouse down.
const MOUSE_DOWNS: usize = (1 << 1) | (1 << 3) | (1 << 25);

/// A global monitor of mouse downs in other apps; removed when dropped. Clicks in this app's own windows never
/// reach it (AppKit sends only other apps' events to a global monitor).
pub struct OutsideClicks {
    token: Retained<NSObject>,
}

impl OutsideClicks {
    /// Calls `f` on the main thread at each mouse down in another app. `None` off the main thread.
    pub fn watch(f: impl Fn() + 'static) -> Option<Self> {
        MainThreadMarker::new()?;
        let handler = block2::RcBlock::new(move |_: *mut NSEvent| f());
        // SAFETY: main-thread AppKit call; AppKit copies the block, and the token it returns is removed once, in
        // `Drop`.
        let token: Option<Retained<NSObject>> = unsafe {
            msg_send![NSEvent::class(), addGlobalMonitorForEventsMatchingMask: MOUSE_DOWNS, handler: &*handler]
        };
        token.map(|token| Self { token })
    }
}

impl Drop for OutsideClicks {
    fn drop(&mut self) {
        // SAFETY: the token came from `addGlobalMonitorForEventsMatchingMask:handler:` and is removed exactly once.
        unsafe {
            let _: () = msg_send![NSEvent::class(), removeMonitor: &*self.token];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_the_main_thread_nothing_is_watched() {
        assert!(OutsideClicks::watch(|| {}).is_none());
    }
}
