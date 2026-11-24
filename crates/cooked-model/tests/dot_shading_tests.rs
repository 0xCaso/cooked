//! `DotShadingTests.swift`: the per-dot level and red flag. The radius and glow asserts are dropped: chrome tiles
//! have neither (`unlitDotSitsAtFloor`, `litDotGlows` keep their alpha asserts only).

use cooked_model::dot_shading::{DotGrid, DotShade, DotShading};

fn floor(_: usize) -> f32 {
    0.07
}

fn grid3(centre: f32, hot: bool) -> DotGrid {
    DotGrid::new(3, 3, |c, r| {
        if c == 1 && r == 1 {
            (centre, hot)
        } else {
            (0.0, false)
        }
    })
}

fn shade(g: &DotGrid, c: usize, r: usize, vignette: bool) -> DotShade {
    DotShading::shade(g, floor, vignette)[c * g.rows + r]
}

#[test]
fn unlit_dot_sits_at_floor() {
    let s = shade(&grid3(0.0, false), 1, 1, false);
    assert!((s.alpha - 0.07).abs() < 1e-5);
}

#[test]
fn lit_dot_glows() {
    let s = shade(&grid3(1.0, false), 1, 1, false);
    assert!((s.alpha - 0.98).abs() < 1e-5);
}

#[test]
fn neighbour_gets_halo() {
    let s = shade(&grid3(1.0, false), 0, 1, false);
    assert!((s.alpha - (0.07 + 0.16 * 0.91)).abs() < 1e-5);
    let none = shade(&grid3(0.5, false), 0, 1, false);
    assert!((none.alpha - 0.07).abs() < 1e-5);
    let diagonal = shade(&grid3(1.0, false), 0, 0, false);
    assert!((diagonal.alpha - 0.07).abs() < 1e-5);
}

#[test]
fn hot_spreads_to_halo() {
    assert!(shade(&grid3(1.0, true), 1, 0, false).hot);
    assert!(!shade(&grid3(1.0, false), 1, 0, false).hot);
    assert!(!shade(&grid3(0.0, true), 1, 1, false).hot);
}

#[test]
fn vignette_dims_rim() {
    let g = DotGrid::new(9, 1, |_, _| (1.0, false));
    assert!((shade(&g, 0, 0, true).alpha - 0.98 * 0.25).abs() < 1e-5);
    assert!((shade(&g, 4, 0, true).alpha - 0.98).abs() < 1e-5);
    assert!((shade(&g, 0, 0, false).alpha - 0.98).abs() < 1e-5);
}
