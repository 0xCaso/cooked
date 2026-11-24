//! Cooked: live Claude and Codex usage as a glass island under the menu bar.

mod app;
mod cli;
mod island;
mod mac;
mod menu_bar;
mod paths;
mod popover;
#[cfg(test)]
mod readability;
mod scheduler;
mod setup;
#[cfg(any(test, feature = "tooling"))]
mod snaps;
mod theme;
mod tiles;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let command = cli::parse(&args);
    // `--limits`, `--statusline`: print and exit, before anything opens a window.
    if let Some(code) = cli::run_headless(&command) {
        std::process::exit(code);
    }
    let cli::Command::App(launch) = command else {
        unreachable!("run_headless handles every other command");
    };
    #[cfg(feature = "tooling")]
    {
        if snaps::wanted_scene(&args, island::scenes::SCENES) {
            island::scenes::app().run();
            return;
        }
        if snaps::wanted_scene(&args, popover::scenes::SCENES) {
            popover::scenes::app().run();
            return;
        }
        if snaps::wanted_scene(&args, setup::scenes::SCENES) {
            setup::scenes::app().run();
            return;
        }
        if snaps::wanted(&args) {
            snaps::app().run();
            return;
        }
    }
    app::run(launch);
}
