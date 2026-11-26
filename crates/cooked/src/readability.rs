//! How well the lit soft dots read on their well. Each scene is rendered headless at `snaps::SNAP_T`; for every dot lit
//! past `LIT`, the mean luminance of its centre (2 x 2 device px) is set against the well's fill with WCAG's
//! `contrast_ratio`. The tests print the minimum per colour and hold it to the floors below, set on the chrome-tile
//! look (its mins less ~0.05) and kept for the soft dots, which clear them.
//!
//! It lives in the binary's tests, not `tests/`: the crate has no library for an integration test to reach the
//! scenes and grids through.

use std::path::PathBuf;

use cooked_model::contrast::{Rgb, contrast_ratio, relative_luminance};
use cooked_model::demo_data::Feed;
use cooked_model::tiles::TileLevel;

use crate::dots::Layout;
use crate::island;
use crate::popover;
use crate::snaps::SNAP_T;
use crate::snaps::tests::decode;
use crate::tiles::{self, TileGrid, TileGrids};

/// Floors, WCAG ratio: white dots, red dots in the popover and on the island.
const CHROME_FLOOR: f64 = 1.7;
const RED_POPOVER_FLOOR: f64 = 2.4;
const RED_ISLAND_FLOOR: f64 = 3.0;
/// A dot counts as lit from this light: the chart's surface and the braid's head, which carry the value; the dimmer
/// body and tail below it are dim by design.
const LIT: f32 = 0.7;

/// A scene rendered headless: its pixels (RGBA, device pixels), its tree, and the scale between them.
struct Render {
    width: usize,
    pixels: Vec<u8>,
    tree: serde_json::Value,
}

