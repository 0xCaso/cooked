//! The island under the menu bar, as `Views/IslandView.swift` and `IslandPanel.swift` draw and move it, in the
//! "B · Chrome tiles" look: one 290 x 40 pt glass capsule with the wave
//! (16 x 5 chrome tiles) in a well, the active account's 5-hour % left in a well, the account and the rate. In
//! urgency it keeps its shape: the percentage, the hairline and the wave turn red.
//!
//! A tap toggles the popover (`popover`), a drag past 3 pt moves the island and saves `islandPosition` as Swift
//! does, a right-click opens the menu (Data, Glass, Show in, Settings…, Quit).
//!
//! The overlay is the island plus a transparent `OVERHANG` margin on every side, as Swift's panel was, so the saved
//! position and `island_placement` keep Swift's meaning; clicks on the margin go through (`OverlayInput::Regions`).

use cooked_model::defaults::DefaultsStore;
use cooked_model::demo_data::Feed;
use cooked_model::island_model::{DisplayMode, GlassStyle, IslandState};
use cooked_model::island_placement::{self as placement, OVERHANG, Point, Rect, ScreenInfo, Size};
use cooked_model::position_store::{self, IslandPosition};
use cooked_model::token_format;
use rune::{
    Color, Cx, Drag, El, OverlayHandle, OverlayPlacement, Phase, PointerPress, PopupItem,
    PopupMenu, Shadow, SystemMaterial, Tx, el, row, text,
};
use std::cell::RefCell;
use std::rc::Rc;

use crate::mac::window::{self as mac, Displays};
use crate::popover::{Anchor, Popover};
use crate::scheduler;
use crate::setup;
use crate::theme::{GlassLook, Motion, Palette, TextStyle, well};
use crate::tiles::{TileGrid, TileGrids, Wave, tile_slot};

/// The settings store: `UserDefaults` in the app, `MemoryDefaults` in scenes and tests.
pub type Prefs = Box<dyn DefaultsStore>;
/// The one model the island and the popover share. Both overlays run on the event thread, each callback on its
/// own, so a borrow never outlives the callback that takes it.
pub type Model = Rc<RefCell<IslandState<Prefs>>>;

/// A model to share, from its state.
pub fn shared(model: IslandState<Prefs>) -> Model {
    Rc::new(RefCell::new(model))
}

/// The capsule, points: the approved mockup's 290 x 40 (DESIGN.md's Rune island exception, decision
/// 2026-11-24-rune-island-width).
pub const ISLAND: (f32, f32) = (290.0, 40.0);
/// The overlay: the capsule and its margin.
pub const WINDOW: (f32, f32) = (
    ISLAND.0 + 2.0 * OVERHANG as f32,
    ISLAND.1 + 2.0 * OVERHANG as f32,
);
/// The island wave's tiles (spec, "Chrome tiles").
const WAVE: (usize, usize) = (16, 5);
pub(crate) const WAVE_MARK: u32 = 1;
/// The wave well's width: the mockup's 112, narrower only when a long account name needs the room.
const WAVE_WIDTH: (f32, f32) = (56.0, 112.0);
/// Between the wave's tiles: with the well's 9 x 4 padding, tiles of ~4.75 x 3.44 pt, as the mockup.
const WAVE_GAP: f32 = 1.2;
/// Past this many points a press is a drag, not a tap (`PressableIsland.dragThreshold`).
pub const DRAG_THRESHOLD: f64 = 3.0;

/// A press on the island: where the pointer and the overlay were.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Press {
    mouse: Point,
    origin: Point,
}

/// What a menu pick asks of the app beyond the state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    None,
    Quit,
}

/// The island overlay's state.
pub struct Island {
    pub model: Model,
    /// Set once the overlay exists; `set_live` / `set_accounts` follow the feed and the accounts.
    pub reader: Option<scheduler::Control>,
    /// The overlay itself, to move it. `None` in scenes.
    pub overlay: Option<OverlayHandle<Island>>,
    /// The popover's overlay, which a tap opens and closes. `None` in scenes.
    pub popover: Option<OverlayHandle<Popover>>,
    /// Opens the Settings window ("Settings…"). `None` in scenes.
    pub setup: Option<setup::Opener>,
    /// The wave for the current rate and urgency.
    wave: TileGrid,
    /// The overlay's frame in AppKit screen coordinates (margin included), as Swift's panel frame.
    frame: Rect,
    /// The displays the frame was last placed for.
    displays: Option<Displays>,
    press: Option<Press>,
    dragging: bool,
    /// Reduce Transparency: an opaque `#161616` capsule instead of glass.
    pub solid: bool,
}

