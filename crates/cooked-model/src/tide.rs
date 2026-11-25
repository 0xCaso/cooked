//! The charts are a liquid. Every column is a spring pulled to its target and tied to its
//! neighbours, so a new value flows into place instead of being redrawn. `LiquidShade` keeps the opacity, the white
//! core and the red flag; the LED radius and glow are dropped (chrome tiles have neither).

use crate::dot_shading::smoothstep;
use crate::histogram_clock::MinuteHistory;

pub fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

/// Runs a simulation in fixed 240 Hz steps, so the picture does not depend on the frame rate.
/// A stall counts as at most 1/15 s, so it does not explode.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FixedStepper {
    last: Option<f64>,
    pending: f64,
}

impl FixedStepper {
    pub const STEP: f64 = 1.0 / 240.0;

    /// Steps run for the time between the previous call and `t`.
    pub fn advance(&mut self, t: f64, mut step: impl FnMut(f64)) {
        self.pending += (1.0 / 15.0f64).min((t - self.last.unwrap_or(t)).max(0.0));
        self.last = Some(t);
        while self.pending >= Self::STEP {
            step(Self::STEP);
            self.pending -= Self::STEP;
        }
    }

    /// Runs `seconds` of steps at once, so the first frame already shows a settled picture.
    pub fn warm(seconds: f64, mut step: impl FnMut(f64)) {
        for _ in 0..(seconds / Self::STEP) as usize {
            step(Self::STEP);
        }
    }
}

/// A row of columns: each one a spring with stiffness `k` to its target, `c` to its neighbours, damping `d`.
#[derive(Debug, Clone, PartialEq)]
pub struct Water {
    pub k: f64,
    pub c: f64,
    pub d: f64,
    pub height: Vec<f64>,
    pub velocity: Vec<f64>,
}

impl Water {
    pub fn new(cols: usize, k: f64, c: f64, d: f64, height: f64) -> Self {
        Self {
            k,
            c,
            d,
            height: vec![height; cols],
            velocity: vec![0.0; cols],
        }
    }

    pub fn step(&mut self, dt: f64, target: &[f64]) {
        let n = self.height.len();
        for i in 0..n {
            let l = self.height[i.saturating_sub(1)];
            let r = self.height[(i + 1).min(n - 1)];
            let t = target.get(i).copied().unwrap_or(0.0);
            self.velocity[i] += (self.k * (t - self.height[i])
                + self.c * (l + r - 2.0 * self.height[i])
                - self.d * self.velocity[i])
                * dt;
        }
        for i in 0..n {
            self.height[i] += self.velocity[i] * dt;
        }
    }
}

/// The square-root y scale: one huge minute no longer flattens the other 59.
pub struct TideScale;

impl TideScale {
    pub fn y(v: f64, top: f64) -> f64 {
        if top > 0.0 {
            (v.max(0.0) / top).sqrt().min(1.0)
        } else {
            0.0
        }
    }

    /// The row (from the top) the threshold sits on.
    pub fn threshold_row(rows: usize, threshold: f64, top: f64) -> usize {
        rows.saturating_sub((Self::y(threshold, top) * rows as f64).round() as usize)
    }

    /// Minutes → column heights in rows. Each column shows the average of the minutes it covers, weighted by
    /// overlap, so a single minute is as wide on screen as it is in time. Never below 0.4 rows: the floor is wet.
    pub fn targets(minutes: &[f64], cols: usize, rows: usize, top: f64) -> Vec<f64> {
        let n = minutes.len();
        if n == 0 {
            return vec![0.4; cols];
        }
        let span = n as f64 / cols as f64;
        (0..cols)
            .map(|c| {
                let a = c as f64 * span;
                let b = a + span;
                let mut sum = 0.0;
                let mut m = a as usize;
                while (m as f64) < b && m < n {
                    sum += minutes[m] * (b.min((m + 1) as f64) - a.max(m as f64));
                    m += 1;
                }
                (Self::y(sum / span, top) * rows as f64).max(0.4)
            })
            .collect()
    }
}

