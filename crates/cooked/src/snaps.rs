//! Scenes for Rune's headless tooling (`--snap`, `--tree`; feature `tooling`): fixed data at a fixed time, so a
//! snapshot is the same on every run.
//!
//! `cooked --snap out.png --scene tiles-demo-peak`: the demo peak's histogram (72 x 26, last hour), the urgent island
//! wave (16 x 5) and the active account's bar (44 x 2), metal stopped at `SNAP_T`.

use cooked_model::demo_data;
use cooked_model::histogram_clock::ChartRange;
use cooked_model::tiles as grids;
use rune::{App, AppSpec, Color, Cx, El, col, el, row};

use crate::tiles::{ChromeTiles, TileGrid, TileGrids, tile_slot};

/// The metal's clock in every scene, seconds (step-0 1f used the same).
pub const SNAP_T: f64 = 3.0;
pub const SCENES: &[&str] = &["tiles-demo-peak"];
/// Window size, points.
pub const SIZE: (f32, f32) = (380.0, 190.0);

const HISTOGRAM: u32 = 1;
const WAVE: u32 = 2;
const BAR: u32 = 3;

pub struct Snap {
    grids: Vec<(u32, TileGrid)>,
}

impl TileGrids for Snap {
    fn tile_grid(&self, mark: u32) -> Option<&TileGrid> {
        self.grids.iter().find(|(m, _)| *m == mark).map(|(_, g)| g)
    }
}

/// The demo peak, as the popover and the island show it.
fn demo_peak() -> Snap {
    let snapshot = demo_data::peak();
    let histogram = grids::histogram_grid(&snapshot, ChartRange::Hour, 72, 26);
    let level = grids::wave_level(snapshot.rate_now as f64, snapshot.threshold as f64);
    let wave = grids::wave_grid(SNAP_T as f32, level, snapshot.is_urgent(), 16, 5);
    let left = snapshot
        .active()
        .map_or(0.0, |a| a.five_hour_left() as f32 / 100.0);
    let bar = grids::bar_grid(left, Some(0.4), true, 44, 2);
    Snap {
        grids: vec![
            (HISTOGRAM, TileGrid::new(72, 26, histogram)),
            (WAVE, TileGrid::new(16, 5, wave)),
            (BAR, TileGrid::new(44, 2, bar).spacing(1.5, 0.8)),
        ],
    }
}

fn init() -> Snap {
    Snap { grids: Vec::new() }
}

/// A recessed well (DESIGN.md) holding one grid.
fn well(key: &'static str, mark: u32, w: f32, h: f32) -> El {
    el().key(key)
        .size(w + 12.0, h + 12.0)
        .radius(8.0)
        .fill(hex("#080808"))
        .padding(6.0)
        .child(tile_slot(mark).key("tiles").w_full().h_full())
}

fn hex(s: &str) -> Color {
    rune::color::hex(s).unwrap_or(Color::BLACK)
}

fn view(_: &Snap, _: &mut Cx<Snap>) -> El {
    el().key("page")
        .w_full()
        .h_full()
        .fill(hex("#161616"))
        .padding(16.0)
        .child(
            col()
                .gap(12.0)
                .child(well("histogram", HISTOGRAM, 336.0, 104.0))
                .child(
                    row()
                        .gap(12.0)
                        .center_y()
                        .child(well("wave", WAVE, 56.0, 22.0))
                        .child(well("bar", BAR, 176.0, 7.0)),
                ),
        )
}

/// The scenes' app: one window, the tile compositor with the metal stopped.
pub fn app() -> App<Snap> {
    App::new(AppSpec {
        scenes: &[("tiles-demo-peak", |s| *s = demo_peak())],
        ..AppSpec::new("Cooked scenes", SIZE, init, view)
    })
    .srgb_blend(true)
    .compositor(ChromeTiles::still_at(SNAP_T))
}

/// Whether `args` ask for one of `scenes` (`island::scenes`, `popover::scenes`).
pub fn wanted_scene(args: &[String], scenes: &[&str]) -> bool {
    args.windows(2)
        .any(|w| w[0] == "--scene" && scenes.contains(&w[1].as_str()))
}

