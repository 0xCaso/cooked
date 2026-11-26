//! The soft dots (decision 2026-11-26, after Hex's Dot scope): each dot's light, 0...1, and whether it is red. The
//! chart lights its dots from the liquid's surface, the island wave from a braided pair of traces; both keep a
//! phosphor tail, so a dot fades out instead of switching off. Rows are row-major from the top (`r * cols + c`).
//!
//! The braid is a port of Hex's `Scope::beam` (anomalyco/hex, `src/rune_hud/dot_scope/scope.rs`), under its license:
//!
//! > MIT License. Copyright (c) 2026 Christopher Langton. Permission is hereby granted, free of charge, to any person
//! > obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software
//! > without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute,
//! > sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so,
//! > subject to the following conditions: The above copyright notice and this permission notice shall be included in
//! > all copies or substantial portions of the Software. THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
//! > KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
//! > PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
//! > CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
//! > IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

use std::f32::consts::TAU;

/// How long a dot's light takes to fall to 1/e once nothing lights it, seconds.
pub const TAIL: f32 = 0.6;
/// The chart's tail while it moves to a new range: the old range's dots go at once.
pub const SWITCH_TAIL: f32 = 0.06;
/// The braid's bright head fades faster than its tail, seconds.
pub const HEAD: f32 = 0.15;

/// One dot: its light, 0...1, and whether it is red.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Dot {
    pub power: f32,
    pub red: bool,
}

/// The light of the chart's dot `row` rows up from the bottom (0 = bottom row) in a column whose surface stands
/// `height` rows high: the part of the dot under the surface, so the edge moves smoothly instead of row by row,
/// brightest at the surface like a lit meniscus and dimmer deeper down.
pub fn chart_light(height: f64, row: usize) -> f32 {
    let j = row as f64;
    let fill = (height - j).clamp(0.0, 1.0);
    let surface = (-(height - 0.5 - j).powi(2) / 0.9).exp();
    let depth = (-(height - j).max(0.0) / 7.0).exp();
    (fill * (0.3 + 0.35 * depth + 0.45 * surface)) as f32
}

/// The chart's dots with their phosphor.
#[derive(Debug, Clone, Default)]
pub struct ChartDots {
    glow: Vec<f32>,
    /// Each dot's colour when its light was last set: a fading dot keeps it.
    red: Vec<bool>,
    /// The chart is moving to a new range: `SWITCH_TAIL` instead of `TAIL`.
    pub switching: bool,
}

impl ChartDots {
    /// The dots for column heights `heights` (rows, fractional) after `dt` seconds; `dt` of `None` stands still (no
    /// tail: each dot at its light). Red: every lit dot when `urgent`, else the dots above `threshold_row` (rows from
    /// the top) in the columns that `reaches`; a fading dot keeps the colour it was lit with.
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        heights: &[f64],
        reaches: &[bool],
        threshold_row: usize,
        urgent: bool,
        cols: usize,
        rows: usize,
        dt: Option<f32>,
        out: &mut Vec<Dot>,
    ) {
        let n = cols * rows;
        if self.glow.len() != n {
            self.glow = vec![0.0; n];
            self.red = vec![false; n];
        }
        let tail = if self.switching { SWITCH_TAIL } else { TAIL };
        let keep = dt.map_or(0.0, |dt| (-dt.max(0.0) / tail).exp());
        out.clear();
        out.resize(n, Dot::default());
        for c in 0..cols {
            let h = heights.get(c).copied().unwrap_or(0.0);
            let over = reaches.get(c).copied().unwrap_or(false);
            for r in 0..rows {
                let i = r * cols + c;
                let light = chart_light(h, rows - 1 - r);
                let fading = self.glow[i] * keep;
                if light >= fading {
                    self.glow[i] = light;
                    self.red[i] = urgent || (over && r < threshold_row);
                } else {
                    self.glow[i] = fading;
                }
                out[i] = Dot {
                    power: self.glow[i],
                    red: self.red[i],
                };
            }
        }
    }
}

/// History samples a second, and how many the braid keeps (5 s).
const HISTORY_RATE: f32 = 120.0;
const HISTORY: usize = 600;
/// The braid's four components: speeds (Hz), weights, and waves across the field.
const SPEEDS: [f32; 4] = [0.8, 1.25, 1.85, 2.6];
const WEIGHTS: [f32; 4] = [0.55, 0.42, 0.3, 0.2];
const SPATIAL: [f32; 4] = [1.4, 2.4, 3.8, 5.6];
const PHASES: [[f32; 4]; 2] = [[0.0, 1.1, 2.3, 4.0], [3.1, 0.4, 5.0, 1.9]];
/// How fast the level's history travels out from the middle, points a second.
const TRAVEL: f32 = 150.0;
/// The breath that keeps the braid alive at rest, seconds a cycle.
const BREATH: f32 = 5.5;
/// Samples per column along each trace.
const SUB: usize = 3;

