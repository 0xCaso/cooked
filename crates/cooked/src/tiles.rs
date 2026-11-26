//! The chrome tiles and the soft dots: the account bars as small tiles of liquid metal, the histogram and the island
//! wave as round dots whose size and light follow their value (`crate::dots`, decision 2026-11-26).
//!
//! A grid is ONE Rune element (`tile_slot`), an effect slot whose mark names the grid; the state hands the grid's cells
//! to the compositor (`TileGrids`), which lays the tiles out inside the slot's rect and draws one `liquid_metal` layer
//! per tile: 0.55 ms a frame for 1872 tiles, against 3.1-4.0 ms for one element per tile.
//!
//! Lit tiles are chrome, red ones red metal, off tiles gunmetal at 6%. The metal's clock moves only in frames that are
//! drawn, so it stands still while the window is hidden or covered (Rune draws no frame then), and Reduce Motion (the
//! system setting or `--reduce-motion`) stops it.

use std::ops::Range;

use cooked_model::soft_dots::{Braid, ChartDots, Dot};
use cooked_model::tiles::TileLevel;
use rune::compose::{Compositor, DeviceKey, Frame};
use rune::render::KIND_EFFECT;
use rune::{El, el};
use rune_fx::{
    FillAnchor, Fx, FxFrame, FxLayer, FxOptions, FxRenderer, FxSlot, LiquidMetalParams, Shape, rgb,
};

use crate::dots::{DotPainter, Field, Layout};
use crate::theme::{Palette, Rgb};

/// Off tiles: gunmetal this faint, so the grid reads without competing with the lit tiles.
const OFF_OPACITY: f32 = 0.06;
/// The longest step the metal's clock takes in one frame: a frame after a pause doesn't jump.
const MAX_STEP: f64 = 0.1;
/// How often the system's Reduce Motion setting is read again, seconds.
const SETTING_EVERY: f64 = 1.0;
/// An unlit dot's alpha: the chart's, the wave's.
const CHART_OFF: f32 = 0.06;
const WAVE_OFF: f32 = 0.07;
/// Between the wave's dots, points.
pub const WAVE_PITCH: f32 = 2.6;

/// One grid of tiles, row-major from the top (`cooked_model::tiles`).
#[derive(Debug, Clone, PartialEq)]
pub struct TileGrid {
    pub cols: usize,
    pub rows: usize,
    pub cells: Vec<TileLevel>,
    /// Between tiles, points.
    pub gap: f32,
    /// Tile corner radius, points.
    pub radius: f32,
    /// Soft dots instead of tiles: the compositor lights them each frame, with their phosphor.
    pub dots: Option<Dots>,
}

/// What a dot grid shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Dots {
    /// The chart: column heights in rows (fractional), the columns that reach the threshold, the threshold row from
    /// the top, urgency (every dot red), and whether it is moving to a new range (a short tail).
    Chart {
        heights: Vec<f64>,
        reaches: Vec<bool>,
        threshold_row: usize,
        urgent: bool,
        switching: bool,
    },
    /// The island wave (`cooked_model::soft_dots::Braid`).
    Braid(Wave),
}

/// The island wave's inputs: urgency (red) and the rate's 0...1 level, which sets the braid's swing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wave {
    pub urgent: bool,
    pub level: f32,
}

impl TileGrid {
    /// A grid with the histogram's spacing: 1 pt gaps, 0.8 pt corners.
    pub fn new(cols: usize, rows: usize, cells: Vec<TileLevel>) -> Self {
        debug_assert_eq!(cells.len(), cols * rows);
        Self {
            cols,
            rows,
            cells,
            gap: 1.0,
            radius: 0.8,
            dots: None,
        }
    }

    /// The braided wave on `cols` x `rows` dots.
    pub fn wave(cols: usize, rows: usize, wave: Wave) -> Self {
        Self {
            dots: Some(Dots::Braid(wave)),
            ..Self::new(cols, rows, vec![TileLevel::Off; cols * rows])
        }
    }

