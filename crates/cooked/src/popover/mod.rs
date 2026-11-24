//! The popover under the island, as `Views/PopoverView.swift` lays it out, in the "B · Chrome tiles" look:
//! a 440 pt glass page with the notice on top (one at a time), then
//! tokens per minute (`tpm`), what burns tokens (`burners`) and the usage of every account (`usage`).
//!
//! It is its own overlay, next to the island's: shown under the capsule when the island is tapped, and gone with
//! Esc, any other key, a click outside or another window taking the keyboard (`IslandPanel` in Swift). It takes the
//! keyboard without activating the app, so the app in use stays in front, and gives it back when it hides. Both
//! overlays share the island's model (`island::Model`).
//!
//! Step 0 (1d) found no glass morph across two windows, so it enters as DESIGN.md says: from `scale(.96)` and
//! transparent about its top centre on the 0.24 s spring, and leaves faster. Reduce Motion: no motion, and the
//! histogram and the metal stand still.

mod burners;
mod notice;
mod tpm;
mod usage;

use std::time::Duration;

use cooked_model::clock;
use cooked_model::island_model::{DismissReason, NoticeAction};
use cooked_model::island_placement::Rect;
use cooked_model::notice::Notice;
use cooked_model::token_sources::OpenTarget;
use rune::{
    Color, Cx, El, Key, KeyEvent, OverlayHandle, OverlayPlacement, Shadow, SystemMaterial, Tx, col,
    el, text,
};

use crate::island::Model;
use crate::mac::events::OutsideClicks;
use crate::theme::{GlassLook, Motion, Palette, TextStyle};
use crate::tiles::{self, TileGrid, TileGrids};

/// The page, points (mockup B `.pop`).
pub const WIDTH: f32 = 440.0;
/// The page's corners (mockup B: 26; cards 18, wells 9).
const RADIUS: f32 = 26.0;
pub const CARD_RADIUS: f32 = 18.0;
pub const WELL_RADIUS: f32 = 9.0;
/// Between the capsule's bottom and the page's top (mockup B `.stage` gap).
pub const GAP: f64 = 10.0;
/// The overlay's transparent margin around the page, for its shadow: little above (the island is there), more
/// below, where the shadow falls. Clicks there go through.
pub const MARGIN_X: f32 = 24.0;
pub const MARGIN_TOP: f32 = 8.0;
pub const MARGIN_BOTTOM: f32 = 32.0;
/// Free space kept under the page when it is taller than the display allows.
const SCREEN_BOTTOM_GAP: f64 = 8.0;
/// The page's height before it is first measured: about the demo peak's.
const FIRST_HEIGHT: f32 = 780.0;
/// The overlay before its first placement.
pub const FIRST_SIZE: (f32, f32) = (
    WIDTH + 2.0 * MARGIN_X,
    FIRST_HEIGHT + MARGIN_TOP + MARGIN_BOTTOM,
);

/// The tile grids' marks: the histogram, then a pair per account row (5 hours, week).
const HISTOGRAM_MARK: u32 = 1;
const FIRST_BAR_MARK: u32 = 10;

/// What a click in the popover asks of the rest of the app: opening a session (Task 18), Setup or Settings (Task
/// 15), the clipboard (Task 18). The app sets `Popover::requests`; until then nothing happens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    OpenSession(OpenTarget),
    OpenSetup,
    OpenPercentSettings,
    Copy(String),
}

/// Where the popover hangs: the island's capsule and the visible frame of its display, AppKit coordinates (origin
/// bottom-left of the primary display, y up), and the primary display's height to flip them for Rune.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    pub capsule: Rect,
    pub visible: Rect,
    pub primary_height: f64,
}

/// The overlay's frame (AppKit coordinates, margin included) for a page `height` points tall: centred under the
/// capsule, `GAP` below it, the page kept inside the display's visible frame across, and no taller than the space
/// down to the visible frame's bottom (Swift capped the panel at the screen's visible height).
pub fn frame(anchor: &Anchor, height: f32) -> Rect {
    let (mx, top, bottom) = (MARGIN_X as f64, MARGIN_TOP as f64, MARGIN_BOTTOM as f64);
    let width = WIDTH as f64 + 2.0 * mx;
    let page_top = anchor.capsule.min_y() - GAP;
    let room = (page_top - anchor.visible.min_y() - SCREEN_BOTTOM_GAP).max(0.0);
    let page = (height as f64).min(room);
    let full = page + top + bottom;
    let v = anchor.visible;
    let x = (anchor.capsule.mid_x() - width / 2.0)
        .min(v.max_x() - WIDTH as f64 - mx)
        .max(v.min_x() - mx);
    Rect::new(x, page_top + top - full, width, full)
}