impl TideScale {
    /// `targets`, except where a minute spans `FINE_SPAN` columns or more (10m, 30m): there the columns follow a
    /// line between the minutes' centres, so the chart varies column by column (the approved mockup) instead of
    /// standing in flat steps one minute wide. Coarser ranges keep `targets` as is.
    pub fn fine_targets(minutes: &[f64], cols: usize, rows: usize, top: f64) -> Vec<f64> {
        let n = minutes.len();
        if n == 0 || (cols as f64) < Self::FINE_SPAN * n as f64 {
            return Self::targets(minutes, cols, rows, top);
        }
        let span = n as f64 / cols as f64;
        (0..cols)
            .map(|c| {
                let x = ((c as f64 + 0.5) * span - 0.5).clamp(0.0, (n - 1) as f64);
                let i = x.floor() as usize;
                let j = (i + 1).min(n - 1);
                let f = x - i as f64;
                let v = minutes[i] * (1.0 - f) + minutes[j] * f;
                (Self::y(v, top) * rows as f64).max(0.4)
            })
            .collect()
    }

    /// Columns per minute from which `fine_targets` draws lines between minutes.
    pub const FINE_SPAN: f64 = 2.0;
}

/// How bright the liquid under the meniscus is: `base + gain·e^(−depth/scale)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiquidBody {
    pub base: f64,
    pub gain: f64,
    pub scale: f64,
}

impl LiquidBody {
    /// The island's tank: 20–52% light.
    pub const TANK: LiquidBody = LiquidBody {
        base: 0.2,
        gain: 0.32,
        scale: 4.0,
    };
    /// The popover's chart, brighter: 34–70% light.
    pub const CHART: LiquidBody = LiquidBody {
        base: 0.34,
        gain: 0.36,
        scale: 5.0,
    };

    pub fn light(&self, depth: f64) -> f64 {
        if depth > 0.0 {
            self.base + self.gain * (-depth / self.scale).exp()
        } else {
            0.0
        }
    }
}

/// A grid of dots, row 0 at the top, index `r * cols + c`. `level` 0...1 is brightness,
/// `fresh` 0...1 how white the dot's core is, `hot` coral, `lift` how far the dot sits above its grid place, in points.
#[derive(Debug, Clone, PartialEq)]
pub struct LiquidField {
    pub cols: usize,
    pub rows: usize,
    pub level: Vec<f32>,
    pub fresh: Vec<f32>,
    pub hot: Vec<bool>,
    pub lift: Vec<f32>,
}

impl LiquidField {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            cols,
            rows,
            level: vec![0.0; cols * rows],
            fresh: vec![0.0; cols * rows],
            hot: vec![false; cols * rows],
            lift: vec![0.0; cols * rows],
        }
    }

    /// The surface row is bright, the liquid below fades with depth; where the surface moves fast it catches
    /// more light. Dots above `hot_row` are coral when `urgent`; with `reaches`, only in the columns whose real value
    /// reaches that row, so a spring overshooting the threshold never flashes coral.
    pub fn pour(
        &mut self,
        water: &Water,
        hot_row: usize,
        urgent: bool,
        still: bool,
        reaches: Option<&[bool]>,
        liquid: LiquidBody,
    ) {
        for c in 0..self.cols {
            let h = water.height[c];
            let speed = if still {
                0.0
            } else {
                (water.velocity[c].abs() * 0.05).min(0.35)
            };
            let reach = reaches.and_then(|r| r.get(c).copied()) != Some(false);
            for r in 0..self.rows {
                let i = r * self.cols + c;
                let depth = h - ((self.rows - 1 - r) as f64 + 0.5);
                // the meniscus: one row, its edge anti-aliased
                let top = clamp01(depth + 0.5) * (1.0 - smoothstep(0.5, 1.6, depth as f32) as f64);
                let body = liquid.light(depth);
                self.level[i] = (top * (0.8 + speed)).max(body).min(1.0) as f32;
                self.fresh[i] = if top > 0.5 { 0.6 + speed } else { 0.15 } as f32;
                self.hot[i] = urgent && r < hot_row && reach;
            }
        }
    }
}

/// Park–Miller, like the mockup: the same drops on every run.
fn park_miller(seed: &mut u64) -> f64 {
    *seed = (*seed * 16807) % 2_147_483_647;
    *seed as f64 / 2_147_483_647.0
}

/// The island's tank: the level follows the token rate, and tokens fall in as drops that dent the surface.
#[derive(Debug, Clone, PartialEq)]
pub struct TideIsland {
    water: Water,
    stepper: FixedStepper,
    level: f64,
    drip: f64,
    seed: u64,
    warmed: bool,
    field: LiquidField,
}

