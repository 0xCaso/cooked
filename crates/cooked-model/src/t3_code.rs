//! T3 Code, the app the sources run in: the link half; opening it is the app's `mac::open`.
//! `t3code://threads/<environment>/<thread>` opens one thread (T3 Code 0.0.46 and later, pingdotgg/t3code#18454);
//! older builds only come to the front.

use std::path::{Path, PathBuf};

pub const BUNDLE_ID: &str = "com.t3tools.t3code";

/// The local server's environment id, the first part of a thread's route.
pub fn environment_file(home: &str) -> PathBuf {
    Path::new(home).join(".t3/userdata/environment-id")
}

/// The thread's link; `None` without an environment id.
pub fn link(thread: &str, environment_file: &Path) -> Option<String> {
    let env = std::fs::read_to_string(environment_file).ok()?;
    let env = env.trim();
    if env.is_empty() {
        return None;
    }
    // one path segment each: delegated threads' ids hold ":" and "%3A", which T3 Code decodes once
    Some(format!(
        "t3code://threads/{}/{}",
        segment(env),
        segment(thread)
    ))
}

/// Percent-encodes all but ASCII letters, digits and `-._~`.
fn segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