/// The island wave: two traces braided around the middle row, their swing following the rate's 0...1 level, which
/// travels out from the middle; the dots near a trace light up and fade with a phosphor tail.
#[derive(Debug, Clone)]
pub struct Braid {
    pub cols: usize,
    pub rows: usize,
    /// Between dots, points.
    pub pitch: f32,
    history: Vec<f32>,
    head: usize,
    level: f32,
    carrier: f32,
    phases: [[f32; 4]; 2],
    head_glow: [Vec<f32>; 2],
    tail_glow: [Vec<f32>; 2],
    traces: [Vec<f32>; 2],
    bright: [Vec<f32>; 2],
    power: Vec<f32>,
}

impl Braid {
    pub fn new(cols: usize, rows: usize, pitch: f32) -> Self {
        let n = cols * rows;
        let samples = cols.saturating_sub(1) * SUB + 1;
        Self {
            cols,
            rows,
            pitch,
            history: vec![0.0; HISTORY],
            head: 0,
            level: 0.0,
            carrier: 0.0,
            phases: PHASES,
            head_glow: [vec![0.0; n], vec![0.0; n]],
            tail_glow: [vec![0.0; n], vec![0.0; n]],
            traces: [vec![0.0; samples], vec![0.0; samples]],
            bright: [vec![0.0; samples], vec![0.0; samples]],
            power: vec![0.0; n],
        }
    }

    /// Each dot's light, row-major from the top.
    pub fn powers(&self) -> &[f32] {
        &self.power
    }

    /// Moves the braid on by `dt` seconds toward `level` (0...1); `t` is the clock the breath follows.
    pub fn step(&mut self, t: f32, dt: f32, level: f32) {
        let dt = dt.max(0.0);
        let target = level.clamp(0.0, 1.0);
        let lag = if target > self.level { 0.05 } else { 0.25 };
        self.level += (target - self.level) * (1.0 - (-dt / lag).exp());
        let pushes = ((dt * HISTORY_RATE).round() as usize).max(1);
        for _ in 0..pushes.min(HISTORY) {
            self.history[self.head] = self.level;
            self.head = (self.head + 1) % HISTORY;
        }
        let go = Self::go(self.level);
        self.carrier = (self.carrier + dt * TAU * Self::carrier_hz(self.level)) % TAU;
        for trace in &mut self.phases {
            for (c, phase) in trace.iter_mut().enumerate() {
                *phase = (*phase + dt * TAU * SPEEDS[c] * go) % TAU;
            }
        }
        self.trace(t);
        self.light(Some(((-dt / HEAD).exp(), (-dt / TAIL).exp())));
    }

    /// The braid standing at `level` at time `t`, with no tail: Reduce Motion and snapshots.
    pub fn settle(&mut self, t: f32, level: f32) {
        self.level = level.clamp(0.0, 1.0);
        self.history.fill(self.level);
        let go = Self::go(self.level);
        self.carrier = (t * TAU * Self::carrier_hz(self.level)) % TAU;
        for (trace, start) in self.phases.iter_mut().zip(PHASES) {
            for c in 0..4 {
                trace[c] = (start[c] + t * TAU * SPEEDS[c] * go) % TAU;
            }
        }
        self.trace(t);
        self.light(None);
    }

    fn go(level: f32) -> f32 {
        0.6 + 1.1 * (level * 1.6).min(1.0)
    }

    fn carrier_hz(level: f32) -> f32 {
        0.14 + 0.5 * level
    }

    /// The level `delay` seconds ago.
    fn past(&self, delay: f32) -> f32 {
        let back = ((delay * HISTORY_RATE).round() as usize).min(HISTORY - 2);
        self.history[(self.head + 2 * HISTORY - 1 - back) % HISTORY]
    }

