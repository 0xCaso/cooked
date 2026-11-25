//! Token counts as short labels.

/// 950 → "950", 9 000 → "9k", 1 200 000 → "1.2M".
pub fn short(n: f64) -> String {
    if n >= 1_000_000.0 {
        return format!("{:.1}M", n / 1_000_000.0);
    }
    if n >= 1000.0 {
        return format!("{}k", (n / 1000.0).round() as i64);
    }
    format!("{}", n.round() as i64)
}
