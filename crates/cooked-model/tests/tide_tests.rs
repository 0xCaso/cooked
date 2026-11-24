//! `TideTests.swift`: the charts as a liquid. `shadeFadesTheRimAndCapsTheGlow` keeps its alpha asserts only: chrome
//! tiles have no radius or glow.

use cooked_model::histogram_clock::MinuteHistory;
use cooked_model::tide::{
    FixedStepper, LiquidBody, LiquidField, LiquidShade, Motion, TideHistogram, TideIsland,
    TideScale, Water,
};

fn history(values: Vec<f64>) -> MinuteHistory {
    MinuteHistory {
        values,
        top: 64000.0,
        threshold: 40000.0,
    }
}

fn max_abs_diff(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f64::max)
}

#[test]
fn stepper_runs_fixed_steps_whatever_the_frame_rate() {
    let mut stepper = FixedStepper::default();
    let mut steps = 0i64;
    stepper.advance(0.0, |_| steps += 1);
    for frame in 1..=60 {
        stepper.advance(frame as f64 / 60.0, |_| steps += 1);
    }
    assert!((steps - 240).abs() <= 1);
    let mut slow = FixedStepper::default();
    let mut slow_steps = 0i64;
    slow.advance(0.0, |_| {});
    for frame in 1..=24 {
        slow.advance(frame as f64 / 24.0, |_| slow_steps += 1);
    }
    assert!((slow_steps - 240).abs() <= 1);
}

#[test]
fn stepper_caps_a_stall() {
    let mut stepper = FixedStepper::default();
    let mut steps = 0;
    stepper.advance(0.0, |_| {});
    stepper.advance(5.0, |_| steps += 1);
    assert_eq!(steps, 16); // 1/15 s at 240 Hz
}

#[test]
fn spring_settles_on_its_target_without_blowing_up() {
    let mut water = Water::new(84, 120.0, 110.0, 9.0, 0.0);
    let target: Vec<f64> = (0..84).map(|i| (i % 7) as f64 * 3.0).collect();
    let mut peak = 0.0f64;
    FixedStepper::warm(4.0, |dt| {
        water.step(dt, &target);
        peak = peak.max(water.height.iter().map(|h| h.abs()).fold(0.0, f64::max));
    });
    assert!(peak < 30.0);
    assert!(water.velocity.iter().all(|v| v.abs() < 0.05));
    // neighbours pull each column a little off its target, never far
    assert!(
        water
            .height
            .iter()
            .zip(&target)
            .all(|(h, t)| (h - t).abs() < 6.0)
    );
}

#[test]
fn one_step_follows_the_spring_law() {
    let mut water = Water::new(3, 120.0, 110.0, 9.0, 0.0);
    water.height = vec![0.0, 1.0, 0.0];
    water.step(0.01, &[0.0, 0.0, 0.0]);
    // v = (k·(0 - 1) + c·(0 + 0 - 2) - d·0)·dt, then h += v·dt
    let v = (-120.0 - 220.0) * 0.01;
    assert!((water.velocity[1] - v).abs() < 1e-12);
    assert!((water.height[1] - (1.0 + v * 0.01)).abs() < 1e-12);
    assert!((water.velocity[0] - 110.0 * 0.01).abs() < 1e-12);
}

#[test]
fn square_root_scale() {
    assert_eq!(TideScale::y(16000.0, 64000.0), 0.5);
    assert_eq!(TideScale::y(64000.0, 64000.0), 1.0);
    assert_eq!(TideScale::y(200_000.0, 64000.0), 1.0);
    assert_eq!(TideScale::y(-5.0, 64000.0), 0.0);
    // the threshold at 1/1.6 of the top sits on sqrt(0.625) of 24 rows
    assert_eq!(TideScale::threshold_row(24, 40000.0, 64000.0), 24 - 19);
}