/// `frame` as Rune's overlay rect (points from the primary display's top-left, y down).
fn placement(anchor: &Anchor, height: f32) -> OverlayPlacement {
    let f = frame(anchor, height);
    OverlayPlacement::Rect {
        x: f.x as f32,
        y: (anchor.primary_height - f.max_y()) as f32,
        width: f.width as f32,
        height: f.height as f32,
    }
}

/// The popover overlay's state.
pub struct Popover {
    pub model: Model,
    /// Set once the overlay exists. `None` in scenes.
    pub overlay: Option<OverlayHandle<Popover>>,
    /// What the clicks ask of the app.
    pub requests: fn(Request),
    /// The page is mounted: open, or on its way in.
    shown: bool,
    anchor: Option<Anchor>,
    /// The page's measured height, points.
    height: f32,
    histogram: TileGrid,
    /// The histogram's liquid: its surface moves between the minutes' heights.
    tide: cooked_model::tide::TideHistogram,
    /// Two per account row: its 5 hours, its week.
    bars: Vec<TileGrid>,
    /// "N more" opened the full list of sources; closing the popover folds it again.
    expanded: bool,
    /// "Copy details" was pressed: the button says "Copied" until the popover closes.
    copied: bool,
    /// `--reduce-motion`; the system setting is read at each open.
    reduce_motion_flag: bool,
    reduce_motion: bool,
    /// Reduce Transparency, read at each open: an opaque `#080808` page.
    solid: bool,
    /// `--open`: kept open for checks and measurements; only Esc or a key closes it (a test display never gives
    /// the panel the keyboard, and reports it lost).
    pinned: bool,
    /// The panel got the keyboard since it opened: only then does losing it close the popover (a panel that
    /// never became key, like one on a test display, reports losing focus as it shows).
    had_keys: bool,
    /// Scenes: the chart's clock stopped at this time.
    still_at: Option<f64>,
    outside: Option<OutsideClicks>,
}

fn no_requests(_: Request) {}

impl Popover {
    pub fn new(model: Model, args: &[String]) -> Self {
        let mut popover = Self {
            model,
            overlay: None,
            requests: no_requests,
            shown: false,
            anchor: None,
            height: FIRST_HEIGHT,
            histogram: TileGrid::new(0, 0, Vec::new()),
            tide: tpm::tide(),
            bars: Vec::new(),
            expanded: false,
            copied: false,
            reduce_motion_flag: args.iter().any(|a| a == tiles::REDUCE_MOTION_FLAG),
            reduce_motion: false,
            solid: false,
            had_keys: false,
            pinned: args.iter().any(|a| a == crate::app::OPEN_FLAG),
            still_at: None,
            outside: None,
        };
        popover.refresh();
        popover
    }

    /// Shown at once with the chart stopped at `t`: the headless scenes.
    #[cfg(any(test, feature = "tooling"))]
    pub fn still(model: Model, t: f64) -> Self {
        let mut popover = Self::new(model, &[]);
        popover.still_at = Some(t);
        popover.reduce_motion = true;
        popover.shown = true;
        popover.refresh();
        popover
    }

    #[cfg(test)]
    pub fn is_shown(&self) -> bool {
        self.shown
    }

    /// Opens under `anchor` (none in scenes: it stays where it is).
    pub fn show(&mut self, anchor: Option<Anchor>, tx: &mut Tx) {
        self.reduce_motion = self.reduce_motion_flag || tiles::system_reduce_motion();
        self.solid = crate::mac::window::reduce_transparency();
        self.shown = true;
        self.anchor = anchor;
        self.refresh();
        // The tap was in the island's window: the entrance plays only when this commit asks for it.
        tx.animated();
        let Some(overlay) = self.overlay.clone() else {
            return;
        };
        if let Some(anchor) = &self.anchor {
            overlay.set_placement(Some(placement(anchor, self.height)));
        }
        overlay.show();
        overlay.request_keys();
        let handle = overlay.clone();
        self.outside = OutsideClicks::watch(move || {
            handle.update(|p, _| p.dismiss(DismissReason::ClickOutside));
        });
    }

    /// Goes, the island or the model already closed: the page leaves on its exit while the window fades.
    pub fn hide(&mut self) {
        if !self.shown {
            return;
        }
        self.shown = false;
        self.expanded = false;
        self.copied = false;
        self.had_keys = false;
        self.outside = None;
        if let Some(overlay) = &self.overlay {
            if self.reduce_motion {
                overlay.hide();
            } else {
                overlay.fade_out(Duration::from_secs_f32(Motion::POPOVER_OUT_SECONDS));
            }
        }
    }

    /// Esc, a key, a click outside, another window taking the keyboard.
    pub fn dismiss(&mut self, reason: DismissReason) {
        let pinned = self.pinned
            && matches!(
                reason,
                DismissReason::ClickOutside | DismissReason::ResignedKey
            );
        if !self.shown || pinned {
            return;
        }
        self.model
            .borrow_mut()
            .dismiss_popover(reason, clock::now());
        self.hide();
    }