impl Island {
    pub fn new(model: Model) -> Self {
        let size = WINDOW;
        let mut island = Self {
            model,
            reader: None,
            overlay: None,
            popover: None,
            setup: None,
            wave: TileGrid::new(0, 0, Vec::new()),
            frame: Rect::new(0.0, 0.0, size.0 as f64, size.1 as f64),
            displays: None,
            press: None,
            dragging: false,
            solid: false,
        };
        island.refresh_wave();
        island
    }

    /// Switches the feed; the reader reads only while it is live.
    pub fn select_feed(&mut self, feed: Feed) {
        self.model.borrow_mut().select(feed);
        if let Some(reader) = &self.reader {
            reader.set_live(feed == Feed::Live);
        }
        self.refresh_wave();
        self.sync_popover();
    }

    /// The wave for the model's urgency; it sways in the compositor (the mockup's formula).
    pub fn refresh_wave(&mut self) {
        let (cols, rows) = WAVE;
        let wave = Wave {
            urgent: self.model.borrow().snapshot.is_urgent(),
        };
        // the mockup rounds tiles at 0.28 of their short side
        self.wave = TileGrid::wave(cols, rows, wave).spacing(WAVE_GAP, 0.95);
    }

    /// The connected accounts changed (Setup, Settings): the reader reads again at once with a new source.
    pub fn accounts_changed(&mut self, accounts: Vec<cooked_model::accounts::ConnectedAccount>) {
        self.model.borrow_mut().accounts_changed();
        if let Some(reader) = &self.reader {
            reader.set_accounts(accounts);
        }
    }

    /// A tap: opens or closes the popover. A click that just closed it from outside (the island is outside the
    /// popover) leaves it closed.
    pub fn toggle_popover(&mut self) {
        let (open, just_closed) = {
            let m = self.model.borrow();
            (
                m.is_open,
                m.closed_by_this_click(cooked_model::clock::now()),
            )
        };
        if open {
            self.close_popover();
        } else if !just_closed {
            self.open_popover();
        }
    }

    /// Opens the popover under the capsule.
    pub fn open_popover(&mut self) {
        self.model.borrow_mut().open();
        let anchor = self.popover_anchor();
        if let Some(popover) = &self.popover {
            popover.update(move |p, tx| p.show(anchor, tx));
        }
    }

    /// Closes the popover (a tap, a drag); Esc and clicks outside close it from its own side.
    pub fn close_popover(&mut self) {
        let was_open = std::mem::replace(&mut self.model.borrow_mut().is_open, false);
        if let Some(popover) = self.popover.as_ref().filter(|_| was_open) {
            popover.update(|p, _| p.hide());
        }
    }

    /// The model changed under an open popover: it draws again with it.
    pub fn sync_popover(&self) {
        if let Some(popover) = &self.popover {
            popover.update_if(|p, _| p.refresh());
        }
    }

    /// Where the popover hangs: the capsule, where its window really is, and the display that holds its centre.
    fn popover_anchor(&self) -> Option<Anchor> {
        let displays = self.displays.as_ref()?;
        let frame = self
            .overlay
            .as_ref()
            .and_then(|o| o.window_number())
            .and_then(mac::window_frame)
            .unwrap_or(self.frame);
        let capsule = frame.inset_by(OVERHANG, OVERHANG);
        let centre = Point::new(capsule.mid_x(), capsule.mid_y());
        let screen =
            placement::screen_containing(centre, &displays.screens).unwrap_or(&displays.main);
        Some(Anchor {
            capsule,
            visible: screen.visible_frame,
            primary_height: displays.primary_height,
        })
    }

