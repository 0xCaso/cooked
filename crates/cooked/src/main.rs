//! Cooked: live Claude and Codex usage as a glass island under the menu bar.

mod app;
mod island;
mod mac;
mod paths;
mod popover;
mod scheduler;
#[cfg(any(test, feature = "tooling"))]
mod snaps;
mod theme;
mod tiles;

fn main() {
    #[cfg(feature = "tooling")]
    {
        let args: Vec<String> = std::env::args().collect();
        if snaps::wanted_scene(&args, island::scenes::SCENES) {
            island::scenes::app().run();
            return;
        }
        if snaps::wanted_scene(&args, popover::scenes::SCENES) {
            popover::scenes::app().run();
            return;
        }
        if snaps::wanted(&args) {
            snaps::app().run();
            return;
        }
    }
    app::run();
}
