//! Port of `Dots/HistogramClock.swift`: the popover chart's minutes, from the demo clock or the live data, and how far
//! back it looks (`ChartRange`).

/// The last minutes in tokens per minute, oldest first, the minute in progress last.
/// `top` is the top of the scale, `threshold` the p99 threshold.
#[derive(Debug, Clone, PartialEq)]
pub struct MinuteHistory {
    pub values: Vec<f64>,
    pub top: f64,
    pub threshold: f64,
}

impl MinuteHistory {
    /// Never below 1.6 × the threshold, so a calm hour does not fill the chart.
    pub fn top(values: &[f64], threshold: f64) -> f64 {
        let max = values.iter().copied().reduce(f64::max).unwrap_or(0.0);
        max.max(threshold * 1.6).max(1.0)
    }

    /// The last `minutes` minutes, with empty minutes before the first one known, and the scale's top for them alone.
    pub fn window(&self, minutes: usize) -> MinuteHistory {
        let known = &self.values[self.values.len().saturating_sub(minutes)..];
        let mut values = vec![0.0; minutes.saturating_sub(known.len())];
        values.extend_from_slice(known);
        MinuteHistory {
            top: Self::top(&values, self.threshold),
            values,
            threshold: self.threshold,
        }
    }
}

/// How far back the popover's chart looks: its columns re-bin the range's minutes, each column the average tokens
/// per minute of the minutes it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ChartRange {
    #[default]
    TenMinutes,
    HalfHour,
    Hour,
    FourHours,
    Day,
}

impl ChartRange {
    pub const ALL: [ChartRange; 5] = [
        ChartRange::TenMinutes,
        ChartRange::HalfHour,
        ChartRange::Hour,
        ChartRange::FourHours,
        ChartRange::Day,
    ];
    pub const DEFAULTS_KEY: &'static str = "chartRange";

    /// The Swift raw value, as saved in the defaults.
    pub fn raw(self) -> &'static str {
        match self {
            ChartRange::TenMinutes => "10m",
            ChartRange::HalfHour => "30m",
            ChartRange::Hour => "1h",
            ChartRange::FourHours => "4h",
            ChartRange::Day => "1d",
        }
    }

    pub fn from_raw(raw: &str) -> Option<ChartRange> {
        Self::ALL.into_iter().find(|r| r.raw() == raw)
    }

    pub fn minutes(self) -> usize {
        match self {
            ChartRange::TenMinutes => 10,
            ChartRange::HalfHour => 30,
            ChartRange::Hour => 60,
            ChartRange::FourHours => 4 * 60,
            ChartRange::Day => 24 * 60,
        }
    }
}

/// The demo's minutes: the minute in progress grows as tokens arrive; when it closes, the history moves left
/// by one and `next` gives the new minute's total. The chart's springs carry the motion.
pub struct HistogramClock {
    minute_length: f64,
    next: Box<dyn FnMut() -> f64 + Send>,
    top: f64,
    threshold: f64,
    values: Vec<f64>,
    target: f64,
    minute: Option<i64>,
}

impl HistogramClock {
    pub fn new(
        values: Vec<f64>,
        threshold: f64,
        minute_length: f64,
        next: Box<dyn FnMut() -> f64 + Send>,
    ) -> Self {
        Self {
            target: values.last().copied().unwrap_or(0.0),
            top: MinuteHistory::top(&values, threshold),
            values,
            threshold,
            minute_length,
            next,
            minute: None,
        }
    }

    /// The history at chart time `t`. `still`: no growth (Reduce Motion, snapshots).
    pub fn history(&mut self, t: f64, still: bool) -> MinuteHistory {
        let m = (t / self.minute_length).floor() as i64;
        if let Some(minute) = self.minute
            && m != minute
            && let Some(last) = self.values.last_mut()
        {
            *last = self.target;
            self.values.remove(0);
            self.values.push(0.0);
            self.target = (self.next)();
        }
        self.minute = Some(m);
        let mut values = self.values.clone();
        if let Some(last) = values.last_mut() {
            let phase = (t % self.minute_length) / self.minute_length;
            *last = self.target * if still { 1.0 } else { phase };
        }
        MinuteHistory {
            values,
            top: self.top,
            threshold: self.threshold,
        }
    }
}

/// The real last minutes. Minutes follow the wall clock: when one closes, the history moves left and a new empty
/// minute opens, even between readings.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveHistogram {
    epoch: f64,
    values: Vec<f64>,
    threshold: f64,
    /// Wall-clock minute (Unix seconds / 60) of the last value.
    minute: i64,
}

impl LiveHistogram {
    /// `epoch`: the chart clock's time zero, Unix seconds. `minute`: the wall-clock minute of `values.last`.
    pub fn new(values: Vec<f64>, threshold: f64, minute: i64, epoch: f64) -> Self {
        Self {
            epoch,
            values,
            threshold,
            minute,
        }
    }

    /// New data from the database.
    pub fn update(&mut self, values: Vec<f64>, threshold: f64, minute: i64) {
        self.values = values;
        self.threshold = threshold;
        self.minute = minute;
    }

    pub fn history(&mut self, t: f64, still: bool) -> MinuteHistory {
        let m = ((self.epoch + t) / 60.0).floor() as i64;
        if !still && m > self.minute {
            let d = self.values.len().min((m - self.minute) as usize);
            self.values.drain(..d);
            self.values.extend(std::iter::repeat_n(0.0, d));
            self.minute = m;
        }
        MinuteHistory {
            values: self.values.clone(),
            top: MinuteHistory::top(&self.values, self.threshold),
            threshold: self.threshold,
        }
    }
}

/// What the popover histogram draws from: the demo clock or the live data (`HistogramSource`).
pub enum Histogram {
    Clock(HistogramClock),
    Live(LiveHistogram),
}

impl Histogram {
    /// The history at chart time `t`. `still`: no growth (Reduce Motion, snapshots).
    pub fn history(&mut self, t: f64, still: bool) -> MinuteHistory {
        match self {
            Histogram::Clock(c) => c.history(t, still),
            Histogram::Live(l) => l.history(t, still),
        }
    }
}