    /// Every 3 s (`IslandRoot`'s task): the calm demo's next rate, the displays, Reduce Transparency and the
    /// glass style Rune's views may have lost. True when the view must change.
    pub fn tick(&mut self) -> bool {
        let changed_rate = {
            let mut m = self.model.borrow_mut();
            let rate = m.rate_now;
            m.tick_rate();
            rate != m.rate_now
        };
        let mut changed = changed_rate;
        if changed {
            self.refresh_wave();
            self.sync_popover();
        }
        let solid = mac::reduce_transparency();
        changed |= solid != self.solid;
        self.solid = solid;
        if let Some(now) = Displays::current()
            && self.displays.as_ref() != Some(&now)
        {
            self.restore_position(now);
        }
        self.apply_glass_style();
        changed
    }

    /// Where the island was left, on the same display; the main display's top centre the first time or when that
    /// display is gone (Review Focus 4).
    pub fn restore_position(&mut self, displays: Displays) {
        let saved = position_store::load(self.model.borrow().defaults());
        let size = Size::new(WINDOW.0 as f64, WINDOW.1 as f64);
        let frame =
            placement::restored_frame(size, saved.as_ref(), &displays.screens, &displays.main);
        self.displays = Some(displays);
        self.place(frame);
    }

    /// Moves the overlay to `frame` (AppKit coordinates).
    fn place(&mut self, frame: Rect) {
        self.frame = frame;
        if let (Some(overlay), Some(d)) = (&self.overlay, &self.displays) {
            overlay.set_placement(Some(overlay_rect(frame, d.primary_height)));
        }
    }

    /// The overlay's placement for the current frame, before the overlay exists.
    pub fn placement(&self) -> Option<OverlayPlacement> {
        self.displays
            .as_ref()
            .map(|d| overlay_rect(self.frame, d.primary_height))
    }

    /// The style on Rune's glass views: Clear is a style of `NSGlassEffectView` Rune doesn't set (step 0, 1c).
    /// Rune makes its views at the first frame and may make them again (a new window); each tick checks.
    pub fn apply_glass_style(&self) {
        if self.solid {
            return;
        }
        if let Some(number) = self.overlay.as_ref().and_then(|o| o.window_number()) {
            mac::set_glass_style(number, self.model.borrow().glass_style().native_style());
        }
    }

    /// A press, drag and release on the capsule. Below `DRAG_THRESHOLD` it is a tap; past it the island follows
    /// the pointer, kept on the display under the pointer, and its position is saved on release.
    fn drag(&mut self, d: &Drag) {
        match d.phase {
            Phase::Down => {
                self.press = Some(Press {
                    mouse: mac::mouse_location(),
                    origin: Point::new(self.frame.x, self.frame.y),
                });
                self.dragging = false;
            }
            Phase::Move => {
                let Some(press) = self.press else { return };
                let mouse = mac::mouse_location();
                if !self.dragging && moved(press.mouse, mouse) < DRAG_THRESHOLD {
                    return;
                }
                if !self.dragging {
                    self.dragging = true;
                    self.close_popover();
                }
                let screens = self
                    .displays
                    .as_ref()
                    .map(|d| d.screens.as_slice())
                    .unwrap_or(&[]);
                let frame = dragged_frame(press, mouse, self.frame, screens);
                self.place(frame);
            }
            Phase::Up => {
                let dragged = self.dragging;
                self.press = None;
                self.dragging = false;
                if dragged {
                    self.save_position();
                } else if !d.cancelled() {
                    self.toggle_popover();
                }
            }
        }
    }

    /// `PositionStore.save`: the frame's top centre on the display that holds the island's centre.
    fn save_position(&mut self) {
        let Some(displays) = &self.displays else {
            return;
        };
        let island = self.frame.inset_by(OVERHANG, OVERHANG);
        let centre = Point::new(island.mid_x(), island.mid_y());
        let screen =
            placement::screen_containing(centre, &displays.screens).unwrap_or(&displays.main);
        let position = saved_position(self.frame, screen);
        position_store::save(&position, self.model.borrow_mut().defaults_mut());
    }

