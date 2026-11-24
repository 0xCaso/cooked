//! Cooked: live Claude and Codex usage as a glass island under the menu bar.

use rune::{AppSpec, Cx, El, col, text};

#[derive(Default)]
struct State;

fn view(_: &State, _: &mut Cx<State>) -> El {
    col().w_full().h_full().center().child(text("Cooked"))
}

fn main() {
    rune::run(AppSpec::<State>::new("Cooked", (320.0, 120.0), State::default, view));
}
