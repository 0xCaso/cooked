//! The app, as `CookedApp.swift` and `AppDelegate` start it: settings migrated, the island as a Rune overlay in an
//! accessory app (no Dock icon, no main window), and the live reader feeding the island's state.

use std::time::Duration;

use cooked_model::accounts::AccountStore;
use cooked_model::demo_data::{self, Feed};
use cooked_model::island_model::IslandState;
use cooked_model::{clock, legacy_defaults};
use rune::{App, AppSpec, Cx, El, KeyPolicy, OverlayInput, OverlayOptions, OverlaySpec, el};

use crate::island::{self, Island, Prefs, shared};
use crate::mac::defaults::UserDefaults;
use crate::mac::window::Displays;
use crate::popover::{self, Popover};
use crate::scheduler::{self, LiveReader};
use crate::theme::PlexFont;
use crate::tiles::ChromeTiles;

/// How often the island ticks (`IslandRoot`'s 3 s task): the calm demo's rate, displays, glass.
const TICK: Duration = Duration::from_secs(3);
/// The first tick, soon after the overlay's first frame made Rune's glass view.
const FIRST_TICK: Duration = Duration::from_millis(500);

/// `--open`: start with the popover open (as the Swift app's flag), for checks and measurements.
pub const OPEN_FLAG: &str = "--open";

/// `--feed demoCalm|demoPeak|live`: start on that feed, as the Swift app.
fn launch_feed(args: &[String]) -> Option<Feed> {
    let i = args.iter().position(|a| a == "--feed")?;
    Feed::from_raw(args.get(i + 1)?)
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
    let prefs: Prefs = Box::new(defaults);
    let model = shared(IslandState::new(
        demo_data::calm(),
        Some(Feed::Live),
        prefs,
        clock::now(),
    ));
    let args: Vec<String> = std::env::args().collect();
    let feed = launch_feed(&args);

    let mut state = Island::new(model.clone());
    state.solid = crate::mac::window::reduce_transparency();
    if let Some(displays) = Displays::current() {
        state.restore_position(displays);
    }
    let mut app = App::new(AppSpec {
        fonts: PlexFont::FILES,
        families: PlexFont::FAMILIES,
        ..AppSpec::new("Cooked", (1.0, 1.0), || (), no_window)
    })
    .windowless()
    .accessory();
    let options = OverlayOptions {
        placement: state.placement(),
        // Only the capsule takes the pointer: clicks on the margin reach the window below.
        input: OverlayInput::Regions,
        keys: KeyPolicy::None,
        prewarm: true,
        ..Default::default()
    };
    let island = app.overlay(
        OverlaySpec::new("Cooked island", island::WINDOW, island::view)
            .options(options)
            .srgb_blend(true)
            .compositor(ChromeTiles::new(&args)),
        state,
    );
    // The popover: its own overlay under the island, sharing the model. Hidden until a tap; it takes the keyboard
    // without activating the app while it is up, so Esc reaches it.
    let popover = app.overlay(
        OverlaySpec::new("Cooked popover", popover::FIRST_SIZE, popover::view)
            .options(OverlayOptions {
                input: OverlayInput::Regions,
                keys: KeyPolicy::OnDemand,
                prewarm: true,
                ..Default::default()
            })
            .srgb_blend(true)
            .animate_reveal()
            .poll(popover::poll)
            .on_key(popover::on_key)
            .on_focus(popover::on_focus)
            .compositor(ChromeTiles::new(&args)),
        Popover::new(model, &args),
    );
    let handle = popover.clone();
    popover.update(move |p, _| p.overlay = Some(handle));
    let sink = island.clone();
    let reader = scheduler::spawn(
        LiveReader::new(accounts),
        feed.is_none_or(|f| f == Feed::Live),
        move |reading| {
            sink.update(move |s, _| {
                s.model.borrow_mut().apply(&reading);
                s.refresh_wave();
                s.sync_popover();
            });
        },
    );
    let handle = island.clone();
    let open = args.iter().any(|a| a == OPEN_FLAG);
    island.update(move |s, _| {
        s.reader = Some(reader);
        s.overlay = Some(handle);
        s.popover = Some(popover);
        if let Some(feed) = feed {
            s.select_feed(feed);
        }
        if open {
            s.open_popover();
        }
    });
    let ticker = island.clone();
    let _ = std::thread::Builder::new()
        .name("cooked-tick".into())
        .spawn(move || {
            std::thread::sleep(FIRST_TICK);
            loop {
                ticker.update_if(|s, _| s.tick());
                std::thread::sleep(TICK);
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
