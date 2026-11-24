//! Cooked: live Claude and Codex usage as a glass island under the menu bar.

mod app;
mod mac;
mod paths;
mod scheduler;
#[cfg(any(test, feature = "tooling"))]
mod snaps;
mod tiles;

fn main() {
    #[cfg(feature = "tooling")]
    if snaps::wanted(&std::env::args().collect::<Vec<_>>()) {
        snaps::app().run();
        return;
    }
    app::run();
}