    /// The chart's dots; `cells` are the same surface as tiles, for the tests and the readability check.
    pub fn chart(cols: usize, rows: usize, cells: Vec<TileLevel>, dots: Dots) -> Self {
        Self {
            dots: Some(dots),
            ..Self::new(cols, rows, cells)
        }
    }

    pub fn spacing(mut self, gap: f32, radius: f32) -> Self {
        self.gap = gap;
        self.radius = radius;
        self
    }
}

/// The level of tile `(c, r)` in row-major `cells` of width `cols`; missing cells are off.
fn level(cells: &[TileLevel], cols: usize, c: usize, r: usize) -> TileLevel {
    cells.get(r * cols + c).copied().unwrap_or_default()
}

/// The state's grids, by the mark their slot carries.
pub trait TileGrids {
    fn tile_grid(&self, mark: u32) -> Option<&TileGrid>;
}

/// The element a grid draws in; size it like any element. `mark` (not 0) names the grid in `TileGrids`.
pub fn tile_slot(mark: u32) -> El {
    el().effect(mark)
}

/// The metal of each kind of tile.
#[derive(Clone, Copy)]
struct Metals {
    lit: LiquidMetalParams,
    red: LiquidMetalParams,
    off: LiquidMetalParams,
}

impl Metals {
    fn new() -> Self {
        let lit = LiquidMetalParams {
            scale: 140.0,
            bevel: 0.0,
            dome: 0.0,
            flow: 0.35,
            speed: 0.35,
            contrast: 0.55,
            ..LiquidMetalParams::chrome()
        }
        .anchor(FillAnchor::World);
        // Red tiles: a lighter red and a much softer floor than the chrome (contrast 0.15 against 0.55), so they
        // read as red.
        let red = LiquidMetalParams {
            tint: rgb(0xff4f42),
            light: rgb(0xffe4e0),
            iridescence: 0.0,
            contrast: 0.15,
            ..lit
        };
        let off = LiquidMetalParams {
            bevel: 0.8,
            dome: 0.25,
            scale: 70.0,
            ..LiquidMetalParams::gunmetal()
        }
        .anchor(FillAnchor::World);
        Self { lit, red, off }
    }

    fn layer(&self, level: TileLevel, shape: Shape, t: f32) -> FxLayer {
        match level {
            TileLevel::Lit => Fx::fill(self.lit).on(shape).at(t),
            TileLevel::Red => Fx::fill(self.red).on(shape).at(t),
            TileLevel::Off => Fx::fill(self.off).on(shape).opacity(OFF_OPACITY).at(t),
        }
    }
}

/// The effect renderer, kept for the device and format it was made on.
struct FxCache {
    device_key: DeviceKey,
    format: wgpu::TextureFormat,
    fx: FxRenderer,
}

/// Whether the metal moves, and why not.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Motion {
    /// Follows the system's Reduce Motion setting.
    System { reduced: bool, read_at: Option<f64> },
    /// `--reduce-motion`, or a snapshot: never moves.
    Still,
}

/// The compositor: Rune's frame, with every grid's tiles drawn in its slot. Install it on the window or overlay
/// whose state implements `TileGrids`.
pub struct ChromeTiles {
    fx: Option<FxCache>,
    metals: Metals,
    motion: Motion,
    /// Reads the system's Reduce Motion (a stub in tests).
    read_reduce_motion: fn() -> bool,
    /// The metal's clock, seconds.
    t: f64,
    /// Whether the last frame drew any tiles.
    drew: bool,
    // Reused every frame, so a frame allocates nothing once they have grown.
    layers: Vec<FxLayer>,
    slots: Vec<FxSlot>,
    /// Per slot drawn: its instance and its range of `layers`.
    sites: Vec<(u32, Range<usize>)>,
    /// The dot grids' simulations, by mark.
    sims: Vec<(u32, Sim)>,
    /// This frame's dot fields: per slot drawn, its instance and its field.
    fields: Vec<(u32, Field)>,
    painter: Option<(DeviceKey, DotPainter)>,
    /// This frame's step, seconds; `None` stands the dots still.
    step: Option<f32>,
}