fn render<S: 'static>(app: impl Fn() -> rune::App<S>, scene: &str) -> Render {
    let dir = std::env::temp_dir().join(format!("cooked-read-{}-{scene}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let (png, tree) = (dir.join("snap.png"), dir.join("tree.json"));
    for (flag, out) in [("--snap", &png), ("--tree", &tree)] {
        app()
            .args([
                flag.to_owned(),
                out.to_string_lossy().into_owned(),
                "--scene".to_owned(),
                scene.to_owned(),
            ])
            .try_run()
            .expect("headless render");
    }
    if let Some(keep) = std::env::var_os("COOKED_READABILITY_OUT") {
        let keep = PathBuf::from(keep);
        std::fs::create_dir_all(&keep).expect("output dir");
        std::fs::copy(&png, keep.join(format!("{scene}.png"))).expect("keep snap");
    }
    let (width, _, pixels) = decode(&png);
    let tree = serde_json::from_str(&std::fs::read_to_string(&tree).expect("tree")).expect("json");
    let _ = std::fs::remove_dir_all(&dir);
    Render {
        width: width as usize,
        pixels,
        tree,
    }
}

/// The rect (points) of the element at `path`, and its parent's fill (the well).
fn slot(tree: &serde_json::Value, path: &str) -> ([f64; 4], Rgb) {
    let elements = tree["elements"].as_array().expect("elements");
    let find = |p: &str| {
        elements
            .iter()
            .find(|e| e["path"] == p)
            .unwrap_or_else(|| panic!("no element {p}"))
    };
    let rect = find(path)["rect"]
        .as_array()
        .expect("rect")
        .iter()
        .map(|v| v.as_f64().expect("number"))
        .collect::<Vec<_>>();
    let parent = &path[..path.rfind('/').expect("parent")];
    let fill = find(parent)["fill"].as_str().expect("fill");
    let hex = u32::from_str_radix(&fill[1..7], 16).expect("hex fill");
    ([rect[0], rect[1], rect[2], rect[3]], Rgb::hex(hex))
}

/// One lit dot's centre: its contrast on the well and its mean colour (0...255).
#[derive(Debug, Clone, Copy)]
struct Face {
    level: TileLevel,
    ratio: f64,
    rgb: [f64; 3],
}

fn srgb(v: f64) -> f64 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// Every dot of `grid` lit past `LIT`, standing still at `SNAP_T` in `rect` (points), measured on the render against
/// `well`.
fn faces(r: &Render, grid: &TileGrid, rect: [f64; 4], well: Rgb) -> Vec<Face> {
    let scale = r.tree["scale"].as_f64().expect("scale");
    let field = tiles::still_field(grid, SNAP_T as f32).expect("a dot grid");
    let [x, y, w, h] = rect;
    let (cols, rows) = (field.cols as f64, field.rows as f64);
    let (step, first) = match field.layout {
        Layout::Fill => ([w / cols, h / rows], [w / cols / 2.0, h / rows / 2.0]),
        Layout::Pitch(p) => {
            let p = p as f64;
            (
                [p, p],
                [(w - (cols - 1.0) * p) / 2.0, (h - (rows - 1.0) * p) / 2.0],
            )
        }
    };
    let mut out = Vec::new();
    for (i, dot) in field.dots.iter().enumerate() {
        if dot.power < LIT {
            continue;
        }
        let (c, row) = ((i % field.cols) as f64, (i / field.cols) as f64);
        let cx = ((x + first[0] + c * step[0]) * scale).round() as usize;
        let cy = ((y + first[1] + row * step[1]) * scale).round() as usize;
        let (mut lum, mut sum, mut n) = (0.0, [0.0; 3], 0.0);
        for py in cy - 1..=cy {
            for px in cx - 1..=cx {
                let k = (py * r.width + px) * 4;
                let p = &r.pixels[k..k + 3];
                lum += relative_luminance(Rgb::new(
                    p[0] as f64 / 255.0,
                    p[1] as f64 / 255.0,
                    p[2] as f64 / 255.0,
                ));
                for j in 0..3 {
                    sum[j] += p[j] as f64;
                }
                n += 1.0;
            }
        }
        let grey = srgb(lum / n);
        out.push(Face {
            level: if dot.red {
                TileLevel::Red
            } else {
                TileLevel::Lit
            },
            ratio: contrast_ratio(Rgb::new(grey, grey, grey), well),
            rgb: sum.map(|s| s / n),
        });
    }
    out
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(f64::NAN)
}

/// Prints the minimum contrast of `faces` per colour and overall; returns (chrome, red), infinite when none.
fn report(scene: &str, faces: &[Face]) -> (f64, f64) {
    let mut mins = [f64::INFINITY; 2];
    for (k, level) in [TileLevel::Lit, TileLevel::Red].into_iter().enumerate() {
        let of: Vec<&Face> = faces.iter().filter(|f| f.level == level).collect();
        if of.is_empty() {
            continue;
        }
        mins[k] = of.iter().map(|f| f.ratio).fold(f64::INFINITY, f64::min);
        let med = |i: usize| median(of.iter().map(|f| f.rgb[i]).collect());
        println!(
            "readability {scene} {}: dots {} min {:.2}:1 median {:.2}:1 median rgb ({:.0}, {:.0}, {:.0})",
            if level == TileLevel::Red {
                "red"
            } else {
                "chrome"
            },
            of.len(),
            mins[k],
            median(of.iter().map(|f| f.ratio).collect()),
            med(0),
            med(1),
            med(2),
        );
    }
    println!("readability {scene} min {:.2}:1", mins[0].min(mins[1]));
    (mins[0], mins[1])
}

#[test]
fn popover_peak_dots_read_on_their_well() {
    let r = render(popover::scenes::app, "popover-peak");
    let (rect, well) = slot(&r.tree, "root/popover/tint/veil/body/tpm/histogram/tiles");
    let p = popover::scenes::popover(Feed::DemoPeak);
    let grid = p.tile_grid(popover::HISTOGRAM_MARK).expect("histogram");
    let faces = faces(&r, grid, rect, well);
    assert!(faces.iter().any(|f| f.level == TileLevel::Red));
    let (chrome, red) = report("popover-peak", &faces);
    assert!(
        chrome >= CHROME_FLOOR,
        "chrome {chrome:.2} < {CHROME_FLOOR}"
    );
    assert!(
        red >= RED_POPOVER_FLOOR,
        "red {red:.2} < {RED_POPOVER_FLOOR}"
    );
}

#[test]
fn island_urgent_dots_read_on_their_well() {
    let r = render(island::scenes::app, "island-urgent");
    let (rect, well) = slot(&r.tree, "root/island/tint/veil/row/wave/tiles");
    let mut s = island::scenes::init();
    s.select_feed(Feed::DemoPeak);
    let grid = s.tile_grid(island::WAVE_MARK).expect("wave");
    let faces = faces(&r, grid, rect, well);
    assert!(faces.iter().all(|f| f.level == TileLevel::Red));
    let (_, red) = report("island-urgent", &faces);
    assert!(red >= RED_ISLAND_FLOOR, "red {red:.2} < {RED_ISLAND_FLOOR}");
}