/// Whether `args` ask for one of these scenes (or the list of them).
pub fn wanted(args: &[String]) -> bool {
    args.iter().any(|a| a == "--list-scenes")
        || args
            .windows(2)
            .any(|w| w[0] == "--scene" && SCENES.contains(&w[1].as_str()))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use cooked_model::tiles::TileLevel;
    use std::path::{Path, PathBuf};

    /// Mean absolute difference per channel, out of 255, the new render may have against the checked-in one. Step 0
    /// (1f) measured 0.28 between a headless snap and the same frame on screen; renders on one machine are equal.
    const MEAN_TOLERANCE: f64 = 1.0;

    pub(crate) fn decode(path: &Path) -> (u32, u32, Vec<u8>) {
        let file = std::fs::File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut reader = png::Decoder::new(std::io::BufReader::new(file))
            .read_info()
            .expect("png header");
        let mut buf = vec![0u8; reader.output_buffer_size().expect("png size")];
        let info = reader.next_frame(&mut buf).expect("png frame");
        assert_eq!(info.color_type, png::ColorType::Rgba, "{}", path.display());
        buf.truncate(info.buffer_size());
        (info.width, info.height, buf)
    }

    /// Renders `scene` of `app` headless and compares it with `tests/snaps/<scene>.png`: the mean absolute
    /// difference per channel stays within `MEAN_TOLERANCE`. `COOKED_UPDATE_SNAPS=1` writes the new render there
    /// instead.
    pub(crate) fn assert_matches_snap<S: 'static>(app: rune::App<S>, scene: &str) {
        assert_matches_snap_with(app, scene, &[]);
    }

    /// For a window that fits its content: a first run reads the content's height from `--tree`, and the snap is
    /// taken at that size (a headless snap keeps the window's first size).
    pub(crate) fn assert_matches_fitted_snap<S: 'static>(app: fn() -> rune::App<S>, scene: &str) {
        let dir = std::env::temp_dir().join(format!("cooked-fit-{}-{scene}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let tree = dir.join("tree.json");
        app()
            .args([
                "--tree".to_owned(),
                tree.to_string_lossy().into_owned(),
                "--scene".to_owned(),
                scene.to_owned(),
            ])
            .try_run()
            .expect("headless tree");
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&tree).expect("tree"))
                .expect("tree json");
        let _ = std::fs::remove_dir_all(&dir);
        let size: Vec<f64> = json["viewport"]
            .as_array()
            .expect("viewport")
            .iter()
            .filter_map(|v| v.as_f64())
            .collect();
        let size = format!("{}x{}", size[0], size[1]);
        assert_matches_snap_with(app(), scene, &["--size".to_owned(), size]);
    }

    fn assert_matches_snap_with<S: 'static>(app: rune::App<S>, scene: &str, extra: &[String]) {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let dir = std::env::temp_dir().join(format!("cooked-snap-{}-{scene}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let out = dir.join(format!("{scene}.png"));
        let mut args = vec![
            "--snap".to_owned(),
            out.to_string_lossy().into_owned(),
            "--scene".to_owned(),
            scene.to_owned(),
        ];
        args.extend_from_slice(extra);
        app.args(args).try_run().expect("headless render");
        let expected = manifest.join("tests/snaps").join(format!("{scene}.png"));
        if std::env::var_os("COOKED_UPDATE_SNAPS").is_some() {
            std::fs::copy(&out, &expected).expect("update snap");
        }
        let (w, h, new) = decode(&out);
        let (ew, eh, old) = decode(&expected);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!((w, h), (ew, eh), "{scene}: snap size");
        let total: u64 = new
            .iter()
            .zip(&old)
            .map(|(a, b)| a.abs_diff(*b) as u64)
            .sum();
        let mean = total as f64 / new.len() as f64;
        assert!(
            mean <= MEAN_TOLERANCE,
            "{scene}: mean abs diff {mean:.3}/255 > {MEAN_TOLERANCE}"
        );
    }

    #[test]
    fn the_peak_has_lit_red_and_off_tiles() {
        let snap = demo_peak();
        let histogram = snap.tile_grid(HISTOGRAM).expect("histogram");
        for level in [TileLevel::Off, TileLevel::Lit, TileLevel::Red] {
            assert!(histogram.cells.contains(&level), "{level:?}");
        }
        assert!(
            snap.tile_grid(WAVE)
                .expect("wave")
                .cells
                .contains(&TileLevel::Red)
        );
    }

    #[test]
    fn scene_flag_is_ours_only() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(wanted(&args(&[
            "cooked",
            "--snap",
            "x.png",
            "--scene",
            "tiles-demo-peak"
        ])));
        assert!(wanted(&args(&["cooked", "--list-scenes"])));
        assert!(!wanted(&args(&["cooked", "--scene", "other"])));
        assert!(!wanted(&args(&["cooked", "--feed", "demoPeak"])));
        let island = crate::island::scenes::SCENES;
        assert!(wanted_scene(
            &args(&["cooked", "--tree", "t.json", "--scene", "island-urgent"]),
            island
        ));
        assert!(!wanted_scene(
            &args(&["cooked", "--scene", "tiles-demo-peak"]),
            island
        ));
        assert!(wanted_scene(
            &args(&["cooked", "--snap", "p.png", "--scene", "popover-peak"]),
            crate::popover::scenes::SCENES
        ));
    }

    /// Renders `tiles-demo-peak` headless and compares it with the PNG in `tests/snaps/`.
    #[test]
    fn tiles_demo_peak_matches_the_checked_in_snap() {
        assert_matches_snap(app(), "tiles-demo-peak");
    }
}