/// A dot grid's state across frames.
enum Sim {
    Chart(ChartDots),
    Braid(Box<Braid>),
}

impl ChromeTiles {
    /// Moving metal, unless `reduce_motion` (`--reduce-motion`) or the system asks for reduced motion.
    pub fn new(reduce_motion: bool) -> Self {
        let motion = if reduce_motion {
            Motion::Still
        } else {
            Motion::System {
                reduced: false,
                read_at: None,
            }
        };
        Self::with(motion, 0.0)
    }

    /// The metal stopped at `t`: snapshots and tests.
    #[cfg_attr(
        not(any(test, feature = "tooling")),
        expect(
            dead_code,
            reason = "only the headless scenes stop the metal at a set time"
        )
    )]
    pub fn still_at(t: f64) -> Self {
        Self::with(Motion::Still, t)
    }

    fn with(motion: Motion, t: f64) -> Self {
        Self {
            fx: None,
            metals: Metals::new(),
            motion,
            read_reduce_motion: system_reduce_motion,
            t,
            drew: false,
            layers: Vec::new(),
            slots: Vec::new(),
            sites: Vec::new(),
            sims: Vec::new(),
            fields: Vec::new(),
            painter: None,
            step: None,
        }
    }

    /// Whether the metal stands still now; reads the system setting again once a second of `clock`, and at every
    /// frame while it is reduced: the metal then draws none of its own, so the next one (the island saw the
    /// setting change) must find it off.
    fn still(&mut self, clock: f64) -> bool {
        match &mut self.motion {
            Motion::Still => true,
            Motion::System { reduced, read_at } => {
                if *reduced || read_at.is_none_or(|at| clock - at >= SETTING_EVERY || clock < at) {
                    *reduced = (self.read_reduce_motion)();
                    *read_at = Some(clock);
                }
                *reduced
            }
        }
    }

    /// Fills `layers`, `slots` and `sites` with the tiles of every grid slot in this frame.
    fn build<S: TileGrids>(&mut self, f: &Frame<'_, S>) {
        self.layers.clear();
        self.slots.clear();
        self.sites.clear();
        self.fields.clear();
        let t = self.t as f32;
        let rt = &f.rt;
        // An `FxLayer` is about 2 KB: sized to the tiles, not doubled past them, the popover's ~2400 layers hold
        // ~5 MB instead of the 9 MB a growing Vec ends at.
        let tiles = rt
            .instances
            .iter()
            .enumerate()
            .filter(|(_, i)| i.params[1] == KIND_EFFECT)
            .filter_map(|(n, _)| rt.state.tile_grid(rt.marks.get(n).copied().unwrap_or(0)))
            .filter(|g| g.dots.is_none())
            .map(|g| g.cols * g.rows)
            .sum::<usize>();
        fit(&mut self.layers, tiles);
        fit(&mut self.slots, tiles);
        for (n, i) in rt.instances.iter().enumerate() {
            if i.params[1] != KIND_EFFECT {
                continue;
            }
            let mark = rt.marks.get(n).copied().unwrap_or(0);
            let Some(grid) = rt.state.tile_grid(mark) else {
                continue;
            };
            if grid.cols == 0 || grid.rows == 0 {
                continue;
            }
            if let Some(dots) = &grid.dots {
                let field = self.light(mark, grid, dots, t);
                self.fields.push((n as u32, field));
                continue;
            }
            // The instance is in device pixels; the layers are in points at the slot's scale.
            let k = i.params[3];
            let [x, y, w, h] = i.rect.map(|v| v / k);
            let tw = (w - grid.gap * (grid.cols - 1) as f32) / grid.cols as f32;
            let th = (h - grid.gap * (grid.rows - 1) as f32) / grid.rows as f32;
            if tw <= 0.0 || th <= 0.0 {
                continue;
            }
            let slot = FxSlot::default().scale(k).clip(i.clip, i.fade);
            let start = self.layers.len();
            for r in 0..grid.rows {
                for c in 0..grid.cols {
                    let shape = Shape::rounded(
                        x + c as f32 * (tw + grid.gap),
                        y + r as f32 * (th + grid.gap),
                        tw,
                        th,
                        grid.radius,
                    );
                    self.layers.push(self.metals.layer(
                        level(&grid.cells, grid.cols, c, r),
                        shape,
                        t,
                    ));
                    self.slots.push(slot);
                }
            }
            self.sites.push((n as u32, start..self.layers.len()));
        }
    }

    /// The dots of the grid in slot `mark` at the clock `t`, stepped by this frame's `step`.
    fn light(&mut self, mark: u32, grid: &TileGrid, dots: &Dots, t: f32) -> Field {
        let (cols, rows) = (grid.cols, grid.rows);
        let at = match self.sims.iter().position(|(m, _)| *m == mark) {
            Some(i) => i,
            None => {
                self.sims.push((mark, Sim::Chart(ChartDots::default())));
                self.sims.len() - 1
            }
        };
        let sim = &mut self.sims[at].1;
        let mut out = Vec::new();
        match dots {
            Dots::Chart {
                heights,
                reaches,
                threshold_row,
                urgent,
                switching,
            } => {
                if !matches!(sim, Sim::Chart(_)) {
                    *sim = Sim::Chart(ChartDots::default());
                }
                let Sim::Chart(chart) = sim else {
                    unreachable!()
                };
                chart.switching = *switching;
                chart.step(
                    heights,
                    reaches,
                    *threshold_row,
                    *urgent,
                    cols,
                    rows,
                    self.step,
                    &mut out,
                );
                Field {
                    cols,
                    rows,
                    layout: Layout::Fill,
                    off: CHART_OFF,
                    dots: out,
                }
            }
            Dots::Braid(wave) => {
                if !matches!(sim, Sim::Braid(b) if b.cols == cols && b.rows == rows) {
                    let mut braid = Braid::new(cols, rows, WAVE_PITCH);
                    braid.settle(t, wave.level);
                    *sim = Sim::Braid(Box::new(braid));
                }
                let Sim::Braid(braid) = sim else {
                    unreachable!()
                };
                match self.step {
                    Some(dt) => braid.step(t, dt, wave.level),
                    None => braid.settle(t, wave.level),
                }
                out.extend(braid.powers().iter().map(|&power| Dot {
                    power,
                    red: wave.urgent,
                }));
                Field {
                    cols,
                    rows,
                    layout: Layout::Pitch(WAVE_PITCH),
                    off: WAVE_OFF,
                    dots: out,
                }
            }
        }
    }
}