impl Default for TideIsland {
    fn default() -> Self {
        Self::new()
    }
}

impl TideIsland {
    pub const COLS: usize = 36;
    pub const ROWS: usize = 9;
    /// The threshold sits at 75% of the tank.
    pub const THRESHOLD_SHARE: f64 = 0.75;

    pub fn new() -> Self {
        Self {
            water: Water::new(Self::COLS, 30.0, 900.0, 5.0, 2.0),
            stepper: FixedStepper::default(),
            level: 0.0,
            drip: 0.0,
            seed: 3,
            warmed: false,
            field: LiquidField::new(Self::COLS, Self::ROWS),
        }
    }

    pub fn threshold_row() -> usize {
        Self::ROWS - Self::surface(Self::THRESHOLD_SHARE).round() as usize
    }

    /// Height in rows of a 0...1 level: never empty, never over the brim.
    pub fn surface(level: f64) -> f64 {
        1.2 + clamp01(level) * (Self::ROWS as f64 - 2.4)
    }

    /// Rate → 0...1 level: the threshold sits at 75% of the tank.
    pub fn level(rate: f64, threshold: f64) -> f64 {
        if threshold > 0.0 {
            clamp01(rate / threshold * Self::THRESHOLD_SHARE)
        } else {
            0.0
        }
    }

    pub fn field(&self) -> &LiquidField {
        &self.field
    }

    pub fn frame(&mut self, t: f64, level: f64, urgent: bool, still: bool) -> &LiquidField {
        self.level = level;
        if still {
            self.water.height = vec![Self::surface(level); Self::COLS];
            self.water.velocity = vec![0.0; Self::COLS];
        } else if !self.warmed {
            self.water.height = vec![Self::surface(level); Self::COLS];
            FixedStepper::warm(3.0, |dt| self.step(dt));
            self.warmed = true;
            self.stepper.advance(t, |_| {});
        } else {
            let mut stepper = self.stepper;
            stepper.advance(t, |dt| self.step(dt));
            self.stepper = stepper;
        }
        self.field.pour(
            &self.water,
            Self::threshold_row(),
            urgent,
            still,
            None,
            LiquidBody::TANK,
        );
        &self.field
    }

    fn step(&mut self, dt: f64) {
        let target = vec![Self::surface(self.level); Self::COLS];
        self.drip += dt * (2.0 + 22.0 * self.level);
        while self.drip > 1.0 {
            self.drip -= 1.0;
            let c = (park_miller(&mut self.seed) * Self::COLS as f64) as usize;
            self.water.velocity[c.min(Self::COLS - 1)] += -(14.0 + 18.0 * self.level);
        }
        self.water.step(dt, &target);
    }
}

/// How the histogram's ripple layer moves: `CALM` for the long ranges, `LIVELY` for 10 minutes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Motion {
    /// The ripple layer runs at this share of real time.
    pub slow: f64,
    /// Drops per second of ripple time at a 0...1 level (base, per level).
    pub drop_rate: (f64, f64),
    /// How hard a drop hits: base plus per-level times the level, or times a random share when not `follows_level`.
    pub kick: (f64, f64),
    pub follows_level: bool,
    /// The ripples never move a column further than this, in rows.
    pub max_ripple: f64,
}

impl Motion {
    /// 40% slower than the island, 1.6 drops a second whatever the data.
    pub const CALM: Motion = Motion {
        slow: 0.6,
        drop_rate: (1.6, 0.0),
        kick: (8.0, 6.0),
        follows_level: false,
        max_ripple: 0.6,
    };
    /// The island's drops: more and harder as tokens flow in (`TideIsland`).
    pub const LIVELY: Motion = Motion {
        slow: 1.0,
        drop_rate: (2.0, 22.0),
        kick: (14.0, 18.0),
        follows_level: true,
        max_ripple: 1.2,
    };
}