#[test]
fn targets_keep_a_minute_as_wide_as_it_is() {
    let mut minutes = vec![0.0; 60];
    minutes[30] = 64000.0;
    let t = TideScale::targets(&minutes, 84, 24, 64000.0);
    assert_eq!(t.len(), 84);
    assert_eq!(t.iter().filter(|v| **v > 0.4).count(), 2); // minute 30 is columns 42 and 43 (0.714 min each)
    // back to tokens: the two columns hold the whole minute
    let tokens: f64 = t
        .iter()
        .filter(|v| **v > 0.4)
        .map(|v| (v / 24.0).powi(2) * 64000.0 * 60.0 / 84.0)
        .sum();
    assert!((tokens - 64000.0).abs() < 1.0);
    assert!(t.iter().all(|v| *v >= 0.4 && *v <= 24.0));
}

/// The polish criterion: a single spike, once settled, is no more than 1 column wider than its real value.
#[test]
fn settled_spike_stays_narrow() {
    let mut minutes = vec![2000.0; 60];
    minutes[30] = 64000.0;
    let spiked = history(minutes.clone());
    let mut flat = TideHistogram::new();
    let mut spike = TideHistogram::new();
    flat.frame(&history(vec![2000.0; 60]), 0.0, false, Motion::CALM);
    spike.frame(&history(vec![2000.0; 60]), 0.0, false, Motion::CALM);
    for frame in 1..=240 {
        spike.frame(&spiked, frame as f64 / 60.0, false, Motion::CALM);
    }
    let target = TideScale::targets(&minutes, 84, 24, 64000.0);
    let base = flat.heights()[0];
    let wide = |h: &[f64]| {
        let top = h.iter().cloned().fold(f64::MIN, f64::max) - base;
        h.iter().filter(|v| **v - base >= top / 2.0).count()
    };
    assert!(wide(spike.heights()) <= wide(&target) + 1);
}

/// Task 4: the open chart keeps moving on a flat history, by less than 0.6 rows, and never into the threshold row.
#[test]
fn idle_ripples_stay_small() {
    let flat = history(vec![2000.0; 60]);
    let mut tide = TideHistogram::new();
    let row = TideScale::threshold_row(24, 40000.0, 64000.0);
    let (mut biggest, mut moved) = (0.0f64, 0);
    for frame in 0..=(60 * 30) {
        let f = tide
            .frame(&flat, frame as f64 / 60.0, false, Motion::CALM)
            .clone();
        let offset = max_abs_diff(tide.shown_heights(), tide.heights());
        biggest = biggest.max(offset);
        if offset > 0.05 {
            moved += 1;
        }
        for c in 0..84 {
            for r in 0..=row {
                assert!(f.level[r * 84 + c] < 0.05);
            }
        }
    }
    println!("idle ripple: biggest {biggest} rows, moving in {moved} of 1801 frames");
    assert!(biggest < 0.6);
    assert!(moved > 1200); // alive, not a drop now and then (slow drops: about 3 frames in 4)
}

/// A column just under the threshold row is never lifted into it by a ripple.
#[test]
fn ripples_never_lift_into_the_threshold_row() {
    let row = TideScale::threshold_row(24, 40000.0, 64000.0);
    let line = (24 - row) as f64;
    let h = history(vec![37000.0; 60]);
    let mut tide = TideHistogram::new();
    for frame in 0..=(60 * 20) {
        tide.frame(&h, frame as f64 / 60.0, false, Motion::CALM);
        for (shown, h) in tide.shown_heights().iter().zip(tide.heights()) {
            assert!(*shown <= h.max(line - 1.0) + 1e-9);
        }
    }
}

/// Dot motion task 1: the chart's body is brighter (34–70%), the island's tank keeps 20–52%.
#[test]
fn chart_body_is_brighter_than_the_tank() {
    let h = history(vec![16000.0; 60]);
    let mut tide = TideHistogram::new();
    let f = tide.frame(&h, 0.0, true, Motion::CALM);
    // 12 rows of liquid in column 40: the dot 3 rows under the meniscus (row 12)
    assert!(f.level[15 * 84 + 40] >= 0.5);
    assert!((LiquidBody::TANK.light(3.5) - (0.2 + 0.32 * (-3.5f64 / 4.0).exp())).abs() < 1e-12);
    assert_eq!(LiquidBody::CHART.light(0.0), 0.0);
    assert!(LiquidBody::CHART.light(100.0) > 0.34);
}