    /// The right-click menu, as `IslandRoot`'s context menu and `MenuBarItem.showMenu`.
    pub fn menu(&self) -> PopupMenu {
        let (feed, glass, mode) = {
            let m = self.model.borrow();
            (m.feed(), m.glass_style(), m.display_mode())
        };
        PopupMenu::new()
            .item(PopupItem::new("data", "Data").enabled(false))
            .items(
                Feed::ALL.map(|f| {
                    PopupItem::new(format!("feed:{}", f.raw()), f.title()).checked(f == feed)
                }),
            )
            .separator()
            .submenu(
                "Glass",
                PopupMenu::new().items(GlassStyle::ALL.map(|g| {
                    PopupItem::new(format!("glass:{}", g.raw()), g.title()).checked(g == glass)
                })),
            )
            .submenu(
                "Show in",
                PopupMenu::new().items(DisplayMode::ALL.map(|m| {
                    PopupItem::new(format!("show:{}", m.raw()), m.title()).checked(m == mode)
                })),
            )
            .separator()
            .item(PopupItem::new("settings", "Settings…"))
            .item(PopupItem::new("quit", "Quit Cooked"))
    }

    /// A pick from `menu`.
    pub fn menu_pick(&mut self, id: &str) -> MenuAction {
        let (kind, value) = id.split_once(':').unwrap_or((id, ""));
        match kind {
            "feed" => {
                if let Some(feed) = Feed::from_raw(value) {
                    self.select_feed(feed);
                }
            }
            "glass" => {
                if let Some(style) = GlassStyle::from_raw(value) {
                    self.model.borrow_mut().set_glass_style(style);
                    self.apply_glass_style();
                }
            }
            // Saved now; the island hides for the status item in Task 16.
            "show" => {
                if let Some(mode) = DisplayMode::from_raw(value) {
                    self.model.borrow_mut().set_display_mode(mode);
                }
            }
            "settings" => {
                if let Some(setup) = &self.setup {
                    setup.open(setup::Mode::Settings, setup::Tab::Accounts);
                }
            }
            "quit" => return MenuAction::Quit,
            _ => {}
        }
        MenuAction::None
    }
}

impl TileGrids for Island {
    fn tile_grid(&self, mark: u32) -> Option<&TileGrid> {
        (mark == WAVE_MARK).then_some(&self.wave)
    }
}

/// Straight-line distance, points.
fn moved(a: Point, b: Point) -> f64 {
    (b.x - a.x).hypot(b.y - a.y)
}

/// `IslandPanel.dragChanged`: the press's frame moved with the pointer, kept on the display under the pointer.
fn dragged_frame(press: Press, mouse: Point, frame: Rect, screens: &[ScreenInfo]) -> Rect {
    let moved = Rect::new(
        press.origin.x + mouse.x - press.mouse.x,
        press.origin.y + mouse.y - press.mouse.y,
        frame.width,
        frame.height,
    );
    // Swift: the display under the pointer, else the panel's own (the pointer is in the menu bar or the Dock).
    let centre = Point::new(frame.mid_x(), frame.mid_y());
    let screen = placement::screen_containing(mouse, screens)
        .or_else(|| placement::screen_containing(centre, screens));
    match screen {
        Some(screen) => placement::clamped(moved, screen.visible_frame),
        None => moved,
    }
}

/// `SavedPosition(displayID:, topCenter: (frame.midX, frame.maxY))` of the overlay's frame (margin included).
fn saved_position(frame: Rect, screen: &ScreenInfo) -> IslandPosition {
    IslandPosition {
        display_id: screen.id,
        top_center: [frame.mid_x(), frame.max_y()],
    }
}

/// An AppKit frame (origin bottom-left of the primary display, y up) as Rune's overlay rect (points from the
/// primary display's top-left, y down).
fn overlay_rect(frame: Rect, primary_height: f64) -> OverlayPlacement {
    OverlayPlacement::Rect {
        x: frame.x as f32,
        y: (primary_height - frame.max_y()) as f32,
        width: frame.width as f32,
        height: frame.height as f32,
    }
}

fn press_handlers(capsule: El) -> El {
    capsule
        .hit()
        .on_drag(|s: &mut Island, d: &mut Drag, _: &mut Tx| s.drag(d))
        .on_secondary_press(|s: &mut Island, at: PointerPress, tx: &mut Tx| {
            tx.popup_menu(s.menu(), at, |s: &mut Island, picked, tx: &mut Tx| {
                if let Some(id) = picked
                    && s.menu_pick(id) == MenuAction::Quit
                {
                    if let Some(reader) = s.reader.take() {
                        reader.stop();
                    }
                    tx.quit();
                }
            });
        })
        .press_scale(Motion::ISLAND_SCALE)
}

