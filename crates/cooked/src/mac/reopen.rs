//! Opening the app again while it runs (Applications, Spotlight, the Dock, `open`): macOS sends the running app the
//! reopen Apple event (`'aevt'`/`'rapp'`), and Swift's `applicationShouldHandleReopen` opened the popover. Rune's
//! app delegate has no such hook, so Cooked handles the Apple event itself: AppKit's own handler only asked the
//! delegate, and Cooked never wants its default (no window to bring back).
//!
//! A later launch's arguments (`open --args --settings`) never reach the running app: LaunchServices sends only
//! the reopen event, in Swift too. Main thread only.

use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::{AnyObject, NSObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, class, define_class, msg_send, sel};

/// `'aevt'`: `kCoreEventClass`.
const CORE_EVENT_CLASS: u32 = u32::from_be_bytes(*b"aevt");
/// `'rapp'`: `kAEReopenApplication`.
const REOPEN: u32 = u32::from_be_bytes(*b"rapp");

pub struct ReopenIvars {
    on_reopen: Box<dyn Fn()>,
}

define_class!(
    /// The reopen event's handler object.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = ReopenIvars]
    struct ReopenHandler;

    impl ReopenHandler {
        #[unsafe(method(handleReopen:withReplyEvent:))]
        fn handle_reopen(&self, _event: Option<&AnyObject>, _reply: Option<&AnyObject>) {
            (self.ivars().on_reopen)();
        }
    }
);

fn manager() -> Option<Retained<AnyObject>> {
    // SAFETY: a class method returning the process's shared manager.
    unsafe { msg_send![class!(NSAppleEventManager), sharedAppleEventManager] }
}

/// The installed handler; dropping it gives the event back to AppKit.
pub struct Reopen {
    _handler: Retained<ReopenHandler>,
}

impl Reopen {
    /// Makes `on_reopen` the reopen event's handler. Call it once the app has finished launching (AppKit installs
    /// its own handler then). `None` off the main thread.
    pub fn install(on_reopen: impl Fn() + 'static) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let this = ReopenHandler::alloc(mtm).set_ivars(ReopenIvars {
            on_reopen: Box::new(on_reopen),
        });
        // SAFETY: NSObject's init.
        let handler: Retained<ReopenHandler> = unsafe { msg_send![super(this), init] };
        let manager = manager()?;
        // SAFETY: Foundation on the main thread; the manager does not retain the handler, `Reopen` keeps it.
        autoreleasepool(|_| unsafe {
            let _: () = msg_send![&*manager, setEventHandler: &*handler,
                andSelector: sel!(handleReopen:withReplyEvent:),
                forEventClass: CORE_EVENT_CLASS, andEventID: REOPEN];
        });
        Some(Self { _handler: handler })
    }
}

impl Drop for Reopen {
    fn drop(&mut self) {
        let Some(manager) = manager() else { return };
        // SAFETY: Foundation on the main thread (`Reopen` is made there and is not `Send`).
        autoreleasepool(|_| unsafe {
            let _: () = msg_send![&*manager, removeEventHandlerForEventClass: CORE_EVENT_CLASS,
                andEventID: REOPEN];
        });
    }
}