/// Dot motion task 2: body dots rise and sink with the ripple above them, at most 0.5 pt, less with depth.
#[test]
fn body_dots_follow_the_ripple_by_at_most_half_a_point() {
    let h = history(vec![16000.0; 60]);
    let mut tide = TideHistogram::new();
    let (mut biggest, mut deep_moved) = (0.0f32, 0.0f32);
    for frame in 0..=(60 * 20) {
        let f = tide.frame(&h, frame as f64 / 60.0, false, Motion::CALM);
        biggest = biggest.max(f.lift.iter().map(|l| l.abs()).fold(0.0, f32::max));
        for c in 0..84 {
            let shallow = f.lift[13 * 84 + c].abs();
            let deep = f.lift[23 * 84 + c].abs();
            assert!(deep <= shallow + 1e-6);
            deep_moved = deep_moved.max(deep);
            assert_eq!(f.lift[c], 0.0); // dry dots stay put
        }
    }
    println!("body lift: biggest {biggest} pt, deepest row {deep_moved} pt");
    assert!(biggest <= 0.5 + 1e-6);
    assert!(biggest > 0.15);
    assert!(deep_moved > 0.0); // the ripple reaches the bottom row too
    let still = tide.frame(&h, 20.1, true, Motion::CALM);
    assert!(still.lift.iter().all(|l| *l == 0.0));
}

/// 10m moves like the island: more ripple than the long ranges, more with the rate, still bounded and never into
/// the threshold row.
#[test]
fn ten_minutes_ripples_like_the_island() {
    let row = TideScale::threshold_row(24, 40000.0, 64000.0);
    let line = (24 - row) as f64;
    let run = |rate: f64, motion: Motion| -> f64 {
        let h = history(vec![rate; 10]);
        let mut tide = TideHistogram::new();
        let mut total = 0.0;
        for frame in 0..=(60 * 20) {
            tide.frame(&h, frame as f64 / 60.0, false, motion);
            let moved: Vec<f64> = tide
                .shown_heights()
                .iter()
                .zip(tide.heights())
                .map(|(s, h)| (s - h).abs())
                .collect();
            assert!(moved.iter().cloned().fold(0.0, f64::max) <= 1.2 + 1e-9);
            for (shown, h) in tide.shown_heights().iter().zip(tide.heights()) {
                assert!(*shown <= h.max(line - 1.0) + 1e-9);
            }
            total += moved.iter().sum::<f64>();
        }
        total
    };
    let calm = run(2000.0, Motion::CALM);
    let quiet = run(2000.0, Motion::LIVELY);
    let busy = run(30000.0, Motion::LIVELY);
    println!("ripple totals: calm {calm}, lively quiet {quiet}, lively busy {busy}");
    assert!(quiet > calm * 2.0);
    assert!(busy > quiet);
}

#[test]
fn still_histogram_has_no_ripples() {
    let flat = history(vec![2000.0; 60]);
    let mut tide = TideHistogram::new();
    for frame in 0..=600 {
        tide.frame(&flat, frame as f64 / 60.0, false, Motion::CALM);
    }
    tide.frame(&flat, 10.1, true, Motion::CALM);
    assert_eq!(tide.shown_heights(), tide.heights());
    assert_eq!(
        tide.heights(),
        TideScale::targets(&flat.values, 84, 24, 64000.0).as_slice()
    );
}

#[test]
fn histogram_is_red_only_above_the_threshold() {
    let h = history(vec![64000.0; 60]);
    let mut tide = TideHistogram::new();
    let f = tide.frame(&h, 0.0, true, Motion::CALM);
    let row = TideScale::threshold_row(24, 40000.0, 64000.0);
    assert!(f.hot[0]);
    assert!(!f.hot[row * 84]);
}

/// A past minute over the threshold is coral even when the rate now is calm; the calm minutes stay pearl.
#[test]
fn past_minutes_over_the_threshold_are_coral() {
    let mut minutes = vec![2000.0; 60];
    for m in minutes.iter_mut().take(30).skip(20) {
        *m = 60000.0;
    }
    let mut tide = TideHistogram::new();
    let f = tide.frame(&history(minutes), 0.0, true, Motion::CALM);
    let row = TideScale::threshold_row(24, 40000.0, 64000.0);
    let (spike_col, calm_col) = (25 * 84 / 60, 70);
    assert!((0..row).any(|r| f.hot[r * 84 + spike_col]));
    assert!(!(0..24).any(|r| f.hot[r * 84 + calm_col]));
    assert!(!(row..24).any(|r| f.hot[r * 84 + spike_col]));
}