    /// Both traces' rows and brightness at each sample.
    fn trace(&mut self, t: f32) {
        let (cols, rows, pitch) = (self.cols, self.rows, self.pitch);
        if cols < 2 || rows == 0 {
            return;
        }
        let width = (cols - 1) as f32 * pitch;
        let hw = width / 2.0;
        let mid = (rows - 1) as f32 / 2.0;
        let half = (mid - 0.55).max(0.1);
        let breath = 0.5 + 0.5 * (t * TAU / BREATH).sin();
        let rest = (0.55 + 0.35 * breath) * 0.4;
        let samples = self.traces[0].len();
        for k in 0..2 {
            let sign = if k == 0 { 1.0 } else { -1.0 };
            for s in 0..samples {
                let x = (s as f32 / SUB as f32 - (cols - 1) as f32 / 2.0) * pitch;
                let xn = (x + hw) / width;
                let ends = ((x + hw - 2.0) / 14.0)
                    .max(0.0)
                    .min(((hw - x - 2.0) / 14.0).max(0.0))
                    .min(1.0);
                let a = self.past(x.abs() / TRAVEL);
                let reach = 1.0 - 0.8 * ((x.abs() - 14.0) / (hw - 14.0).max(1.0)).clamp(0.0, 1.0);
                let spread: f32 = (0..4)
                    .map(|c| a * WEIGHTS[c] * (TAU * SPATIAL[c] * xn - self.phases[k][c]).sin())
                    .sum();
                let helix =
                    (rest + half * 0.55 * a * reach) * (TAU * 1.6 * xn - self.carrier).sin();
                self.traces[k][s] = mid
                    - sign * half * ((helix + half * 0.7 * spread * reach) / half).tanh() * ends;
                self.bright[k][s] = (0.3 + 0.7 * (a * 1.8).min(1.0)) * (0.45 + 0.55 * ends);
            }
        }
    }

