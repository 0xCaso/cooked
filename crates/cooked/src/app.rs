//! The app, as `CookedApp.swift` and `AppDelegate` start it: settings migrated, the island as a Rune overlay in an
//! accessory app (no Dock icon, no main window), and the live reader feeding the island's state.

use cooked_model::accounts::{AccountStore, ConnectedAccount};
use cooked_model::demo_data::{self, Feed};
use cooked_model::island_model::IslandState;
use cooked_model::{clock, legacy_defaults};
use rune::{
    Align, App, AppSpec, Color, Cx, El, KeyPolicy, OverlayInput, OverlayOptions, OverlayPlacement,
    OverlaySpec, ScreenEdge, SystemMaterial, TextStyle, el, row, text,
};

use crate::mac::defaults::UserDefaults;
use crate::scheduler::{self, LiveReader};

/// The island's size, in points (Task 12 sets the real layout).
const SIZE: (f32, f32) = (240.0, 44.0);
/// Under the menu bar: 6 pt below a 33 pt bar (step-0 spike).
const TOP_MARGIN: f32 = 39.0;

/// The island overlay's state: the model, and the reader thread that feeds it.
pub struct Island {
    pub model: IslandState<UserDefaults>,
    /// Set once the overlay exists; `set_live` / `set_accounts` follow the feed and the accounts.
    pub reader: Option<scheduler::Control>,
}

impl Island {
    /// Switches the feed; the reader reads only while it is live.
    pub fn select_feed(&mut self, feed: Feed) {
        self.model.select(feed);
        if let Some(reader) = &self.reader {
            reader.set_live(feed == Feed::Live);
        }
    }

    /// The connected accounts changed (Setup, Settings): the reader reads again at once with a new source.
    #[expect(dead_code, reason = "the Setup window (Task 15) calls it")]
    pub fn accounts_changed(&mut self, accounts: Vec<ConnectedAccount>) {
        self.model.accounts_changed();
        if let Some(reader) = &self.reader {
            reader.set_accounts(accounts);
        }
    }
}

/// `--feed demoCalm|demoPeak|live`: start on that feed, as the Swift app.
fn launch_feed(args: &[String]) -> Option<Feed> {
    let i = args.iter().position(|a| a == "--feed")?;
    Feed::from_raw(args.get(i + 1)?)
}

/// Placeholder until the island view (Task 12): the glass capsule with the 5-hour label and the rate.
fn island_view(s: &Island, _: &mut Cx<Island>) -> El {
    let ink = Color::WHITE;
    el().key("root").w_full().h_full().child(
        row()
            .key("capsule")
            .size(SIZE.0, SIZE.1)
            .radius(SIZE.1 / 2.0)
            .continuous_corners()
            .system_material(SystemMaterial::Glass)
            .px(16.0)
            .gap(10.0)
            .center_y()
            .child(
                text(s.model.five_hour_label())
                    .font(TextStyle::new(13.0, 500.0))
                    .color(ink.alpha(0.72)),
            )
            .child(el().grow(1.0))
            .child(
                text(format!("{}/min", s.model.rate_now))
                    .font(TextStyle::new(12.0, 400.0))
                    .color(ink.alpha(0.55)),
            ),
    )
}

/// The app has no window of its own; everything is an overlay.
fn no_window(_: &(), _: &mut Cx<()>) -> El {
    el()
}

pub fn run() {
    let mut defaults = UserDefaults::app();
    // Before the model reads its settings, as Swift's `LegacyDefaults.migrate()`.
    let legacy = UserDefaults::suite(legacy_defaults::DOMAIN);
    legacy_defaults::migrate(Some(&legacy), &mut defaults);
    let home = std::env::var("HOME").unwrap_or_default();
    let accounts = AccountStore::current(&mut defaults, &home);
    let model = IslandState::new(demo_data::calm(), Some(Feed::Live), defaults, clock::now());
    let feed = launch_feed(&std::env::args().collect::<Vec<_>>());

    let mut app = App::new(AppSpec::new("Cooked", (1.0, 1.0), || (), no_window))
        .windowless()
        .accessory();
    let options = OverlayOptions {
        placement: Some(OverlayPlacement::Edge {
            edge: ScreenEdge::Top,
            margin: TOP_MARGIN,
            align: Align::Center,
            inset: 0.0,
        }),
        input: OverlayInput::All,
        keys: KeyPolicy::None,
        prewarm: true,
        ..Default::default()
    };
    let island = app.overlay(
        OverlaySpec::new("Cooked island", SIZE, island_view)
            .options(options)
            .srgb_blend(true),
        Island {
            model,
            reader: None,
        },
    );
    let sink = island.clone();
    let reader = scheduler::spawn(
        LiveReader::new(accounts),
        feed.is_none_or(|f| f == Feed::Live),
        move |reading| {
            sink.update(move |s, _| s.model.apply(&reading));
        },
    );
    island.update(move |s, _| {
        s.reader = Some(reader);
        if let Some(feed) = feed {
            s.select_feed(feed);
        }
    });
    island.show();
    app.run();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_feed_reads_the_flag() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            launch_feed(&args(&["cooked", "--feed", "demoPeak"])),
            Some(Feed::DemoPeak)
        );
        assert_eq!(launch_feed(&args(&["cooked", "--feed"])), None);
        assert_eq!(launch_feed(&args(&["cooked"])), None);
    }
}