/// A spring overshooting the threshold row is not a real value: it stays pearl.
#[test]
fn overshoot_never_turns_coral() {
    let mut minutes = vec![2000.0; 60];
    for m in minutes.iter_mut().take(35).skip(26) {
        *m = 37000.0; // just under the 40000 threshold
    }
    let mut tide = TideHistogram::new();
    tide.frame(&history(vec![2000.0; 60]), 0.0, false, Motion::CALM);
    let mut saw_overshoot = false;
    let row = TideScale::threshold_row(24, 40000.0, 64000.0);
    for frame in 1..=120 {
        // every few frames the spike moves one minute left, like minutes closing quickly
        let k = frame / 20;
        let mut shifted: Vec<f64> = minutes[k..].to_vec();
        shifted.extend(std::iter::repeat_n(2000.0, k));
        let f = tide
            .frame(&history(shifted), frame as f64 / 60.0, false, Motion::CALM)
            .clone();
        if tide.heights().iter().any(|h| *h > (24 - row) as f64) {
            saw_overshoot = true;
        }
        assert!(!f.hot.contains(&true));
    }
    assert!(saw_overshoot); // the test means something only if a spring did overshoot
}

#[test]
fn meniscus_is_the_brightest_row() {
    let h = history(vec![16000.0; 60]);
    let mut tide = TideHistogram::new();
    let f = tide.frame(&h, 0.0, true, Motion::CALM);
    let column: Vec<f32> = (0..24).map(|r| f.level[r * 84 + 40]).collect(); // 12 rows of liquid
    let brightest = (0..24)
        .max_by(|a, b| column[*a].total_cmp(&column[*b]))
        .unwrap_or(0);
    assert_eq!(brightest, 24 - 12);
    assert_eq!(column[0], 0.0); // dry above
    assert!(column[23] > 0.2 && column[23] < column[brightest]); // the body below is dimmer
}

#[test]
fn island_level_puts_the_threshold_at_three_quarters() {
    assert_eq!(TideIsland::level(40000.0, 40000.0), 0.75);
    assert_eq!(TideIsland::level(400_000.0, 40000.0), 1.0);
    assert_eq!(TideIsland::level(5.0, 0.0), 0.0);
    assert_eq!(TideIsland::threshold_row(), 3);
}

/// The polish criterion: the drops never lift a calm surface over the threshold row.
#[test]
fn calm_drops_stay_below_the_threshold() {
    let mut tank = TideIsland::new();
    let level = TideIsland::level(11000.0, 40000.0);
    let row = TideIsland::threshold_row();
    for frame in 0..=(60 * 30) {
        let f = tank.frame(frame as f64 / 60.0, level, false, false);
        for c in 0..TideIsland::COLS {
            for r in 0..row {
                assert!(f.level[r * TideIsland::COLS + c] < 0.05);
            }
        }
    }
}

#[test]
fn still_island_is_flat() {
    let mut tank = TideIsland::new();
    let f = tank.frame(3.0, 0.5, false, true);
    for r in 0..TideIsland::ROWS {
        let row = &f.level[r * TideIsland::COLS..(r + 1) * TideIsland::COLS];
        assert!(row.iter().all(|v| *v == row[0]));
    }
}

#[test]
fn shade_fades_the_rim_and_caps_the_glow() {
    let mut f = LiquidField::new(36, 1);
    f.level = vec![1.0; 36];
    f.fresh = vec![1.0; 36];
    let edge = LiquidShade::shade(&f, 0, 0.06, 3);
    let middle = LiquidShade::shade(&f, 18, 0.06, 3);
    assert!(edge.alpha < middle.alpha);
    assert_eq!(middle.alpha, 1.0);
    f.level[18] = 0.0;
    let off = LiquidShade::shade(&f, 18, 0.06, 3);
    assert_eq!(off.alpha, 0.06);
}