/// The dots of a dot grid standing still at `t`, as the compositor draws them in snapshots.
#[cfg(test)]
pub(crate) fn still_field(grid: &TileGrid, t: f32) -> Option<Field> {
    let dots = grid.dots.as_ref()?;
    Some(ChromeTiles::still_at(t as f64).light(0, grid, dots, t))
}

/// A palette colour as the shader takes it.
fn shader_rgb(c: Rgb) -> [f32; 3] {
    [c.r as f32, c.g as f32, c.b as f32]
}

impl<S: TileGrids + 'static> Compositor<S> for ChromeTiles {
    fn animating(&self) -> bool {
        self.drew
            && !matches!(
                self.motion,
                Motion::Still | Motion::System { reduced: true, .. }
            )
    }

    fn release_gpu(&mut self) {
        self.fx = None;
        // The frame's layers go with the renderer: ~5 MB for the popover's grids, rebuilt at the next show.
        self.layers = Vec::new();
        self.slots = Vec::new();
        self.sites = Vec::new();
        self.painter = None;
    }

    fn render(&mut self, f: &mut Frame<'_, S>) {
        self.step = if self.still(f.rt.clock) {
            None
        } else {
            let dt = f.dt.clamp(0.0, MAX_STEP);
            self.t += dt;
            Some(dt as f32)
        };
        self.build(f);
        self.drew = !self.sites.is_empty() || !self.fields.is_empty();
        if !self.drew {
            // No grid on screen: Rune's own frame, and no effect renderer kept for nothing.
            self.fx = None;
            self.painter = None;
            f.draw_ui();
            return;
        }
        let (device, queue, size) = (f.device, f.queue, f.size);
        let device_key = f.device_key();
        if self.fields.is_empty() {
            self.painter = None;
        } else if self.painter.as_ref().is_none_or(|(k, _)| *k != device_key) {
            self.painter = Some((device_key, DotPainter::new(device)));
        }
        if self.sites.is_empty() {
            self.fx = None;
        } else if self
            .fx
            .as_ref()
            .is_none_or(|c| c.device_key != device_key || c.format != f.format)
        {
            let mut fx = FxRenderer::new(device, f.format, FxOptions { dither: true });
            // Slots inside an isolated layer (a fade) draw into this format.
            fx.add_format(device, wgpu::TextureFormat::Rgba16Float);
            self.fx = Some(FxCache {
                device_key,
                format: f.format,
                fx,
            });
        }
        let mut fx = self.fx.as_mut().map(|c| &mut c.fx);
        if let Some(fx) = fx.as_mut() {
            fx.set_srgb_blend(f.srgb_blend());
            fx.set_fade_stops(f.fade_stops());
            fx.prepare_with_slots(
                device,
                queue,
                &FxFrame {
                    viewport: [size.0, size.1],
                    scale: 1.0,
                    layers: &self.layers,
                    backdrop: None,
                    sources: &[],
                },
                &self.slots,
            );
        }
        let clear = f.rt.clear_color();
        f.ui.prepare(device, queue, &mut f.rt.text.atlas, &f.rt.instances, size);
        let (sites, fields) = (&self.sites, &self.fields);
        let mut painter = self.painter.as_mut().map(|(_, p)| p);
        let colors = (shader_rgb(Palette::INK), shader_rgb(Palette::RED));
        f.ui.draw_with(f.encoder, f.target, clear, &mut |pass, s| {
            if let (Some(fx), Some((_, range))) =
                (fx.as_ref(), sites.iter().find(|(n, _)| *n == s.index))
            {
                for k in range.clone() {
                    fx.draw_layer_in(pass, s.format, s.origin, k);
                }
            }
            if let (Some(painter), Some((_, field))) =
                (painter.as_mut(), fields.iter().find(|(n, _)| *n == s.index))
            {
                painter.draw(device, queue, pass, s, field, colors);
            }
        });
    }
}

