//! The chrome-tile grids the Rune UI draws, and the island with no log folders.
mod common;

use cooked_model::clock::FixedOffset;
use cooked_model::codex_log::CodexLog;
use cooked_model::defaults::MemoryDefaults;
use cooked_model::demo_data::Feed;
use cooked_model::histogram_clock::ChartRange;
use cooked_model::island_model::IslandState;
use cooked_model::notice::Notice;
use cooked_model::tide::TideScale;
use cooked_model::tiles::{TileLevel, bar_grid, histogram_grid, island_wave_grid, wave_grid};
use cooked_model::transcript_log::TranscriptLog;
use cooked_model::usage_snapshot::UsageSnapshot;

fn count(grid: &[TileLevel], level: TileLevel) -> usize {
    grid.iter().filter(|t| **t == level).count()
}

#[test]
fn grid_sizes() {
    let s = UsageSnapshot::empty("—");
    assert_eq!(histogram_grid(&s, ChartRange::Hour, 72, 26).len(), 72 * 26);
    assert_eq!(wave_grid(0.0, 0.5, false, 16, 5).len(), 16 * 5);
    assert_eq!(bar_grid(0.5, Some(0.5), false, 44, 2).len(), 44 * 2);
}

#[test]
fn empty_snapshot_is_all_off() {
    let s = UsageSnapshot::empty("—");
    for range in ChartRange::ALL {
        let g = histogram_grid(&s, range, 72, 26);
        assert_eq!(count(&g, TileLevel::Off), 72 * 26);
    }
}

#[test]
fn a_minute_over_the_threshold_is_red_only_above_the_threshold_row() {
    let mut s = UsageSnapshot::empty("—");
    s.total_threshold = Some(40000);
    s.series = vec![0.0; 60];
    s.series[30] = 64000.0;
    let (cols, rows) = (72, 26);
    let g = histogram_grid(&s, ChartRange::Hour, cols, rows);
    // top: 1.6 × the threshold = 64000, so the minute fills its columns
    let row = TideScale::threshold_row(rows, 40000.0, 64000.0);
    assert!(count(&g, TileLevel::Red) > 0);
    for r in 0..rows {
        for c in 0..cols {
            let t = g[r * cols + c];
            if t == TileLevel::Red {
                assert!(r < row, "red at row {r}, threshold row {row}");
            }
            if r >= row && c == 36 {
                assert_eq!(t, TileLevel::Lit, "row {r} col {c}");
            }
        }
    }
    // 0.83 minutes per column: minute 30 fills column 36 and a sixth of 37; the calm columns stay dark
    assert_eq!(g[c_at(0, 36, cols)], TileLevel::Red);
    assert_eq!(g[c_at(rows - 1, 10, cols)], TileLevel::Off);
    assert_eq!(g[c_at(0, 37, cols)], TileLevel::Off);
    assert_eq!(g[c_at(rows - 1, 37, cols)], TileLevel::Lit);
}

fn c_at(r: usize, c: usize, cols: usize) -> usize {
    r * cols + c
}

#[test]
fn a_minute_under_the_threshold_is_never_red() {
    let mut s = UsageSnapshot::empty("—");
    s.total_threshold = Some(40000);
    s.series = vec![37000.0; 60];
    let g = histogram_grid(&s, ChartRange::Hour, 72, 26);
    assert_eq!(count(&g, TileLevel::Red), 0);
    assert!(count(&g, TileLevel::Lit) > 0);
}

#[test]
fn rows_lit_from_the_bottom_rounded_to_the_nearest_row() {
    let mut s = UsageSnapshot::empty("—");
    s.total_threshold = Some(40000);
    s.series = vec![16000.0; 60]; // sqrt(16000 / 64000) = half of 26 rows
    let g = histogram_grid(&s, ChartRange::Hour, 72, 26);
    for c in 0..72 {
        let lit: Vec<usize> = (0..26)
            .filter(|r| g[r * 72 + c] != TileLevel::Off)
            .collect();
        assert_eq!(lit, (13..26).collect::<Vec<_>>());
    }
}

/// The share left and the pace line, on whole tiles: team on 30/10 at 13:08, 26% left
/// and 27% of the window to go on 15 columns. The fill is 3.9 columns (the front tile is lit from half a tile) and
/// the line, an off tile, is at column 4.
#[test]
fn bar_lights_the_share_left_and_puts_the_line_at_time_left() {
    let row = |g: Vec<TileLevel>| g[..15].to_vec();
    let lit = |c: usize| {
        if c < 4 {
            TileLevel::Lit
        } else {
            TileLevel::Off
        }
    };
    let g = bar_grid(0.26, Some(0.27), false, 15, 2);
    assert_eq!(&g[..15], &g[15..], "every row alike");
    assert_eq!(row(g), (0..15).map(lit).collect::<Vec<_>>());
    // the line is off even inside the lit part: at column 4 of a strip lit to 7.5
    let g = row(bar_grid(0.5, Some(0.27), false, 15, 1));
    assert_eq!(g[4], TileLevel::Off);
    assert_eq!(count(&g, TileLevel::Lit), 7);
    // over 100% left and no time left: all lit, no line
    assert_eq!(
        count(&bar_grid(1.4, None, false, 15, 1), TileLevel::Lit),
        15
    );
}

