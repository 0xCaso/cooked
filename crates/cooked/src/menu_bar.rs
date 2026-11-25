//! Menu bar mode's clicks: what a click on the status item asks of the island, kept apart from AppKit so it can be
//! tested. The item (`mac::status_item`) calls `item_clicked` from its action, outside Rune's callbacks; the island
//! runs the `Command`s on its own turn.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use cooked_model::clock;
use cooked_model::menu_bar::{ItemAction, ItemClick, item_action};
use rune::{OverlayHandle, PopupMenu};

use crate::island::{self, Island, Model};
use crate::mac::status_item::StatusItem;

/// What a click on the item asks of the island.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Open,
    Close,
    /// A menu item, picked once the menu had closed.
    Pick(String),
}

/// Where the item's menu pops up: the item's button, or a test's stand-in.
pub trait MenuHost {
    /// Shows `menu` and returns once it has closed, with the id of the item picked.
    fn pop_up(&self, menu: &PopupMenu) -> Option<String>;
}

/// A click on the item. The left button toggles the popover (a click that just closed it from outside leaves it
/// closed, 1688b1a); the right one closes it and shows the menu, the same as the island's. The pick is sent after
/// the menu has closed (36cd10d: inside the menu's tracking loop, quitting did nothing).
pub fn item_clicked(click: ItemClick, model: &Model, host: &dyn MenuHost, send: &dyn Fn(Command)) {
    probe("item click");
    let action = {
        let m = model.borrow();
        item_action(click, m.is_open, m.closed_by_this_click(clock::now()))
    };
    match action {
        ItemAction::Open => send(Command::Open),
        ItemAction::Close => send(Command::Close),
        ItemAction::LeaveClosed => {}
        ItemAction::Menu => {
            send(Command::Close);
            // built and released before the menu tracks: Rune keeps drawing meanwhile, and may borrow the model
            let menu = island::menu_for(&model.borrow());
            if let Some(id) = host.pop_up(&menu) {
                send(Command::Pick(id));
            }
        }
    }
}

/// Adds the menu bar item for the island's `overlay`: its clicks become the island's `Command`s, run on the
/// island's turn. `None` off the main thread.
pub fn status_item(model: Model, overlay: OverlayHandle<Island>) -> Option<StatusItem> {
    StatusItem::new(Box::new(move |click, host| {
        let overlay = overlay.clone();
        item_clicked(click, &model, host, &move |command| {
            overlay.update(move |s, tx| s.command(command, tx));
        });
    }))
}

static PROBE: AtomicBool = AtomicBool::new(false);
static PROBE_START: OnceLock<std::sync::Mutex<Instant>> = OnceLock::new();

/// The click-to-popover latency probe (`--probe-item-click`, feature `tooling`): on, the popover opens where it is
/// (no anchor under the real menu bar), and each step logs its time since the click.
#[cfg(feature = "tooling")]
pub fn start_probe() {
    PROBE.store(true, Ordering::Relaxed);
}

pub fn probing() -> bool {
    PROBE.load(Ordering::Relaxed)
}

/// Logs `step`: the click starts the clock.
pub fn probe(step: &str) {
    if !probing() {
        return;
    }
    let start = PROBE_START.get_or_init(|| std::sync::Mutex::new(Instant::now()));
    let Ok(mut start) = start.lock() else { return };
    if step == "item click" {
        *start = Instant::now();
    }
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64());
    eprintln!(
        "probe {step}: {:.2} ms (at {epoch:.3})",
        start.elapsed().as_secs_f64() * 1000.0
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use cooked_model::defaults::MemoryDefaults;
    use cooked_model::demo_data::{self, Feed};
    use cooked_model::island_model::{DismissReason, IslandState};
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::island::{Prefs, shared};

    fn model() -> Model {
        let prefs: Prefs = Box::<MemoryDefaults>::default();
        shared(IslandState::new(
            demo_data::calm(),
            Some(Feed::DemoCalm),
            prefs,
            0.0,
        ))
    }

    /// A menu that checks what was sent while it was open, then picks `pick`.
    struct FakeMenu {
        sent: Rc<RefCell<Vec<Command>>>,
        during: RefCell<Vec<Command>>,
        pick: Option<&'static str>,
        shown: RefCell<Option<PopupMenu>>,
    }

    impl MenuHost for FakeMenu {
        fn pop_up(&self, menu: &PopupMenu) -> Option<String> {
            *self.during.borrow_mut() = self.sent.borrow().clone();
            *self.shown.borrow_mut() = Some(menu.clone());
            self.pick.map(str::to_owned)
        }
    }

    fn fake(pick: Option<&'static str>) -> (FakeMenu, Rc<RefCell<Vec<Command>>>) {
        let sent = Rc::new(RefCell::new(Vec::new()));
        let menu = FakeMenu {
            sent: sent.clone(),
            during: RefCell::new(Vec::new()),
            pick,
            shown: RefCell::new(None),
        };
        (menu, sent)
    }

    fn click(c: ItemClick, model: &Model, menu: &FakeMenu, sent: &Rc<RefCell<Vec<Command>>>) {
        let sink = sent.clone();
        item_clicked(c, model, menu, &move |cmd| sink.borrow_mut().push(cmd));
    }

    /// Regression 36cd10d: Quit and the other picks run after the menu has closed, never inside its tracking.
    #[test]
    fn actions_run_after_menu_closes() {
        let model = model();
        let (menu, sent) = fake(Some("quit"));
        click(ItemClick::Secondary, &model, &menu, &sent);
        // while the menu was up only the popover was asked to close
        assert_eq!(*menu.during.borrow(), vec![Command::Close]);
        assert_eq!(
            *sent.borrow(),
            vec![Command::Close, Command::Pick("quit".into())]
        );
        // the item's menu is the island's: Data, Glass, Show in, Settings…, Quit
        let shown = menu.shown.borrow().clone().expect("menu shown");
        assert!(shown.find("show:menuBar").is_some());
        assert!(shown.find("quit").is_some());
        // dismissed: nothing to run
        let (menu, sent) = fake(None);
        click(ItemClick::Secondary, &model, &menu, &sent);
        assert_eq!(*sent.borrow(), vec![Command::Close]);
    }

    /// Regression 1688b1a, through the item: the second click closes the popover, also when its mouse-down already
    /// closed it as a click outside.
    #[test]
    fn second_click_closes_popover() {
        let model = model();
        let (menu, sent) = fake(None);
        click(ItemClick::Primary, &model, &menu, &sent);
        assert_eq!(*sent.borrow(), vec![Command::Open]);
        model.borrow_mut().open();
        // the click-outside monitor saw the mouse-down first
        model
            .borrow_mut()
            .dismiss_popover(DismissReason::ClickOutside, clock::now());
        sent.borrow_mut().clear();
        click(ItemClick::Primary, &model, &menu, &sent);
        assert!(sent.borrow().is_empty(), "{:?}", sent.borrow());
        assert!(!model.borrow().is_open);
        // the monitor not yet run: the click closes it
        model.borrow_mut().open();
        click(ItemClick::Primary, &model, &menu, &sent);
        assert_eq!(*sent.borrow(), vec![Command::Close]);
        assert!(menu.shown.borrow().is_none());
    }
}