    /// The grids for the model as it is now; true when the view must draw again (the popover is shown).
    pub fn refresh(&mut self) -> bool {
        if !self.shown {
            return false;
        }
        self.bars = usage::bars(&self.model.borrow().snapshot);
        self.step_chart();
        true
    }

    /// The histogram at the chart's clock: the minute in progress grows, and closed minutes move left.
    fn step_chart(&mut self) {
        let mut m = self.model.borrow_mut();
        let (t, still) = match self.still_at {
            Some(t) => (t, true),
            None => (clock::now() - m.chart_epoch(), self.reduce_motion),
        };
        let range = m.chart_range();
        let history = m.histogram().history(t, still);
        self.histogram = tpm::histogram(&mut self.tide, &history, range, t, still);
    }

    /// The page's measured height: the overlay follows it.
    fn measured(&mut self, height: f32) {
        if (height - self.height).abs() < 0.5 {
            return;
        }
        self.height = height;
        if let (Some(overlay), Some(anchor)) = (&self.overlay, &self.anchor) {
            overlay.set_placement(Some(placement(anchor, height)));
        }
    }

    fn request(&self, request: Request) {
        (self.requests)(request);
    }

    /// The notice card's button.
    fn act(&mut self, notice: &Notice) {
        let action = self.model.borrow_mut().act(notice);
        match action {
            NoticeAction::OpenSetup => self.request(Request::OpenSetup),
            NoticeAction::OpenPercentSettings => self.request(Request::OpenPercentSettings),
            NoticeAction::Copy(text) => {
                self.copied = true;
                self.request(Request::Copy(text));
            }
            NoticeAction::Dismissed => {}
        }
    }

    /// "Not now".
    fn put_off(&mut self, notice: &Notice) {
        self.model.borrow_mut().dismiss(notice);
    }
}

impl TileGrids for Popover {
    fn tile_grid(&self, mark: u32) -> Option<&TileGrid> {
        if mark == HISTOGRAM_MARK {
            return Some(&self.histogram);
        }
        let i = mark.checked_sub(FIRST_BAR_MARK)? as usize;
        self.bars.get(i)
    }
}

/// Before each frame: the chart moves with its clock while the page is up. The view doesn't change for it.
pub fn poll(s: &mut Popover, _: f64, _: &mut Tx) -> bool {
    if s.shown && s.still_at.is_none() && !s.reduce_motion {
        s.step_chart();
    }
    false
}

/// The panel holds the keyboard only for Esc: any other key means the user went back to typing elsewhere, so it
/// closes and gives the keyboard back (at most that one key is lost).
pub fn on_key(s: &mut Popover, e: &KeyEvent, _: &mut Tx) -> bool {
    let reason = if e.key == Key::Escape {
        DismissReason::Escape
    } else {
        DismissReason::Typing
    };
    s.dismiss(reason);
    true
}

/// Another window took the keyboard.
pub fn on_focus(s: &mut Popover, focused: bool, _: &mut Tx) {
    if focused {
        s.had_keys = s.shown;
    } else if s.had_keys {
        s.dismiss(DismissReason::ResignedKey);
    }
}

/// The page: its glass, then the cards.
pub fn view(s: &Popover, _: &mut Cx<Popover>) -> El {
    let root = el()
        .key("root")
        .w_full()
        .h_full()
        .pt(MARGIN_TOP)
        .px(MARGIN_X)
        .pb(MARGIN_BOTTOM)
        // the page at its own height, so `on_layout` measures it even while the overlay is still too short
        .items_start();
    if !s.shown {
        return root;
    }
    root.child(page(s))
}

fn page(s: &Popover) -> El {
    let m = s.model.borrow();
    let snapshot = &m.snapshot;
    let notice = m.notice().cloned();
    let mut body = col().key("body").w_full().pt(14.0).px(12.0).pb(12.0);
    if let Some(n) = &notice {
        body = body.child(notice::card_for(n, s.copied));
    }
    // no account: the notice is all there is to show
    if notice != Some(Notice::NoAccounts) {
        body = body
            .child(section(
                "tpm-head",
                "Tokens per minute",
                None,
                notice.is_none(),
            ))
            .child(tpm::card_for(&m));
        if !snapshot.sources.is_empty() {
            body = body
                .child(section(
                    "burners-head",
                    "What burns tokens",
                    Some("last 5 min"),
                    false,
                ))
                .child(burners::card_for(snapshot, s.expanded));
        }
        if !snapshot.accounts.is_empty() {
            body = body
                .child(section("usage-head", "Usage", None, false))
                .child(usage::card_for(snapshot, FIRST_BAR_MARK));
        }
    }
    surface(s, body)
}

