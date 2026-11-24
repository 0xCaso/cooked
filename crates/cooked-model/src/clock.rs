//! Time without a calendar library: Unix seconds as `f64` (Swift's `Date.timeIntervalSince1970`), civil dates in the
//! proleptic Gregorian calendar, and the local zone behind a trait (the app gives the real one).

/// The UTC offset in effect at a moment, in seconds east of UTC. Stands in for Swift's `Calendar.current`.
pub trait TimeZone {
    fn utc_offset(&self, unix: f64) -> i64;
}

/// A zone with no DST: tests, and UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FixedOffset(pub i64);

impl TimeZone for FixedOffset {
    fn utc_offset(&self, _unix: f64) -> i64 {
        self.0
    }
}

/// Days since 1970-01-01 of a civil date (Howard Hinnant's algorithm).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// (year, month, day) of a day count since 1970-01-01.
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        if m <= 2 {
            yoe + era * 400 + 1
        } else {
            yoe + era * 400
        },
        m,
        d,
    )
}

pub fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Local wall-clock fields of a moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Civil {
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub hour: i64,
    pub minute: i64,
    pub second: i64,
}

pub fn local(unix: f64, tz: &dyn TimeZone) -> Civil {
    let secs = unix.floor() as i64 + tz.utc_offset(unix);
    let (year, month, day) = civil_from_days(secs.div_euclid(86_400));
    let rest = secs.rem_euclid(86_400);
    Civil {
        year,
        month,
        day,
        hour: rest / 3600,
        minute: rest % 3600 / 60,
        second: rest % 60,
    }
}

/// The moment of local wall-clock fields, lenient like `Calendar.date(from:)`: month 13 is next January, hour 25 the
/// next day. A time skipped or repeated by DST takes the offset in effect just before it.
pub fn from_local(c: Civil, tz: &dyn TimeZone) -> f64 {
    let year = c.year + (c.month - 1).div_euclid(12);
    let month = (c.month - 1).rem_euclid(12) + 1;
    let days = days_from_civil(year, month, 1) + c.day - 1;
    let wall = (days * 86_400 + c.hour * 3600 + c.minute * 60 + c.second) as f64;
    let guess = wall - tz.utc_offset(wall) as f64;
    wall - tz.utc_offset(guess) as f64
}

/// An ISO 8601 time with fractional seconds, as `ISO8601DateFormatter` with `.withInternetDateTime,
/// .withFractionalSeconds` reads it: `2026-10-08T16:46:11.683Z` or `...683+02:00`. Without a fraction it is nil.
pub fn iso_date(s: &str) -> Option<f64> {
    let b = s.as_bytes();
    let num = |from: usize, len: usize| -> Option<i64> {
        let part = b.get(from..from + len)?;
        part.iter()
            .all(u8::is_ascii_digit)
            .then(|| part.iter().fold(0i64, |n, d| n * 10 + i64::from(d - b'0')))
    };
    let (year, month, day) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    let (hour, minute, second) = (num(11, 2)?, num(14, 2)?, num(17, 2)?);
    if b.get(4) != Some(&b'-') || b.get(7) != Some(&b'-') || !matches!(b.get(10), Some(b'T' | b't'))
    {
        return None;
    }
    if b.get(13) != Some(&b':') || b.get(16) != Some(&b':') || b.get(19) != Some(&b'.') {
        return None;
    }
    if !(1..=12).contains(&month) || day < 1 || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let mut i = 20;
    while b.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    if i == 20 {
        return None;
    }
    let fraction: f64 = s.get(19..i)?.parse().ok()?;
    let offset = match b.get(i)? {
        b'Z' | b'z' if i + 1 == b.len() => 0,
        sign @ (b'+' | b'-') => {
            let h = num(i + 1, 2)?;
            let (m, end) = if b.get(i + 3) == Some(&b':') {
                (num(i + 4, 2)?, i + 6)
            } else {
                (num(i + 3, 2)?, i + 5)
            };
            if end != b.len() {
                return None;
            }
            let o = h * 3600 + m * 60;
            if *sign == b'-' { -o } else { o }
        }
        _ => return None,
    };
    let days = days_from_civil(year, month, day);
    Some((days * 86_400 + hour * 3600 + minute * 60 + second - offset) as f64 + fraction)
}

/// A moment as `ISO8601DateFormatter` writes it, in UTC: `2026-10-08T16:46:11.683Z` with `fractional` (milliseconds),
/// `2026-10-08T16:46:11Z` without.
pub fn iso_string(unix: f64, fractional: bool) -> String {
    let ms = (unix * 1000.0).floor() as i64;
    let secs = ms.div_euclid(1000);
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let rest = secs.rem_euclid(86_400);
    let (hh, mm, ss) = (rest / 3600, rest % 3600 / 60, rest % 60);
    if fractional {
        format!(
            "{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.{:03}Z",
            ms.rem_euclid(1000)
        )
    } else {
        format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
    }
}

/// Unix seconds of a file time.
pub fn unix(t: std::time::SystemTime) -> f64 {
    match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs_f64(),
        Err(e) => -e.duration().as_secs_f64(),
    }
}

/// Now, in Unix seconds.
pub fn now() -> f64 {
    unix(std::time::SystemTime::now())
}