/// The island: the capsule centred in its margin.
pub fn view(s: &Island, _: &mut Cx<Island>) -> El {
    el().key("root")
        .w_full()
        .h_full()
        .padding(OVERHANG as f32)
        .child(capsule(s))
}

/// The surface: Liquid Glass with the style's tint and veil, or opaque `#161616` with Reduce Transparency;
/// a 0.5 pt hairline, red in urgency.
fn capsule(s: &Island) -> El {
    let (w, h) = ISLAND;
    let radius = h / 2.0;
    let (style, urgent) = {
        let m = s.model.borrow();
        (m.glass_style(), m.snapshot.is_urgent())
    };
    let ring = if urgent {
        Some(Palette::red().alpha(0.7))
    } else if s.solid {
        Some(Color::WHITE.alpha(0.16))
    } else if style.draws_rim() {
        Some(Color::WHITE.alpha(0.14))
    } else {
        None
    };
    let layer = |key: &'static str| {
        el().key(key)
            .w_full()
            .h_full()
            .radius(radius)
            .continuous_corners()
    };
    let mut face = layer("veil").child(content(s));
    if let Some(ring) = ring {
        face = face.border(0.5, ring);
    }
    let shell = el()
        .key("island")
        .size(w, h)
        .radius(radius)
        .continuous_corners()
        .fill(Palette::bg_base());
    let shell = if s.solid {
        face = face.fill(Palette::bg_base().alpha(0.98));
        shell
            .box_shadow(Shadow::core_animation(
                0.0,
                8.0,
                8.0,
                Color::BLACK.alpha(0.3),
            ))
            .box_shadow(Shadow::core_animation(
                0.0,
                4.0,
                4.0,
                Color::BLACK.alpha(0.3),
            ))
            .child(face)
    } else {
        face = face.fill(Palette::bg_base().alpha(style.veil()));
        let tinted = match style.tint() {
            Some((tint, alpha)) => layer("tint").fill(tint.color().alpha(alpha)).child(face),
            None => face,
        };
        shell
            .system_material(SystemMaterial::Glass)
            .box_shadow(Shadow::core_animation(
                0.0,
                6.0,
                9.0,
                Color::BLACK.alpha(0.35),
            ))
            .child(tinted)
    };
    press_handlers(shell)
}

/// The row: wave, percentage, account, rate; "Set up Cooked" with no accounts.
fn content(s: &Island) -> El {
    let m = s.model.borrow();
    let snapshot = &m.snapshot;
    let urgent = snapshot.is_urgent();
    // The rate sits on the glass: ink3 there falls to 4.2:1 over a white window, so one step brighter.
    let rate_ink = if s.solid {
        Palette::ink3()
    } else {
        Palette::ink2()
    };
    let wave = well(
        el().key("wave")
            .h(30.0)
            .grow(1.0)
            .min_w(WAVE_WIDTH.0)
            .max_w(WAVE_WIDTH.1)
            .px(9.0)
            .py(4.0)
            .child(tile_slot(WAVE_MARK).key("tiles").w_full().h_full()),
        15.0,
    );
    let mut line = row()
        .key("row")
        .w_full()
        .h_full()
        .pl(5.0)
        .pr(14.0)
        .gap(8.0)
        .center_y()
        .child(wave);
    if m.shows_setup() {
        return line.child(
            text("Set up Cooked")
                .key("setup")
                .font(TextStyle::Body.font())
                .color(rate_ink),
        );
    }
    // Red 13 pt needs 4.5:1, which no glass reaches: the percentage sits in a well, red on #1b1b1b is 5.1:1.
    // Calm keeps the well too: calm → urgent changes only color, never shape (DESIGN.md).
    let percent = well(
        row().key("percent").h(24.0).px(7.0).center_y().child(
            text(m.five_hour_label())
                .key("percent-text")
                .font(TextStyle::NumberEmphasis.font())
                .color(if urgent {
                    Palette::red()
                } else {
                    Palette::ink()
                }),
        ),
        12.0,
    );
    line = line.child(percent);
    if let Some(account) = snapshot.active() {
        line = line.child(
            text(account.short.clone())
                .key("account")
                .font(TextStyle::Body.font())
                .color(Palette::ink2()),
        );
    }
    // The number at the rate ink, its unit at 55% as the mockup.
    line.child(
        row()
            .key("rate-row")
            .child(
                text(token_format::short(m.rate_now as f64))
                    .key("rate")
                    .font(TextStyle::Number.font())
                    .color(rate_ink),
            )
            .child(
                text("/min")
                    .key("rate-unit")
                    .font(TextStyle::Number.font())
                    .color(Palette::ink3()),
            ),
    )
}