/// Gives the empty `v` room for exactly `n` items: grows it to `n`, or shrinks it when it holds far more (the
/// popover's grids got smaller).
fn fit<T>(v: &mut Vec<T>, n: usize) {
    if v.capacity() < n {
        v.reserve_exact(n);
    } else if v.capacity() > n.saturating_mul(2).max(64) {
        v.shrink_to(n);
    }
}

/// System Settings › Accessibility › Display › Reduce motion.
pub fn system_reduce_motion() -> bool {
    objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_reads_row_major_and_pads_missing_cells_off() {
        let cells = vec![
            TileLevel::Lit,
            TileLevel::Off,
            TileLevel::Red,
            TileLevel::Lit,
        ];
        assert_eq!(level(&cells, 2, 0, 1), TileLevel::Red);
        assert_eq!(level(&cells, 2, 1, 0), TileLevel::Off);
        assert_eq!(level(&cells, 2, 5, 5), TileLevel::Off);
    }

    #[test]
    fn a_wave_is_a_braid_of_dots() {
        let wave = Wave {
            urgent: true,
            level: 0.4,
        };
        let grid = TileGrid::wave(40, 9, wave);
        assert_eq!(grid.dots, Some(Dots::Braid(wave)));
        let mut tiles = ChromeTiles::still_at(1.0);
        let field = tiles.light(1, &grid, grid.dots.as_ref().unwrap(), 1.0);
        assert_eq!(field.dots.len(), 360);
        assert_eq!(field.layout, Layout::Pitch(WAVE_PITCH));
        assert!(field.dots.iter().all(|d| d.red));
        assert!(field.dots.iter().any(|d| d.power > 0.5));
    }

    #[test]
    fn chart_dots_keep_a_tail_while_moving_and_none_when_still() {
        let chart = |h: f64| Dots::Chart {
            heights: vec![h; 4],
            reaches: vec![false; 4],
            threshold_row: 2,
            urgent: false,
            switching: false,
        };
        let grid = |h| TileGrid::chart(4, 10, vec![TileLevel::Off; 40], chart(h));
        let mut tiles = ChromeTiles::still_at(0.0);
        tiles.step = Some(0.016);
        let high = grid(10.0);
        tiles.light(1, &high, high.dots.as_ref().unwrap(), 0.0);
        let low = grid(1.0);
        let fading = tiles.light(1, &low, low.dots.as_ref().unwrap(), 0.0);
        assert!(fading.dots[0].power > 0.3, "the top dot fades");
        tiles.step = None;
        let still = tiles.light(1, &low, low.dots.as_ref().unwrap(), 0.0);
        assert_eq!(still.dots[0].power, 0.0);
    }

    #[test]
    fn fit_sizes_to_the_tiles() {
        let mut v: Vec<u64> = Vec::new();
        fit(&mut v, 2400);
        assert_eq!(v.capacity(), 2400, "exact, not the next power of two");
        fit(&mut v, 2000);
        assert_eq!(v.capacity(), 2400, "kept when a bit smaller");
        fit(&mut v, 100);
        assert!(v.capacity() < 2400, "shrunk when far smaller");
    }

    #[test]
    fn reduce_motion_flag_stops_the_metal() {
        let mut still = ChromeTiles::new(true);
        assert!(still.still(0.0));
        assert!(still.still(10.0));
        let mut system = ChromeTiles::new(false);
        assert_eq!(system.still(0.0), system_reduce_motion());
    }

    /// While reduced the metal draws no frame of its own: the next frame that comes (the island saw the setting
    /// change) reads it again at once, and the metal moves on.
    #[test]
    fn reduced_metal_reads_the_setting_at_its_next_frame() {
        let mut tiles = ChromeTiles::new(false);
        tiles.read_reduce_motion = || true;
        assert!(tiles.still(5.0));
        tiles.drew = true;
        assert!(!Compositor::<Grids>::animating(&tiles));
        tiles.read_reduce_motion = || false;
        assert!(!tiles.still(5.1), "read again within the second");
        assert!(Compositor::<Grids>::animating(&tiles));
    }

    struct Grids;

    impl TileGrids for Grids {
        fn tile_grid(&self, _: u32) -> Option<&TileGrid> {
            None
        }
    }

    /// While the metal moves, the setting is read once a second.
    #[test]
    fn system_setting_is_read_again_after_a_second() {
        let mut tiles = ChromeTiles::new(false);
        tiles.read_reduce_motion = || false;
        assert!(!tiles.still(5.0));
        tiles.read_reduce_motion = || true;
        assert!(!tiles.still(5.5), "kept within the second");
        assert!(tiles.still(6.0));
        assert_eq!(
            tiles.motion,
            Motion::System {
                reduced: true,
                read_at: Some(6.0)
            }
        );
    }
}
