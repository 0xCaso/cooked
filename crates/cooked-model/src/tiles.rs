//! The chrome-tile grids the Rune UI draws: one value per tile, row-major from the top (index `r * cols + c`).
//! The histogram uses the Swift chart's bucketing and square-root scale (`TideScale`), the wave the island tank's
//! level (`TideIsland`), the bars the pills' pace strip (`AccountPills.swift`, `PaceStrip`).

use crate::histogram_clock::{ChartRange, MinuteHistory};
use crate::tide::{TideIsland, TideScale};
use crate::usage_snapshot::UsageSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TileLevel {
    #[default]
    Off,
    Lit,
    Red,
}

/// The popover histogram for `range` of the snapshot's minutes. Each column is one time bucket; its rows are lit
/// from the bottom in proportion to tokens/min (square-root scale, rounded to the nearest row). Lit tiles above the
/// p99 threshold row are red in the columns whose value is over the threshold.
pub fn histogram_grid(
    snapshot: &UsageSnapshot,
    range: ChartRange,
    cols: usize,
    rows: usize,
) -> Vec<TileLevel> {
    let all = MinuteHistory {
        values: snapshot.series.clone(),
        top: 0.0,
        threshold: snapshot.chart_threshold() as f64,
    };
    history_grid(&all.window(range.minutes()), cols, rows)
}

/// `histogram_grid` for a history already cut to its range (a `LiveHistogram` or demo clock frame), standing
/// still: each column at its target (`TideScale::fine_targets`).
pub fn history_grid(history: &MinuteHistory, cols: usize, rows: usize) -> Vec<TileLevel> {
    let heights = TideScale::fine_targets(&history.values, cols, rows, history.top);
    let threshold_row = TideScale::threshold_row(rows, history.threshold, history.top);
    let line = (rows - threshold_row) as f64;
    let reaches: Vec<bool> = heights.iter().map(|h| *h > line).collect();
    surface_grid(&heights, &reaches, threshold_row, cols, rows)
}

/// Column heights in rows (a `TideHistogram`'s surface, or targets) as tiles: each column lit from the bottom to
/// its height rounded to the nearest row; the lit tiles above `threshold_row` red in the columns that `reaches`.
pub fn surface_grid(
    heights: &[f64],
    reaches: &[bool],
    threshold_row: usize,
    cols: usize,
    rows: usize,
) -> Vec<TileLevel> {
    let mut grid = vec![TileLevel::Off; cols * rows];
    for (c, h) in heights.iter().take(cols).enumerate() {
        let lit = (h.round().max(0.0) as usize).min(rows);
        let over = reaches.get(c).copied().unwrap_or(false);
        for r in rows - lit..rows {
            grid[r * cols + c] = if over && r < threshold_row {
                TileLevel::Red
            } else {
                TileLevel::Lit
            };
        }
    }
    grid
}

/// The island wave at time `t` (seconds), mirrored around the centre row. `rate_level` is the tank's 0...1 level
/// (`TideIsland::level`): with no tokens a line on the centre row, wider as the rate grows. Red when `urgent`.
pub fn wave_grid(
    t: f32,
    rate_level: f32,
    urgent: bool,
    cols: usize,
    rows: usize,
) -> Vec<TileLevel> {
    let mid = rows.saturating_sub(1) as f32 / 2.0;
    // the tank's surface height for this level, on `rows` rows: 1.2 rows empty, 1.2 rows under the brim full
    let surface = 1.2 + rate_level.clamp(0.0, 1.0) * (rows as f32 - 2.4).max(0.0);
    let on = if urgent {
        TileLevel::Red
    } else {
        TileLevel::Lit
    };
    let mut grid = vec![TileLevel::Off; cols * rows];
    for c in 0..cols {
        // the mockup's slow sway (scene.js), on top of the level
        let amp = (surface - 1.0) / 2.0 + 0.6 * (c as f32 * 0.55 + t * 1.3).sin().abs();
        for r in 0..rows {
            if (r as f32 - mid).abs() <= amp {
                grid[r * cols + c] = on;
            }
        }
    }
    grid
}

/// The Rune island's wave at time `t` (seconds), as the approved mockup draws it
/// (`design/mockups/2026-11-24-rune-look/scene.js`, decision 2026-11-24-rune-island-width): mirrored around the
/// centre row, each column's half-height `1.2 + 1.1 |sin(0.55 c + 1.3 t)|` rows, so 3 to 5 of 5 rows lit and swaying.
/// Chrome when calm, red when `urgent`; the shape is the same in both.
pub fn island_wave_grid(t: f32, urgent: bool, cols: usize, rows: usize) -> Vec<TileLevel> {
    let mid = rows.saturating_sub(1) as f32 / 2.0;
    let on = if urgent {
        TileLevel::Red
    } else {
        TileLevel::Lit
    };
    let mut grid = vec![TileLevel::Off; cols * rows];
    for c in 0..cols {
        let amp = 1.2 + 1.1 * (c as f32 * 0.55 + t * 1.3).sin().abs();
        for r in 0..rows {
            if (r as f32 - mid).abs() <= amp {
                grid[r * cols + c] = on;
            }
        }
    }
    grid
}

/// An account bar: the left `fraction_left` lit, every row alike. `time_left` (0...1, the share of the window still
/// to go) puts the pace line in its column, which stays off. When the window runs out before its reset, the dark
/// tiles between the lit front and the line are the overspend, red: the Swift `PaceStrip` rule, which the mockup
/// (red over the last 45% of the lit part) only sketched.
pub fn bar_grid(
    fraction_left: f32,
    time_left: Option<f32>,
    runs_out_before_reset: bool,
    cols: usize,
    rows: usize,
) -> Vec<TileLevel> {
    let fill = fraction_left.clamp(0.0, 1.0) * cols as f32;
    let line = time_left.map(|t| {
        let col = (t.clamp(0.0, 1.0) * cols.saturating_sub(1) as f32).round() as usize;
        col.min(cols.saturating_sub(1))
    });
    let row: Vec<TileLevel> = (0..cols)
        .map(|c| {
            if Some(c) == line {
                return TileLevel::Off;
            }
            if (fill - c as f32).clamp(0.0, 1.0) >= 0.5 {
                TileLevel::Lit
            } else if runs_out_before_reset && line.is_some_and(|l| c < l) {
                TileLevel::Red
            } else {
                TileLevel::Off
            }
        })
        .collect();
    (0..rows).flat_map(|_| row.iter().copied()).collect()
}

/// The wave's level for a rate and the island's threshold, tokens per minute.
pub fn wave_level(rate: f64, threshold: f64) -> f32 {
    TideIsland::level(rate, threshold) as f32
}
