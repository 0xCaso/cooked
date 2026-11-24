//! The chrome tiles: the histogram, the island wave and the account bars as small tiles of liquid metal.
//!
//! A grid is ONE Rune element (`tile_slot`), an effect slot whose mark names the grid; the state hands the grid's
//! cells to the compositor (`TileGrids`), which lays the tiles out inside the slot's rect and draws one
//! `liquid_metal` layer per tile. Step 0 (1b) measured this at 0.55 ms a frame for 1872 tiles, against 3.1-4.0 ms
//! for one element per tile.
//!
//! Lit tiles are chrome, red ones red metal, off tiles gunmetal at 6%. The metal's clock moves only in frames that
//! are drawn, so it stands still while the window is hidden or covered (Rune draws no frame then), and Reduce
//! Motion (the system setting or `--reduce-motion`) stops it.

use std::ops::Range;

use cooked_model::tiles::{self as grids, TileLevel};
use rune::compose::{Compositor, DeviceKey, Frame};
use rune::render::KIND_EFFECT;
use rune::{El, el};
use rune_fx::{
    FillAnchor, Fx, FxFrame, FxLayer, FxOptions, FxRenderer, FxSlot, LiquidMetalParams, Shape, rgb,
};

/// Off tiles: gunmetal this faint, so the grid reads without competing with the lit tiles.
const OFF_OPACITY: f32 = 0.06;
/// The longest step the metal's clock takes in one frame: a frame after a pause doesn't jump.
const MAX_STEP: f64 = 0.1;
/// How often the system's Reduce Motion setting is read again, seconds.
const SETTING_EVERY: f64 = 1.0;

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
    /// A swaying wave instead of fixed cells: whether it is red. The compositor draws it at the
    /// metal's clock, so it sways with the metal and stands still with it (Reduce Motion).
    pub wave: Option<Wave>,
}

/// The island wave's inputs (`cooked_model::tiles::island_wave_grid`, the mockup's sway).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wave {
    pub urgent: bool,
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
            wave: None,
        }
    }

    /// The mirrored wave on `cols` x `rows` tiles, swaying with the metal.
    pub fn wave(cols: usize, rows: usize, wave: Wave) -> Self {
        Self {
            wave: Some(wave),
            ..Self::new(cols, rows, vec![TileLevel::Off; cols * rows])
        }
    }

    /// The cells at the metal's clock `t`: the wave's at `t`, else the fixed ones.
    pub(crate) fn cells_at(&self, t: f32, out: &mut Vec<TileLevel>) {
        out.clear();
        match self.wave {
            Some(w) => out.extend(grids::island_wave_grid(t, w.urgent, self.cols, self.rows)),
            None => out.extend_from_slice(&self.cells),
        }
        out.resize(self.cols * self.rows, TileLevel::Off);
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
        // read as the mockup's red (Task 14 variant B, signed off 2026-11-24).
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
    /// The metal's clock, seconds.
    t: f64,
    /// Whether the last frame drew any tiles.
    drew: bool,
    // Reused every frame, so a frame allocates nothing once they have grown.
    layers: Vec<FxLayer>,
    slots: Vec<FxSlot>,
    /// Per slot drawn: its instance and its range of `layers`.
    sites: Vec<(u32, Range<usize>)>,
    /// One grid's cells at this frame's clock.
    cells: Vec<TileLevel>,
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
            t,
            drew: false,
            layers: Vec::new(),
            slots: Vec::new(),
            sites: Vec::new(),
            cells: Vec::new(),
        }
    }

    /// Whether the metal stands still now; reads the system setting again once a second of `clock`.
    fn still(&mut self, clock: f64) -> bool {
        match &mut self.motion {
            Motion::Still => true,
            Motion::System { reduced, read_at } => {
                if read_at.is_none_or(|at| clock - at >= SETTING_EVERY || clock < at) {
                    *reduced = system_reduce_motion();
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
        let t = self.t as f32;
        let rt = &f.rt;
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
            // The instance is in device pixels; the layers are in points at the slot's scale.
            let k = i.params[3];
            let [x, y, w, h] = i.rect.map(|v| v / k);
            let tw = (w - grid.gap * (grid.cols - 1) as f32) / grid.cols as f32;
            let th = (h - grid.gap * (grid.rows - 1) as f32) / grid.rows as f32;
            if tw <= 0.0 || th <= 0.0 {
                continue;
            }
            let slot = FxSlot::default().scale(k).clip(i.clip, i.fade);
            grid.cells_at(t, &mut self.cells);
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
                        level(&self.cells, grid.cols, c, r),
                        shape,
                        t,
                    ));
                    self.slots.push(slot);
                }
            }
            self.sites.push((n as u32, start..self.layers.len()));
        }
    }
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
    }

    fn render(&mut self, f: &mut Frame<'_, S>) {
        if !self.still(f.rt.clock) {
            self.t += f.dt.clamp(0.0, MAX_STEP);
        }
        self.build(f);
        self.drew = !self.sites.is_empty();
        if !self.drew {
            // No grid on screen: Rune's own frame, and no effect renderer kept for nothing.
            self.fx = None;
            f.draw_ui();
            return;
        }
        let (device, queue, size) = (f.device, f.queue, f.size);
        let device_key = f.device_key();
        if self
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
        let Some(cache) = self.fx.as_mut() else {
            return;
        };
        let fx = &mut cache.fx;
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
        let clear = f.rt.clear_color();
        f.ui.prepare(device, queue, &mut f.rt.text.atlas, &f.rt.instances, size);
        let sites = &self.sites;
        f.ui.draw_with(f.encoder, f.target, clear, &mut |pass, s| {
            if let Some((_, range)) = sites.iter().find(|(n, _)| *n == s.index) {
                for k in range.clone() {
                    fx.draw_layer_in(pass, s.format, s.origin, k);
                }
            }
        });
    }
}