/// Scenes for Rune's headless tooling (`--tree`, `--snap`; feature `tooling`): the calm and the urgent island
/// from the demos, the metal stopped at `snaps::SNAP_T`.
#[cfg(any(test, feature = "tooling"))]
pub mod scenes {
    use super::*;
    use cooked_model::defaults::MemoryDefaults;
    use cooked_model::demo_data;
    use rune::{App, AppSpec};

    use crate::snaps::SNAP_T;
    use crate::theme::PlexFont;
    use crate::tiles::ChromeTiles;

    pub const SCENES: &[&str] = &["island-calm", "island-urgent"];

    pub(crate) fn init() -> Island {
        let prefs: Prefs = Box::<MemoryDefaults>::default();
        Island::new(shared(IslandState::new(
            demo_data::calm(),
            Some(Feed::DemoCalm),
            prefs,
            0.0,
        )))
    }

    pub fn app() -> App<Island> {
        App::new(AppSpec {
            scenes: &[
                ("island-calm", |s: &mut Island| {
                    s.select_feed(Feed::DemoCalm)
                }),
                ("island-urgent", |s: &mut Island| {
                    s.select_feed(Feed::DemoPeak)
                }),
            ],
            fonts: PlexFont::FILES,
            families: PlexFont::FAMILIES,
            ..AppSpec::new("Cooked island", WINDOW, init, view)
        })
        .srgb_blend(true)
        .compositor(ChromeTiles::still_at(SNAP_T))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cooked_model::defaults::MemoryDefaults;
    use cooked_model::demo_data;
    use cooked_model::island_model::DismissReason;

    fn island(feed: Feed) -> Island {
        let mut island = scenes_init();
        island.select_feed(feed);
        island
    }

    fn scenes_init() -> Island {
        let prefs: Prefs = Box::<MemoryDefaults>::default();
        Island::new(shared(IslandState::new(
            demo_data::calm(),
            Some(Feed::DemoCalm),
            prefs,
            0.0,
        )))
    }

    fn screen(id: u32, x: f64, w: f64, h: f64) -> ScreenInfo {
        // a 24 pt menu bar on top
        ScreenInfo {
            id,
            visible_frame: Rect::new(x, 0.0, w, h - 24.0),
        }
    }

    #[test]
    fn window_is_the_island_and_its_margin() {
        assert_eq!(ISLAND, (290.0, 40.0));
        assert_eq!(WINDOW, (338.0, 88.0));
    }

    #[test]
    fn tap_toggles_the_popover() {
        let mut s = island(Feed::DemoCalm);
        s.toggle_popover();
        assert!(s.model.borrow().is_open);
        s.toggle_popover();
        assert!(!s.model.borrow().is_open);
        // a click outside closed it this instant: the same click on the island doesn't open it again
        s.toggle_popover();
        s.model
            .borrow_mut()
            .dismiss_popover(DismissReason::ClickOutside, cooked_model::clock::now());
        s.toggle_popover();
        assert!(!s.model.borrow().is_open);
    }

    /// `MotionTests.escapeAndOutsideClickClose`.
    #[test]
    fn escape_and_outside_click_close() {
        let s = island(Feed::DemoCalm);
        let mut m = s.model.borrow_mut();
        m.open();
        m.dismiss_popover(DismissReason::Escape, 0.0);
        assert!(!m.is_open);
        m.open();
        m.dismiss_popover(DismissReason::ClickOutside, 0.0);
        assert!(!m.is_open);
    }

    #[test]
    fn urgent_wave_is_red() {
        let calm = island(Feed::DemoCalm);
        let peak = island(Feed::DemoPeak);
        let (c, p) = (calm.wave.wave.unwrap(), peak.wave.wave.unwrap());
        assert!(!c.urgent && p.urgent);
        assert_eq!((peak.wave.cols, peak.wave.rows), WAVE);
    }

    #[test]
    fn menu_lists_the_swift_items_with_checks() {
        let s = island(Feed::DemoPeak);
        let menu = s.menu();
        assert!(!menu.find("data").unwrap().is_enabled());
        assert!(menu.find("feed:demoPeak").unwrap().is_checked());
        assert!(!menu.find("feed:live").unwrap().is_checked());
        assert_eq!(menu.find("feed:demoCalm").unwrap().title(), "Demo: calm");
        assert!(menu.find("glass:darker").unwrap().is_checked());
        assert!(menu.find("show:island").unwrap().is_checked());
        assert_eq!(menu.find("settings").unwrap().title(), "Settings…");
        assert_eq!(menu.find("quit").unwrap().title(), "Quit Cooked");
    }

    #[test]
    fn menu_picks_change_the_settings() {
        let mut s = island(Feed::DemoCalm);
        assert_eq!(s.menu_pick("feed:demoPeak"), MenuAction::None);
        assert_eq!(s.model.borrow().feed(), Feed::DemoPeak);
        assert!(s.wave.wave.unwrap().urgent);
        s.menu_pick("glass:clear");
        assert_eq!(s.model.borrow().glass_style(), GlassStyle::Clear);
        s.menu_pick("show:menuBar");
        assert_eq!(s.model.borrow().display_mode(), DisplayMode::MenuBar);
        assert_eq!(s.menu_pick("settings"), MenuAction::None);
        assert_eq!(s.menu_pick("quit"), MenuAction::Quit);
        assert_eq!(s.menu_pick("nonsense"), MenuAction::None);
        // kept across launches
        let prefs = std::mem::replace(
            &mut *s.model.borrow_mut(),
            IslandState::new(
                demo_data::calm(),
                None,
                Box::new(MemoryDefaults::default()),
                0.0,
            ),
        )
        .into_defaults();
        let again = IslandState::new(demo_data::calm(), None, prefs, 0.0);
        assert_eq!(again.glass_style(), GlassStyle::Clear);
    }

    #[test]
    fn a_drag_follows_the_pointer_and_stays_on_its_display() {
        let screens = [
            screen(1, 0.0, 1440.0, 900.0),
            screen(2, 1440.0, 1920.0, 1200.0),
        ];
        let frame = Rect::new(576.0, 812.0, 338.0, 88.0);
        let press = Press {
            mouse: Point::new(720.0, 850.0),
            origin: Point::new(frame.x, frame.y),
        };
        let moved = dragged_frame(press, Point::new(620.0, 700.0), frame, &screens);
        assert_eq!((moved.x, moved.y), (476.0, 662.0));
        // dragged up into the menu bar: the capsule stays under it (only the margin overhangs)
        let up = dragged_frame(press, Point::new(720.0, 899.0), frame, &screens);
        assert_eq!(up.max_y(), 876.0 + OVERHANG);
        // onto the second display: kept on that one
        let right = dragged_frame(press, Point::new(3000.0, 850.0), frame, &screens);
        assert!(right.x + OVERHANG >= 1440.0);
        assert!(right.max_x() - OVERHANG <= 3360.0);
    }

    #[test]
    fn saved_position_is_swifts_top_centre_and_restores_in_place() {
        let screens = [
            screen(1, 0.0, 1440.0, 900.0),
            screen(2, 1440.0, 1920.0, 1200.0),
        ];
        let frame = Rect::new(2000.0, 900.0, 338.0, 88.0);
        let saved = saved_position(frame, &screens[1]);
        assert_eq!(saved.display_id, 2);
        assert_eq!(saved.top_center, [2169.0, 988.0]);
        let json = String::from_utf8(position_store::encode(&saved)).unwrap();
        assert_eq!(json, r#"{"displayID":2,"topCenter":[2169.0,988.0]}"#);
        let size = Size::new(338.0, 88.0);
        assert_eq!(
            placement::restored_frame(size, Some(&saved), &screens, &screens[0]),
            frame
        );
        // the display is gone: top centre of the main one, 8 pt under the menu bar
        let gone = placement::restored_frame(size, Some(&saved), &screens[..1], &screens[0]);
        assert_eq!(gone.mid_x(), 720.0);
        assert_eq!(
            gone.max_y() - OVERHANG,
            876.0 - placement::GAP_BELOW_MENU_BAR
        );
    }

    #[test]
    fn overlay_rect_flips_to_rune_points() {
        let frame = Rect::new(576.0, 800.0, 338.0, 88.0);
        assert_eq!(
            overlay_rect(frame, 900.0),
            OverlayPlacement::Rect {
                x: 576.0,
                y: 12.0,
                width: 338.0,
                height: 88.0
            }
        );
    }

    /// Runs a scene through Rune's `--tree` and returns the tree.
    fn tree(scene: &str) -> serde_json::Value {
        let dir = std::env::temp_dir().join(format!("cooked-tree-{}-{scene}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let out = dir.join("tree.json");
        scenes::app()
            .args([
                "--tree".to_owned(),
                out.to_string_lossy().into_owned(),
                "--scene".to_owned(),
                scene.to_owned(),
            ])
            .try_run()
            .expect("headless tree");
        let json = std::fs::read_to_string(&out).expect("tree written");
        let _ = std::fs::remove_dir_all(&dir);
        serde_json::from_str(&json).expect("tree json")
    }

    /// The element with `key` in the tree's flat `elements` list.
    fn node<'a>(t: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
        t.get("elements")?
            .as_array()?
            .iter()
            .find(|e| e.get("key").and_then(|k| k.as_str()) == Some(key))
    }

    fn field(t: &serde_json::Value, key: &str, name: &str) -> String {
        node(t, key)
            .and_then(|n| n.get(name))
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("no {name} at {key}"))
            .to_owned()
    }

    fn width(t: &serde_json::Value, key: &str) -> f64 {
        node(t, key)
            .and_then(|n| n.get("rect"))
            .and_then(|r| r[2].as_f64())
            .unwrap_or_else(|| panic!("no rect at {key}"))
    }

    /// The calm island: white percentage, 72% account and rate, Plex, one 240 pt row.
    #[test]
    fn calm_scene_tree() {
        let t = tree("island-calm");
        let calm = demo_data::calm();
        let active = calm.active().expect("active account");
        assert_eq!(
            field(&t, "percent-text", "text"),
            format!("{}%", active.five_hour_left())
        );
        assert_eq!(field(&t, "percent-text", "color"), "#f4f1eaff");
        assert_eq!(field(&t, "account", "text"), active.short);
        assert_eq!(field(&t, "account", "color"), "#f4f1eab8");
        assert_eq!(
            field(&t, "rate", "text"),
            token_format::short(calm.rate_now as f64)
        );
        let family = |key| {
            node(&t, key)
                .and_then(|n| n["textStyle"]["family"].as_str())
                .map(str::to_owned)
        };
        assert_eq!(family("percent-text").as_deref(), Some("IBM Plex Mono"));
        assert_eq!(family("account").as_deref(), Some("IBM Plex Sans"));
        assert_eq!(width(&t, "island"), ISLAND.0 as f64);
        let wave = width(&t, "wave");
        assert!(
            (WAVE_WIDTH.0 as f64..=WAVE_WIDTH.1 as f64).contains(&wave),
            "wave {wave}"
        );
        // Darker by default: tint and 80% veil
        assert_eq!(field(&t, "veil", "fill"), "#161616cc");
        assert!(node(&t, "tint").is_some());
    }

    /// The urgent island: same shape, the percentage red in its well.
    #[test]
    fn urgent_scene_tree() {
        let t = tree("island-urgent");
        let peak = demo_data::peak();
        assert_eq!(
            field(&t, "percent-text", "text"),
            format!("{}%", peak.active().expect("active").five_hour_left())
        );
        assert_eq!(field(&t, "percent-text", "color"), "#ff453aff");
        assert_eq!(field(&t, "percent", "fill"), "#080808ff");
        assert_eq!(field(&t, "rate", "text"), "184k");
        assert_eq!(field(&t, "rate-unit", "text"), "/min");
        assert_eq!(field(&t, "rate-unit", "color"), "#f4f1ea8c");
        // the mockup's wave well, ~4.75 pt tiles
        assert_eq!(width(&t, "wave"), WAVE_WIDTH.1 as f64);
        let tile = (width(&t, "tiles") - 15.0 * WAVE_GAP as f64) / 16.0;
        assert!((tile - 4.75).abs() < 0.05, "tile width {tile}");
        assert_eq!(width(&t, "island"), ISLAND.0 as f64);
    }
}