/// A section's label over its card (mockup B `.sec-h`): 12 pt at 55%, a note on the right.
fn section(key: &'static str, title: &str, note: Option<&str>, first: bool) -> El {
    let label = |key: &'static str, t: &str| {
        text(t.to_owned())
            .key(key)
            .font(TextStyle::Label.font())
            .color(Palette::ink3())
    };
    let mut head = rune::row()
        .key(key)
        .w_full()
        .justify_between()
        .pt(if first { 2.0 } else { 10.0 })
        .px(14.0)
        .pb(8.0)
        .child(label("title", title));
    if let Some(note) = note {
        head = head.child(label("note", note));
    }
    head
}

/// The page's surface: Liquid Glass under the glass style's tint and veil, or opaque `#080808` with Reduce
/// Transparency (as `IslandSurface`); a 0.5 pt hairline. Only the page takes the pointer.
fn surface(s: &Popover, body: El) -> El {
    let m = s.model.borrow();
    let style = m.glass_style();
    let solid = s.solid;
    let layer = |key: &'static str| el().key(key).w_full().radius(RADIUS).continuous_corners();
    let ring = if solid || style.draws_rim() {
        Some(Color::WHITE.alpha(0.16))
    } else {
        None
    };
    let mut face = layer("veil").child(body);
    if let Some(ring) = ring {
        face = face.border(0.5, ring);
    }
    let shell = el()
        .key("popover")
        .w(WIDTH)
        .radius(RADIUS)
        .continuous_corners()
        .fill(Palette::bg_base())
        .box_shadow(Shadow::core_animation(
            0.0,
            16.0,
            16.0,
            Color::BLACK.alpha(0.3),
        ))
        .box_shadow(Shadow::core_animation(
            0.0,
            8.0,
            8.0,
            Color::BLACK.alpha(0.3),
        ))
        .origin(0.5, 0.0)
        .transition(Motion::popover(s.reduce_motion))
        .hit()
        .on_layout(|s: &mut Popover, [_, _, _, h]| s.measured(h));
    if solid {
        shell.child(face.fill(Palette::bg_deep().alpha(0.98)))
    } else {
        let face = face.fill(Palette::bg_base().alpha(style.veil()));
        let tinted = match style.tint() {
            Some((tint, alpha)) => layer("tint").fill(tint.color().alpha(alpha)).child(face),
            None => face,
        };
        shell.system_material(SystemMaterial::Glass).child(tinted)
    }
}

/// Scenes for Rune's headless tooling (`--tree`, `--snap`; feature `tooling`): the popover over the calm and the
/// peak demo, and the calm one under a notice. The metal and the chart stopped at `snaps::SNAP_T`.
#[cfg(any(test, feature = "tooling"))]
pub mod scenes {
    use super::*;
    use cooked_model::defaults::MemoryDefaults;
    use cooked_model::demo_data::{self, Feed};
    use cooked_model::island_model::IslandState;
    use rune::{App, AppSpec};

    use crate::island::{Prefs, shared};
    use crate::snaps::SNAP_T;
    use crate::theme::PlexFont;
    use crate::tiles::ChromeTiles;

    pub const SCENES: &[&str] = &["popover-calm", "popover-peak", "popover-notice"];
    /// The window: the page and its margin, tall enough for the peak.
    pub const SIZE: (f32, f32) = (WIDTH + 2.0 * MARGIN_X, 880.0);

    pub fn popover(feed: Feed) -> Popover {
        let prefs: Prefs = Box::<MemoryDefaults>::default();
        let model = shared(IslandState::new(demo_data::calm(), Some(feed), prefs, 0.0));
        model.borrow_mut().select(feed);
        Popover::still(model, SNAP_T)
    }

    /// The calm demo with Claude's percent missing for "work".
    pub fn with_notice() -> Popover {
        let p = popover(Feed::DemoCalm);
        p.model.borrow_mut().show(vec![Notice::NoPercent {
            id: "work".to_owned(),
            account: "work".to_owned(),
        }]);
        p
    }

    fn init() -> Popover {
        popover(Feed::DemoCalm)
    }

    pub fn app() -> App<Popover> {
        App::new(AppSpec {
            scenes: &[
                ("popover-calm", |s: &mut Popover| {
                    *s = popover(Feed::DemoCalm)
                }),
                ("popover-peak", |s: &mut Popover| {
                    *s = popover(Feed::DemoPeak)
                }),
                ("popover-notice", |s: &mut Popover| *s = with_notice()),
            ],
            fonts: PlexFont::FILES,
            families: PlexFont::FAMILIES,
            ..AppSpec::new("Cooked popover", SIZE, init, view)
        })
        .srgb_blend(true)
        .compositor(ChromeTiles::still_at(SNAP_T))
    }
}

#[cfg(test)]
mod tests;
