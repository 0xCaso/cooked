//! Port of `Dots/DotShading.swift`: each dot's level and red flag. The LED radius and glow are dropped: the Rune
//! look draws chrome tiles, which have neither.

/// A fixed grid of dots. Each dot has a continuous level (0 off, 1 fully lit) and may be red.
/// Index is `c * rows + r`, row 0 at the top.
#[derive(Debug, Clone, PartialEq)]
pub struct DotGrid {
    pub cols: usize,
    pub rows: usize,
    pub level: Vec<f32>,
    pub hot: Vec<bool>,
}

impl DotGrid {
    pub fn new(cols: usize, rows: usize, mut f: impl FnMut(usize, usize) -> (f32, bool)) -> Self {
        let mut level = Vec::with_capacity(cols * rows);
        let mut hot = Vec::with_capacity(cols * rows);
        for c in 0..cols {
            for r in 0..rows {
                let (l, h) = f(c, r);
                level.push(l);
                hot.push(h);
            }
        }
        Self {
            cols,
            rows,
            level,
            hot,
        }
    }
}

/// How one dot is drawn: its opacity and whether it is red.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DotShade {
    pub alpha: f32,
    pub hot: bool,
}

pub fn smoothstep(a: f32, b: f32, v: f32) -> f32 {
    let t = ((v - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The mockup's `fluid()` renderer as numbers (src/lib.js): every value is continuous, so dots fade instead of switching.
pub struct DotShading;

impl DotShading {
    pub fn shade(g: &DotGrid, floor: impl Fn(usize) -> f32, vignette: bool) -> Vec<DotShade> {
        let mut out = Vec::with_capacity(g.level.len());
        const NEIGHBOURS: [(isize, isize); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
        for c in 0..g.cols {
            for r in 0..g.rows {
                let i = c * g.rows + r;
                let mut v = g.level[i];
                let mut hot = g.hot[i];
                // a lit neighbour lifts this dot a little, continuously
                let mut halo = 0.0f32;
                for (dc, dr) in NEIGHBOURS {
                    let (Some(nc), Some(nr)) = (c.checked_add_signed(dc), r.checked_add_signed(dr))
                    else {
                        continue;
                    };
                    if nc >= g.cols || nr >= g.rows {
                        continue;
                    }
                    let n = nc * g.rows + nr;
                    let h = 0.16 * smoothstep(0.6, 0.95, g.level[n]);
                    if h > halo {
                        halo = h;
                        if g.hot[n] && v < 0.16 {
                            hot = true;
                        }
                    }
                }
                v = v.max(halo);
                let rim = if vignette {
                    ((c.min(g.cols - 1 - c) + 1) as f32 / 4.0).min(1.0)
                } else {
                    1.0
                };
                let f = floor(r);
                out.push(DotShade {
                    alpha: (f + v * (0.98 - f)) * rim,
                    hot: hot && v > 0.05,
                });
            }
        }
        out
    }
}