/// `NSWorkspace.shared.accessibilityDisplayShouldReduceMotion`.
pub fn system_reduce_motion() -> bool {
    use objc2::runtime::{AnyClass, AnyObject, Bool};
    use objc2::{msg_send, sel};
    let Some(class) = AnyClass::get(c"NSWorkspace") else {
        return false;
    };
    // SAFETY: `sharedWorkspace` returns the process-wide NSWorkspace (never nil) and
    // `accessibilityDisplayShouldReduceMotion` a BOOL; both are safe from any thread.
    unsafe {
        let workspace: *mut AnyObject = msg_send![class, sharedWorkspace];
        if workspace.is_null()
            || !(*workspace)
                .class()
                .responds_to(sel!(accessibilityDisplayShouldReduceMotion))
        {
            return false;
        }
        let reduced: Bool = msg_send![workspace, accessibilityDisplayShouldReduceMotion];
        reduced.as_bool()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_reads_row_major_and_pads_missing_cells_off() {
        let grid = TileGrid::new(
            2,
            2,
            vec![
                TileLevel::Lit,
                TileLevel::Off,
                TileLevel::Red,
                TileLevel::Lit,
            ],
        );
        let mut cells = Vec::new();
        grid.cells_at(0.0, &mut cells);
        assert_eq!(level(&cells, 2, 0, 1), TileLevel::Red);
        assert_eq!(level(&cells, 2, 1, 0), TileLevel::Off);
        assert_eq!(level(&cells, 2, 5, 5), TileLevel::Off);
    }

    #[test]
    fn a_wave_sways_with_the_clock() {
        let grid = TileGrid::wave(16, 5, Wave { urgent: true });
        let (mut a, mut b) = (Vec::new(), Vec::new());
        grid.cells_at(0.0, &mut a);
        grid.cells_at(0.7, &mut b);
        assert_eq!(a.len(), 80);
        assert_ne!(a, b, "moves with t");
        assert!(a.contains(&TileLevel::Red) && !a.contains(&TileLevel::Lit));
        assert_eq!(a, grids::island_wave_grid(0.0, true, 16, 5));
    }

    #[test]
    fn reduce_motion_flag_stops_the_metal() {
        let mut still = ChromeTiles::new(true);
        assert!(still.still(0.0));
        assert!(still.still(10.0));
        let mut system = ChromeTiles::new(false);
        assert_eq!(system.still(0.0), system_reduce_motion());
    }

    #[test]
    fn system_setting_is_read_again_after_a_second() {
        let mut tiles = ChromeTiles::new(false);
        tiles.still(5.0);
        tiles.motion = Motion::System {
            reduced: true,
            read_at: Some(5.0),
        };
        assert!(tiles.still(5.5), "kept within the second");
        tiles.still(6.0);
        assert_eq!(
            tiles.motion,
            Motion::System {
                reduced: system_reduce_motion(),
                read_at: Some(6.0)
            }
        );
    }
}
