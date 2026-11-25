//! The 5h bar of one account, 20 dots, one per 5%.

use crate::dot_shading::DotGrid;

pub struct AccountBar;

impl AccountBar {
    pub const COLS: usize = 20;

    pub fn grid(percent: i64, hot: bool) -> DotGrid {
        let p = percent.clamp(0, 100);
        let lit = if p > 0 {
            ((p as f64 / 5.0).round() as usize).max(1)
        } else {
            0
        };
        DotGrid::new(Self::COLS, 1, |c, _| (if c < lit { 1.0 } else { 0.0 }, hot))
    }
}