/// The popover's minutes as a liquid: when a minute closes the targets jump, and the surface flows into its
/// new shape. Slow drops keep it alive while nothing changes, like the island: they fall on a separate ripple layer,
/// added to the data's heights only when drawn, so the data's columns stay where the minutes put them. The ripple
/// reaches into the body: the dots under a moving surface rise and sink with it, less with depth.
#[derive(Debug, Clone, PartialEq)]
pub struct TideHistogram {
    cols: usize,
    rows: usize,
    /// `TideScale::fine_targets` instead of `targets`.
    fine: bool,
    water: Water,
    ripple: Water,
    stepper: FixedStepper,
    target: Vec<f64>,
    primed: bool,
    drip: f64,
    motion: Motion,
    /// 0...1, the last minutes' rate as a share of the tank.
    level: f64,
    seed: u64,
    threshold_row: usize,
    field: LiquidField,
    /// The column heights in rows as drawn, ripples included.
    shown_heights: Vec<f64>,
    /// Per column, whether its real value is over the threshold row.
    reaches: Vec<bool>,
    lit_rows: Vec<f64>,
}

impl Default for TideHistogram {
    fn default() -> Self {
        Self::new()
    }
}

impl TideHistogram {
    pub const COLS: usize = 84;
    pub const ROWS: usize = 24;
    pub const PITCH: f64 = 348.0 / Self::COLS as f64;
    /// A body dot never moves further than this from its grid place, in points.
    pub const MAX_LIFT: f64 = 0.5;
    /// Rows under the surface over which a body dot's movement fades by e.
    pub const LIFT_DEPTH: f64 = 6.0;
    /// Rows past half a row the surface must move before a column's lit rows change: a ripple rocking the surface
    /// around half a row would otherwise blink its top tile on and off several times a second.
    pub const TILE_HOLD: f64 = 0.25;

    pub fn new() -> Self {
        Self::sized(Self::COLS, Self::ROWS, false)
    }

    /// The same liquid on `cols` x `rows` (the Rune popover's 72 x 26 tiles); `fine`: lines between minutes where
    /// one spans two columns or more (`TideScale::fine_targets`).
    pub fn sized(cols: usize, rows: usize, fine: bool) -> Self {
        Self {
            cols,
            rows,
            fine,
            water: Water::new(cols, 120.0, 110.0, 9.0, 0.0),
            ripple: Water::new(cols, 30.0, 900.0, 5.0, 0.0),
            stepper: FixedStepper::default(),
            target: Vec::new(),
            primed: false,
            drip: 0.0,
            motion: Motion::CALM,
            level: 0.0,
            seed: 5,
            threshold_row: 0,
            field: LiquidField::new(cols, rows),
            shown_heights: vec![0.0; cols],
            reaches: vec![false; cols],
            lit_rows: vec![0.0; cols],
        }
    }

    pub fn threshold_row(&self) -> usize {
        self.threshold_row
    }

    /// Per column, the whole rows its tiles light.
    pub fn lit_rows(&self) -> &[f64] {
        &self.lit_rows
    }

    pub fn field(&self) -> &LiquidField {
        &self.field
    }

    /// The column heights in rows as drawn, ripples included.
    pub fn shown_heights(&self) -> &[f64] {
        &self.shown_heights
    }

    /// The column heights in rows.
    pub fn heights(&self) -> &[f64] {
        &self.water.height
    }

    /// Per column, whether its real value is over the threshold row (where red may show).
    pub fn reaches(&self) -> &[bool] {
        &self.reaches
    }

    /// Coral above the threshold row in every column whose real value is over it, past minutes included: the
    /// chart shows when usage went over, not only whether it is over now. The island's tank keeps the `urgent` rule.
    pub fn frame(
        &mut self,
        history: &MinuteHistory,
        t: f64,
        still: bool,
        motion: Motion,
    ) -> &LiquidField {
        let shown = self.flow(history, t, still, motion);
        self.field.pour(
            &shown,
            self.threshold_row,
            true,
            still,
            Some(&self.reaches),
            LiquidBody::CHART,
        );
        for c in 0..self.cols {
            let moved = shown.height[c] - self.water.height[c];
            for r in 0..self.rows {
                let depth = shown.height[c] - ((self.rows - 1 - r) as f64 + 0.5);
                self.field.lift[r * self.cols + c] =
                    Self::lift(moved, depth, motion.max_ripple) as f32;
            }
        }
        &self.field
    }

    /// The liquid at `t` without its dots: the surface as drawn, ripples included (`shown_heights`), and which
    /// columns reach the threshold row (`reaches`). The chrome tiles need only these.
    pub fn surface(&mut self, history: &MinuteHistory, t: f64, still: bool, motion: Motion) {
        self.flow(history, t, still, motion);
    }

