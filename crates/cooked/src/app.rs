//! The app, as `CookedApp.swift` and `AppDelegate` start it: settings migrated, the island as a Rune overlay in an
//! accessory app (no Dock icon, no main window), and the live reader feeding the island's state.

use std::time::Duration;

use cooked_model::accounts::AccountStore;
use cooked_model::demo_data::{self, Feed};
use cooked_model::island_model::IslandState;
use cooked_model::{clock, legacy_defaults};
use rune::{
    App, AppSpec, Cx, El, KeyPolicy, OverlayHandle, OverlayInput, OverlayOptions, OverlaySpec, el,
};

use crate::cli::Launch;

use crate::island::{self, Island, Prefs, shared};
use crate::mac::defaults::UserDefaults;
use crate::mac::open;
use crate::mac::pasteboard::Pasteboard;
use crate::mac::reopen::Reopen;
use crate::mac::window::Displays;
#[cfg(feature = "tooling")]
use crate::menu_bar;
use crate::popover::Request;
use crate::popover::{self, Popover};
use crate::scheduler::{self, LiveReader};
use crate::setup::{self, Setup};
use crate::theme::PlexFont;
use crate::tiles::ChromeTiles;

/// How often the island ticks (`IslandRoot`'s 3 s task): the calm demo's rate, displays, glass.
const TICK: Duration = Duration::from_secs(3);
/// The first tick, soon after the overlay's first frame made Rune's glass view.
const FIRST_TICK: Duration = Duration::from_millis(500);

/// The app has no window of its own; everything is an overlay.
fn no_window(_: &(), _: &mut Cx<()>) -> El {
    el()
}

/// Runs the app as `launch` (`cli`) asks.
pub fn run(launch: Launch) {
    let mut defaults = UserDefaults::app();
    // Before the model reads its settings, as Swift's `LegacyDefaults.migrate()`.
    let legacy = UserDefaults::suite(legacy_defaults::DOMAIN);
    legacy_defaults::migrate(Some(&legacy), &mut defaults);
    let home = std::env::var("HOME").unwrap_or_default();
    let accounts = AccountStore::current(&mut defaults, &home);
    let setup_done = setup::is_done(&defaults);
    let prefs: Prefs = Box::new(defaults);
    let model = shared(IslandState::new(
        demo_data::calm(),
        Some(Feed::Live),
        prefs,
        clock::now(),
    ));
    let feed = launch.feed;

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
            .compositor(ChromeTiles::new(launch.reduce_motion)),
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
            .compositor(ChromeTiles::new(launch.reduce_motion)),
        Popover::new(model.clone(), &launch),
    );
    // Setup and Settings: an ordinary window, created on its first open. New accounts go to the island's reader.
    let sink = island.clone();
    let mut settings = Setup::for_app(
        model.clone(),
        Box::new(move |accounts| {
            sink.update(move |s, _| s.accounts_changed(accounts));
        }),
        &launch,
    );
    let sink = island.clone();
    settings.on_display_mode = Some(Box::new(move || {
        sink.update(|s, _| s.sync_display_mode());
    }));
    let opener = setup::install(&mut app, settings);
    let handle = popover.clone();
    let requests = opener.clone();
    popover.update(move |p, _| {
        p.overlay = Some(handle);
        p.requests = std::rc::Rc::new(move |request| match request {
            Request::OpenSetup => requests.open(setup::Mode::Setup, setup::Tab::Accounts),
            Request::OpenPercentSettings => {
                requests.open(setup::Mode::Settings, setup::Tab::Percent)
            }
            Request::OpenSession(target) => open::open(target.as_ref()),
            Request::Copy(text) => Pasteboard::general().copy(&text),
        });
    });
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
    let open = launch.open;
    let mode = setup::launch_mode(&launch, setup_done);
    let menu_opener = opener.clone();
    #[cfg(feature = "tooling")]
    let probe = launch.probe_item_click.inspect(|_| menu_bar::start_probe());
    let reopened = island.clone();
    island.update(move |s, _| {
        // The loop runs, so AppKit has installed its own reopen handler: Cooked's replaces it.
        s.reopen = Reopen::install(move || reopened.update(|s, _| s.reopen()));
        s.reader = Some(reader);
        s.setup = Some(menu_opener);
        s.overlay = Some(handle);
        s.popover = Some(popover);
        // shows the island, or the menu bar item
        s.sync_display_mode();
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
    if let Some(every) = launch.toggle_every {
        toggle_every(every, island.clone());
    }
    #[cfg(feature = "tooling")]
    if let Some(after) = probe {
        probe_item_clicks(after, island.clone());
    }
    // First launch, or `--setup`: the Setup window. `--settings`: the Settings window.
    match mode {
        Some(setup::Mode::Setup) => opener.open(setup::Mode::Setup, setup::Tab::Accounts),
        Some(setup::Mode::Settings) => opener.open(setup::Mode::Settings, setup::Tab::Accounts),
        None => {}
    }
    app.run();
}

/// `--toggle-every <s>`: opens and closes the popover every `s` seconds, as Swift's timer, to record the transitions
/// without clicking.
fn toggle_every(every: Duration, island: OverlayHandle<Island>) {
    let _ = std::thread::Builder::new()
        .name("cooked-toggle".into())
        .spawn(move || {
            loop {
                std::thread::sleep(every);
                island.update(|s, _| {
                    if s.model.borrow().is_open {
                        s.close_popover();
                    } else {
                        s.open_popover();
                    }
                });
            }
        });
}

/// `--probe-item-click <s>` (feature `tooling`): the menu bar item's left click, run by the app itself `s` seconds
/// in and again `s` later (open, then close), with each step's time logged. No mouse event: the click handler runs
/// from an island turn instead of AppKit's action, so the time is the app's own path from the click to the
/// popover's first poll, just before its first frame is drawn.
#[cfg(feature = "tooling")]
fn probe_item_clicks(after: Duration, island: OverlayHandle<Island>) {
    struct NoMenu;
    impl menu_bar::MenuHost for NoMenu {
        fn pop_up(&self, _: &rune::PopupMenu) -> Option<String> {
            None
        }
    }
    let _ = std::thread::Builder::new()
        .name("cooked-probe".into())
        .spawn(move || {
            for _ in 0..2 {
                std::thread::sleep(after);
                island.update(|s, _| {
                    let Some(handle) = s.overlay.clone() else {
                        return;
                    };
                    let model = s.model.clone();
                    let click = cooked_model::menu_bar::ItemClick::Primary;
                    menu_bar::item_clicked(click, &model, &NoMenu, &move |command| {
                        handle.update(move |s, tx| s.command(command, tx));
                    });
                });
            }
        });
}