#[test]
fn bar_runs_out_before_reset_turns_the_overspend_red() {
    let g = bar_grid(0.18, Some(0.5), true, 44, 2);
    assert!(count(&g, TileLevel::Red) > 0);
    assert_eq!(count(&g, TileLevel::Lit), 2 * 8); // 0.18 × 44 = 7.9 columns
    // red only between the lit front and the time-left line, the same in both rows
    for r in 0..2 {
        for c in 0..44 {
            let t = g[r * 44 + c];
            assert_eq!(t == TileLevel::Red, (8..22).contains(&c), "row {r} col {c}");
        }
    }
    let calm = bar_grid(0.18, Some(0.5), false, 44, 2);
    assert_eq!(count(&calm, TileLevel::Red), 0);
    assert_eq!(count(&calm, TileLevel::Lit), 2 * 8);
    // no line, no overspend
    assert_eq!(count(&bar_grid(0.18, None, true, 44, 2), TileLevel::Red), 0);
    assert_eq!(
        count(&bar_grid(0.0, Some(0.5), false, 44, 2), TileLevel::Lit),
        0
    );
    assert_eq!(
        count(&bar_grid(1.4, None, false, 44, 2), TileLevel::Lit),
        88
    );
}

#[test]
fn wave_is_red_only_when_urgent() {
    for frame in 0..120 {
        let t = frame as f32 / 60.0;
        let calm = wave_grid(t, 0.8, false, 16, 5);
        assert_eq!(count(&calm, TileLevel::Red), 0);
        assert!(count(&calm, TileLevel::Lit) > 0);
        let urgent = wave_grid(t, 1.0, true, 16, 5);
        assert!(count(&urgent, TileLevel::Red) > 0);
        assert_eq!(count(&urgent, TileLevel::Lit), 0);
    }
}

#[test]
fn wave_is_mirrored_around_the_centre_row_and_widens_with_the_rate() {
    let quiet = wave_grid(0.7, 0.0, false, 16, 5);
    // no tokens: a line on the centre row
    for r in 0..5 {
        for c in 0..16 {
            assert_eq!(quiet[r * 16 + c] == TileLevel::Lit, r == 2);
        }
    }
    let busy = wave_grid(0.7, 1.0, false, 16, 5);
    for c in 0..16 {
        for (a, b) in [(0, 4), (1, 3)] {
            assert_eq!(busy[a * 16 + c], busy[b * 16 + c]);
        }
    }
    assert!(count(&busy, TileLevel::Lit) > count(&quiet, TileLevel::Lit) + 16);
}

/// No `~/.claude*/projects`, no Codex: the island shows "—" and a notice, no crash.
#[test]
fn no_log_roots_gives_dash_and_notice() {
    let dir = common::temp_dir("no-roots");
    let now = common::now();
    // no account connected at all
    let mut source = common::source(dir.clone());
    source.accounts = Vec::new();
    let reading = source.read(now, &FixedOffset(0));
    assert_eq!(reading.notices, vec![Notice::NoAccounts]);
    assert!(reading.snapshot.active().is_none());
    assert_eq!(reading.snapshot.rate_now, 0);

    // accounts connected, but their folders are gone
    let mut source = common::source(dir.clone());
    for a in &mut source.accounts {
        a.path = dir
            .join("missing")
            .join(&a.id)
            .to_string_lossy()
            .into_owned();
    }
    source.transcripts = Some(TranscriptLog::with_accounts(
        &source.accounts,
        Some(source.ledger.clone()),
    ));
    source.codex = CodexLog::with_accounts(&source.accounts, Some(source.ledger.clone()));
    let reading = source.read(now, &FixedOffset(0));
    assert!(
        reading
            .snapshot
            .active()
            .and_then(|a| a.five_hour_left_if_known())
            .is_none()
    );
    assert!(
        reading
            .notices
            .iter()
            .any(|n| matches!(n, Notice::NoPercent { .. }))
    );

    let mut island = IslandState::new(
        UsageSnapshot::empty("—"),
        Some(Feed::Live),
        MemoryDefaults::default(),
        now,
    );
    island.apply(&reading);
    assert_eq!(island.five_hour_label(), "—");
    assert!(island.notice().is_some());
    assert_eq!(
        count(
            &histogram_grid(&island.snapshot, ChartRange::Hour, 72, 26),
            TileLevel::Red
        ),
        0
    );
}

/// The mockup's island wave: every column lights 3 or 5 of 5 rows, mirrored, and it sways with `t`.
#[test]
fn island_wave_sways_three_to_five_rows_like_the_mockup() {
    for t in [0.0, 0.4, 1.7, 3.0] {
        let grid = island_wave_grid(t, false, 16, 5);
        assert_eq!(grid.len(), 80);
        for c in 0..16 {
            let lit: Vec<bool> = (0..5).map(|r| grid[r * 16 + c] == TileLevel::Lit).collect();
            let n = lit.iter().filter(|&&l| l).count();
            assert!(n == 3 || n == 5, "column {c} at {t}: {n} rows");
            assert_eq!(
                lit,
                lit.iter().rev().copied().collect::<Vec<_>>(),
                "mirrored"
            );
        }
        assert!(!grid.contains(&TileLevel::Red));
    }
    // column 0 at t = 0: half-height 1.2, three rows; and the columns differ
    let calm = island_wave_grid(0.0, false, 16, 5);
    let lit =
        |g: &[TileLevel], c: usize| (0..5).filter(|r| g[r * 16 + c] != TileLevel::Off).count();
    assert_eq!(lit(&calm, 0), 3);
    assert!((0..16).any(|c| lit(&calm, c) == 5));
    assert_ne!(calm, island_wave_grid(0.8, false, 16, 5));
    let urgent = island_wave_grid(0.0, true, 16, 5);
    assert!(urgent.iter().all(|&l| l != TileLevel::Lit));
    assert_eq!(
        urgent
            .iter()
            .map(|&l| l == TileLevel::Off)
            .collect::<Vec<_>>(),
        calm.iter()
            .map(|&l| l == TileLevel::Off)
            .collect::<Vec<_>>(),
        "same shape calm and urgent"
    );
}