    /// Lights each dot from its distance to the nearest stretch of each trace; `keep` is the head's and the tail's
    /// share left after this step, `None` for no phosphor.
    fn light(&mut self, keep: Option<(f32, f32)>) {
        let (cols, rows) = (self.cols, self.rows);
        let samples = self.traces[0].len();
        if samples < 2 {
            return;
        }
        let dx = 1.0 / SUB as f32;
        for j in 0..rows {
            for i in 0..cols {
                let n = j * cols + i;
                let mut lit = [0.0f32; 2];
                for (k, lit_k) in lit.iter_mut().enumerate() {
                    let ys = &self.traces[k];
                    let (mut best, mut near) = (f32::MAX, i * SUB);
                    let from = i.saturating_sub(3) * SUB;
                    let to = ((i + 3) * SUB).min(samples - 1);
                    for s in from..to {
                        let dy = ys[s + 1] - ys[s];
                        let (px, py) = (i as f32 - s as f32 * dx, j as f32 - ys[s]);
                        let u = ((px * dx + py * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
                        let (ex, ey) = (px - u * dx, py - u * dy);
                        let d2 = ex * ex + ey * ey;
                        if d2 < best {
                            best = d2;
                            near = s;
                        }
                    }
                    let b = self.bright[k][near];
                    let on = (-best / 0.25).exp();
                    let instant = (on + 0.5 * (-best.sqrt() / 0.8).exp()).min(1.2) * b;
                    let (head, tail) = match keep {
                        Some((h, t)) => {
                            self.head_glow[k][n] = (self.head_glow[k][n] * h).max(on * b);
                            self.tail_glow[k][n] = (self.tail_glow[k][n] * t).max(on * b);
                            (self.head_glow[k][n], self.tail_glow[k][n])
                        }
                        None => {
                            self.head_glow[k][n] = on * b;
                            self.tail_glow[k][n] = on * b;
                            (on * b, on * b)
                        }
                    };
                    *lit_k = instant.max(head).max(0.55 * tail).min(1.0);
                }
                self.power[n] = 1.0 - (1.0 - lit[0]) * (1.0 - lit[1]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_top_dot_is_lit_by_its_part_under_the_surface() {
        // a column 3.5 rows high: rows 0-2 under, row 3 half under, row 4 dry
        assert!(chart_light(3.5, 4) == 0.0);
        let half = chart_light(3.5, 3);
        let full = chart_light(4.0, 3);
        assert!(half > 0.0 && half < full, "{half} {full}");
        // moving the surface a tenth of a row moves the light a little, not a whole row's step
        let step = (chart_light(3.6, 3) - chart_light(3.5, 3)).abs();
        assert!(step < 0.12, "{step}");
    }

    #[test]
    fn dots_are_brightest_at_the_surface_and_dim_deeper_down() {
        let h = 12.0;
        let surface = chart_light(h, 11);
        let deep = chart_light(h, 0);
        assert!(surface > deep, "{surface} {deep}");
        assert!(deep >= 0.3, "deep dots stay visible: {deep}");
        assert!(surface <= 1.0);
    }

    #[test]
    fn a_new_range_drops_the_old_dots_at_once() {
        let (cols, rows) = (2, 10);
        let fade = |switching: bool| {
            let mut dots = ChartDots {
                switching,
                ..ChartDots::default()
            };
            let mut out = Vec::new();
            let none = [false, false];
            dots.step(&[8.0, 8.0], &none, 3, false, cols, rows, None, &mut out);
            dots.step(
                &[2.0, 2.0],
                &none,
                3,
                false,
                cols,
                rows,
                Some(0.25),
                &mut out,
            );
            out[3 * cols].power
        };
        // a quarter second after the switch the old surface is gone; within a range it still glows
        assert!(fade(true) < 0.05, "{}", fade(true));
        assert!(fade(false) > 0.3, "{}", fade(false));
    }

    #[test]
    fn a_fading_dot_keeps_the_colour_it_was_lit_with() {
        let (cols, rows) = (1, 10);
        let mut dots = ChartDots::default();
        let mut out = Vec::new();
        // a column over the threshold: its top dots red
        dots.step(&[10.0], &[true], 3, false, cols, rows, None, &mut out);
        assert!(out[0].red);
        // it drops under the threshold: the top dot fades, still red, while the dots it now lights are white
        dots.step(&[2.0], &[false], 3, false, cols, rows, Some(0.1), &mut out);
        assert!(out[0].power > 0.3 && out[0].red, "{:?}", out[0]);
        assert!(!out[rows - 1].red, "{:?}", out[rows - 1]);
    }

    #[test]
    fn chart_dots_fade_with_a_tail_and_stand_still_without_dt() {
        let (cols, rows) = (2, 10);
        let mut dots = ChartDots::default();
        let mut out = Vec::new();
        dots.step(
            &[8.0, 8.0],
            &[false, false],
            3,
            false,
            cols,
            rows,
            None,
            &mut out,
        );
        let top_lit = out[3 * cols].power;
        assert!(top_lit > 0.5);
        // the surface drops to 2 rows: the dot fades, it does not switch off
        dots.step(
            &[2.0, 2.0],
            &[false, false],
            3,
            false,
            cols,
            rows,
            Some(0.1),
            &mut out,
        );
        let after = out[3 * cols].power;
        assert!(after > 0.0 && after < top_lit, "{after} {top_lit}");
        let expected = top_lit * (-0.1f32 / TAIL).exp();
        assert!((after - expected).abs() < 1e-4);
        // standing still: at its light at once
        dots.step(
            &[2.0, 2.0],
            &[false, false],
            3,
            false,
            cols,
            rows,
            None,
            &mut out,
        );
        assert_eq!(out[3 * cols].power, 0.0);
    }

    #[test]
    fn chart_dots_are_red_above_the_threshold_in_reaching_columns_or_all_in_urgency() {
        let (cols, rows) = (2, 10);
        let mut dots = ChartDots::default();
        let mut out = Vec::new();
        dots.step(
            &[10.0, 10.0],
            &[true, false],
            3,
            false,
            cols,
            rows,
            None,
            &mut out,
        );
        assert!(out[0].red, "top of a reaching column");
        assert!(!out[1].red, "column that does not reach");
        assert!(!out[3 * cols].red, "under the threshold row");
        dots.step(
            &[10.0, 10.0],
            &[false, false],
            3,
            true,
            cols,
            rows,
            None,
            &mut out,
        );
        assert!(out.iter().all(|d| d.red));
    }

    fn lit_rows(b: &Braid) -> (usize, usize) {
        let rows: Vec<usize> = (0..b.rows)
            .filter(|j| (0..b.cols).any(|i| b.powers()[j * b.cols + i] > 0.5))
            .collect();
        (*rows.first().unwrap_or(&0), *rows.last().unwrap_or(&0))
    }

    #[test]
    fn the_braid_at_rest_is_a_thin_band_and_widens_with_the_level() {
        let mut quiet = Braid::new(40, 9, 2.6);
        quiet.settle(1.0, 0.0);
        let (q0, q1) = lit_rows(&quiet);
        let mut loud = Braid::new(40, 9, 2.6);
        loud.settle(1.0, 1.0);
        let (l0, l1) = lit_rows(&loud);
        assert!(q1 - q0 <= 4, "rest {q0}..{q1}");
        assert!(l1 - l0 > q1 - q0, "loud {l0}..{l1} vs rest {q0}..{q1}");
        assert!(quiet.powers().iter().all(|p| (0.0..=1.0).contains(p)));
    }

    #[test]
    fn the_braid_moves_and_settle_is_repeatable() {
        let (mut a, mut b) = (Braid::new(40, 9, 2.6), Braid::new(40, 9, 2.6));
        a.settle(2.0, 0.5);
        b.settle(2.0, 0.5);
        assert_eq!(a.powers(), b.powers());
        b.settle(2.4, 0.5);
        assert_ne!(a.powers(), b.powers());
    }

    #[test]
    fn a_rise_in_level_travels_out_from_the_middle() {
        let mut b = Braid::new(40, 9, 2.6);
        b.settle(0.0, 0.0);
        for k in 0..6 {
            b.step(k as f32 / 60.0, 1.0 / 60.0, 1.0);
        }
        // 0.1 s after the rise the middle feels it, the ends (50 pt out) not yet
        let (middle, end) = (b.past(0.0), b.past(50.0 / TRAVEL));
        assert!(middle > 0.5 && end < 0.05, "{middle} {end}");
    }
}