    fn flow(&mut self, history: &MinuteHistory, t: f64, still: bool, motion: Motion) -> Water {
        let (cols, rows) = (self.cols, self.rows);
        self.motion = motion;
        // the last minutes' rate as the island's level: the busier, the more drops
        let recent = &history.values[history.values.len().saturating_sub(3)..];
        let rate = recent.iter().sum::<f64>() / recent.len().max(1) as f64;
        self.level = TideIsland::level(rate, history.threshold);
        self.target = if self.fine {
            TideScale::fine_targets(&history.values, cols, rows, history.top)
        } else {
            TideScale::targets(&history.values, cols, rows, history.top)
        };
        self.threshold_row = TideScale::threshold_row(rows, history.threshold, history.top);
        let settled = still || !self.primed;
        if settled {
            self.water.height = self.target.clone();
            self.water.velocity = vec![0.0; cols];
            self.ripple = Water::new(cols, self.ripple.k, self.ripple.c, self.ripple.d, 0.0);
            self.primed = true;
            self.stepper.advance(t, |_| {});
        } else {
            let mut stepper = self.stepper;
            stepper.advance(t, |dt| self.step(dt));
            self.stepper = stepper;
        }
        let line = (rows - self.threshold_row) as f64;
        let mut shown = self.water.clone();
        for c in 0..cols {
            let h = self.water.height[c];
            let r = self.ripple.height[c].clamp(-motion.max_ripple, motion.max_ripple);
            // a ripple never lifts a column into the threshold row, and never below the floor
            shown.height[c] = if r > 0.0 {
                (h + r).min(h.max(line - 1.0))
            } else {
                h + r
            }
            .max(0.0);
            shown.velocity[c] += self.ripple.velocity[c] * motion.slow * 0.1;
        }
        self.shown_heights.clone_from(&shown.height);
        for (lit, h) in self.lit_rows.iter_mut().zip(&shown.height) {
            if settled || (h - *lit).abs() > 0.5 + Self::TILE_HOLD {
                *lit = h.round();
            }
        }
        self.reaches.clear();
        self.reaches.extend(self.target.iter().map(|t| *t > line));
        shown
    }

    /// Points a dot `depth` rows under the surface rises (negative: sinks) when its column's surface moved by `moved`
    /// rows: the whole ripple's share of 0.5 pt, fading with depth; dry dots stay put.
    pub fn lift(moved: f64, depth: f64, max_ripple: f64) -> f64 {
        if depth <= 0.0 {
            return 0.0;
        }
        (moved / max_ripple).clamp(-1.0, 1.0) * Self::MAX_LIFT * (-depth / Self::LIFT_DEPTH).exp()
    }

    fn step(&mut self, dt: f64) {
        self.water.step(dt, &self.target);
        let m = self.motion;
        let slowed = dt * m.slow;
        self.drip += slowed * (m.drop_rate.0 + m.drop_rate.1 * self.level);
        while self.drip > 1.0 {
            self.drip -= 1.0;
            let share = if m.follows_level {
                self.level
            } else {
                park_miller(&mut self.seed)
            };
            let hit = m.kick.0 + m.kick.1 * share;
            let c = (park_miller(&mut self.seed) * self.cols as f64) as usize;
            self.ripple.velocity[c.min(self.cols - 1)] -= hit;
        }
        // no target: the ripples settle back to zero
        self.ripple.step(slowed, &[]);
    }
}

/// How one liquid dot is drawn (the mockup's `paint`), without the LED radius and glow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiquidShade {
    pub alpha: f32,
    /// 0 the body colour, 1 the white core.
    pub fresh: f32,
    pub hot: bool,
}

impl LiquidShade {
    /// `rim_cols`: how many columns fade in from each side (0 for none). `floor`: the light of an unlit dot.
    pub fn shade(f: &LiquidField, i: usize, floor: f32, rim_cols: usize) -> LiquidShade {
        let c = i % f.cols;
        let v = f.level[i].clamp(0.0, 1.0);
        let rim = if rim_cols > 0 {
            smoothstep(
                0.0,
                1.0,
                (c.min(f.cols - 1 - c) + 1) as f32 / rim_cols as f32,
            )
        } else {
            1.0
        };
        LiquidShade {
            alpha: (floor + v * (1.0 - floor)) * rim,
            fresh: f.fresh[i].min(1.0),
            hot: f.hot[i] && v > 0.04,
        }
    }
}
