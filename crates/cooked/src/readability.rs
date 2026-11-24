//! Task 14 (F5, F10): how well the lit chrome tiles read on their well. Each scene is rendered headless at
//! `snaps::SNAP_T`; for every lit tile the mean luminance of its inner face (the tile less its outer 1 px ring) is
//! set against the well's fill with WCAG's `contrast_ratio`. The test prints the minimum per colour; the floor is
//! pinned only after Matteo approves the look (plan Task 14 step 2).

use std::path::PathBuf;

use cooked_model::contrast::{Rgb, contrast_ratio, relative_luminance};
use cooked_model::demo_data::Feed;
use cooked_model::tiles::TileLevel;

use crate::island;
use crate::popover;
use crate::snaps::SNAP_T;
use crate::snaps::tests::decode;
use crate::tiles::{TileGrid, TileGrids};

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

/// One lit tile's inner face: its contrast on the well and its mean colour (0...255).
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

/// Every lit tile of `grid`, drawn in `rect` (points) at `scale`, measured on `pixels` against `well`.
fn faces(r: &Render, grid: &TileGrid, cells: &[TileLevel], rect: [f64; 4], well: Rgb) -> Vec<Face> {
    let scale = r.tree["scale"].as_f64().expect("scale");
    let [x, y, w, h] = rect;
    let gap = grid.gap as f64;
    let tw = (w - gap * (grid.cols - 1) as f64) / grid.cols as f64;
    let th = (h - gap * (grid.rows - 1) as f64) / grid.rows as f64;
    let mut out = Vec::new();
    for row in 0..grid.rows {
        for c in 0..grid.cols {
            let level = cells[row * grid.cols + c];
            if level == TileLevel::Off {
                continue;
            }
            // whole device pixels inside the tile, less its outer 1 px ring
            let x0 = ((x + c as f64 * (tw + gap)) * scale).ceil() as usize + 1;
            let y0 = ((y + row as f64 * (th + gap)) * scale).ceil() as usize + 1;
            let x1 = ((x + c as f64 * (tw + gap) + tw) * scale).floor() as usize - 1;
            let y1 = ((y + row as f64 * (th + gap) + th) * scale).floor() as usize - 1;
            let (mut lum, mut sum, mut n) = (0.0, [0.0; 3], 0.0);
            for py in y0..y1 {
                for px in x0..x1 {
                    let i = (py * r.width + px) * 4;
                    let p = &r.pixels[i..i + 3];
                    let rgb = Rgb::new(
                        p[0] as f64 / 255.0,
                        p[1] as f64 / 255.0,
                        p[2] as f64 / 255.0,
                    );
                    lum += relative_luminance(rgb);
                    for k in 0..3 {
                        sum[k] += p[k] as f64;
                    }
                    n += 1.0;
                }
            }
            assert!(n > 0.0, "tile {c},{row} has no inner face");
            // a flat colour with the face's mean luminance stands for it in the ratio
            let grey = srgb(lum / n);
            out.push(Face {
                level,
                ratio: contrast_ratio(Rgb::new(grey, grey, grey), well),
                rgb: sum.map(|s| s / n),
            });
        }
    }
    out
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(f64::NAN)
}

/// Prints and returns the minimum contrast of `faces`, per colour and overall.
fn report(scene: &str, faces: &[Face]) -> f64 {
    let mut min_all = f64::INFINITY;
    for level in [TileLevel::Lit, TileLevel::Red] {
        let of: Vec<&Face> = faces.iter().filter(|f| f.level == level).collect();
        if of.is_empty() {
            continue;
        }
        let min = of.iter().map(|f| f.ratio).fold(f64::INFINITY, f64::min);
        min_all = min_all.min(min);
        let med = |k: usize| median(of.iter().map(|f| f.rgb[k]).collect());
        println!(
            "readability {scene} {}: tiles {} min {min:.2}:1 median {:.2}:1 median rgb ({:.0}, {:.0}, {:.0})",
            if level == TileLevel::Red {
                "red"
            } else {
                "chrome"
            },
            of.len(),
            median(of.iter().map(|f| f.ratio).collect()),
            med(0),
            med(1),
            med(2),
        );
    }
    println!("readability {scene} min {min_all:.2}:1");
    min_all
}

#[test]
fn popover_peak_tiles_read_on_their_well() {
    let r = render(popover::scenes::app, "popover-peak");
    let (rect, well) = slot(&r.tree, "root/popover/tint/veil/body/tpm/histogram/tiles");
    let p = popover::scenes::popover(Feed::DemoPeak);
    let grid = p.tile_grid(popover::HISTOGRAM_MARK).expect("histogram");
    let faces = faces(&r, grid, &grid.cells, rect, well);
    assert!(faces.iter().any(|f| f.level == TileLevel::Red));
    // no floor yet: Matteo approves the look first (Task 14 step 2)
    assert!(report("popover-peak", &faces) > 1.0);
}

#[test]
fn island_urgent_tiles_read_on_their_well() {
    let r = render(island::scenes::app, "island-urgent");
    let (rect, well) = slot(&r.tree, "root/island/tint/veil/row/wave/tiles");
    let mut s = island::scenes::init();
    s.select_feed(Feed::DemoPeak);
    let grid = s.tile_grid(island::WAVE_MARK).expect("wave");
    let mut cells = Vec::new();
    grid.cells_at(SNAP_T as f32, &mut cells);
    let faces = faces(&r, grid, &cells, rect, well);
    assert!(faces.iter().all(|f| f.level == TileLevel::Red));
    assert!(report("island-urgent", &faces) > 1.0);
}
