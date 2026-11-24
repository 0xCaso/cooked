//! The clipboard: "Copy details" on the format notice (Swift `IslandModel.act`). Tests use a pasteboard of their
//! own (`unique`), never the user's.

use objc2::msg_send;
use objc2::rc::Retained;
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
use objc2_foundation::NSString;

pub struct Pasteboard {
    inner: Retained<NSPasteboard>,
    /// A private pasteboard, released when dropped.
    private: bool,
}

impl Pasteboard {
    /// The user's clipboard.
    pub fn general() -> Self {
        Self {
            inner: NSPasteboard::generalPasteboard(),
            private: false,
        }
    }

    /// A new pasteboard with a unique name, for tests.
    #[cfg(test)]
    pub fn unique() -> Self {
        Self {
            inner: NSPasteboard::pasteboardWithUniqueName(),
            private: true,
        }
    }

    /// Replaces the contents with `text`.
    pub fn copy(&self, text: &str) {
        self.inner.clearContents();
        // SAFETY: an extern static of AppKit's, a constant string.
        let kind = unsafe { NSPasteboardTypeString };
        self.inner
            .setString_forType(&NSString::from_str(text), kind);
    }

    #[cfg(test)]
    pub fn text(&self) -> Option<String> {
        // SAFETY: as in `copy`.
        let kind = unsafe { NSPasteboardTypeString };
        self.inner.stringForType(kind).map(|s| s.to_string())
    }
}

impl Drop for Pasteboard {
    fn drop(&mut self) {
        if self.private {
            // SAFETY: `-[NSPasteboard releaseGlobally]`, for a pasteboard only this process named.
            let _: () = unsafe { msg_send![&*self.inner, releaseGlobally] };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_replaces_the_text() {
        let board = Pasteboard::unique();
        board.copy("first");
        board.copy("Cooked could not read work: /x.jsonl");
        assert_eq!(
            board.text().as_deref(),
            Some("Cooked could not read work: /x.jsonl")
        );
    }
}
