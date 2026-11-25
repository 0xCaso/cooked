//! Tokens per minute: the one big number, the alert line, the histogram in 72 x 26 chrome tiles (red above the
//! threshold row, all red in urgency) and the chart's ranges.

use cooked_model::histogram_clock::{ChartRange, MinuteHistory};
use cooked_model::island_model::IslandState;
use cooked_model::tide::{Motion as Flow, TideHistogram};
use cooked_model::tiles::{TileLevel, surface_grid};
use cooked_model::token_format;
use rune::{Color, Cursor, El, Shadow, col, el, row, text};

use super::{CARD_RADIUS, HISTOGRAM_MARK, Popover, WELL_RADIUS};
use crate::island::Prefs;
use crate::theme::{Motion, Palette, Rgb, TextStyle, card, well};
use crate::tiles::{TileGrid, tile_slot};

/// The histogram's tiles.
pub const COLS: usize = 72;
pub const ROWS: usize = 26;
/// The histogram's well: 150 pt tall, 8 pt around the tiles, near black.
const WELL_HEIGHT: f32 = 150.0;
const WELL_PADDING: f32 = 8.0;
const WELL_FILL: u32 = 0x050505;

/// The histogram for `range` of the chart's minutes at chart time `t`: the liquid (`TideHistogram`, lively at
/// 10 minutes, calm on the longer ranges) on the tiles, with lines between minutes where one spans two columns or
/// more, so the chart moves at 60 fps column by column (DESIGN.md) instead of in one-minute steps. `still`: the
/// liquid stands at its targets (Reduce Motion, scenes). `urgent`: an account fills before its reset, and the whole
/// liquid is red, as the pill's "full HH:MM".
pub fn histogram(
    tide: &mut TideHistogram,
    history: &MinuteHistory,
    range: ChartRange,
    t: f64,
    still: bool,
    urgent: bool,
) -> TileGrid {
    let motion = if range == ChartRange::TenMinutes {
        Flow::LIVELY
    } else {
        Flow::CALM
    };
    tide.surface(&history.window(range.minutes()), t, still, motion);
    let mut cells = surface_grid(
        tide.lit_rows(),
        tide.reaches(),
        tide.threshold_row(),
        COLS,
        ROWS,
    );
    if urgent {
        for c in cells.iter_mut().filter(|c| **c == TileLevel::Lit) {
            *c = TileLevel::Red;
        }
    }
    TileGrid::new(COLS, ROWS, cells)
}

/// The popover's liquid, on the histogram's tiles.
pub fn tide() -> TideHistogram {
    TideHistogram::sized(COLS, ROWS, true)
}

pub fn card_for(m: &IslandState<Prefs>) -> El {
    let snapshot = &m.snapshot;
    let urgent = snapshot.is_urgent();
    let hero = row()
        .key("hero-row")
        .w_full()
        .items_end()
        .gap(8.0)
        .mb(12.0)
        .child(
            text(token_format::short(m.chart_rate() as f64))
                .key("hero")
                .font(TextStyle::Hero.font())
                .color(if urgent {
                    Palette::red()
                } else {
                    Palette::ink()
                }),
        )
        .child(
            text("now")
                .key("hero-unit")
                .font(TextStyle::Body.font())
                .color(Palette::ink2()),
        )
        .child(el().grow(1.0))
        .child(alert(m));
    let chart = well(
        el().key("histogram")
            .w_full()
            .h(WELL_HEIGHT)
            .padding(WELL_PADDING)
            .child(tile_slot(HISTOGRAM_MARK).key("tiles").w_full().h_full()),
        WELL_RADIUS,
    )
    .fill(Rgb::hex(WELL_FILL).color())
    .gradient(Rgb::hex(WELL_FILL).color(), Rgb::hex(WELL_FILL).color());
    card(
        col()
            .key("tpm")
            .w_full()
            .pt(14.0)
            .px(14.0)
            .pb(12.0)
            .child(hero)
            .child(chart)
            .child(ranges(m.chart_range())),
        CARD_RADIUS,
    )
}

/// "alert at 40k/min", or "reading 14 days 42%" while the first launch reads the history the line needs.
fn alert(m: &IslandState<Prefs>) -> El {
    let label = |key: &'static str, t: String| {
        text(t)
            .key(key)
            .font(TextStyle::Label.font())
            .color(Palette::ink3())
    };
    let number = |t: String| {
        text(t)
            .key("alert-number")
            .font(TextStyle::LabelNumber.font())
            .color(Palette::ink2())
    };
    let line = row().key("alert").items_end();
    match m.history_progress() {
        Some(p) => line
            .child(label("alert-lead", "reading 14 days ".to_owned()))
            .child(number(format!("{}%", (p * 100.0) as i64))),
        None => line
            .child(label("alert-lead", "alert at ".to_owned()))
            .child(number(token_format::short(
                m.snapshot.chart_threshold() as f64
            )))
            .child(label("alert-unit", "/min".to_owned())),
    }
}

/// The chart's ranges under it, the chosen one on a chrome key; "now" at the right end.
fn ranges(chosen: ChartRange) -> El {
    let mut line = row().key("ranges").w_full().mt(10.0).gap(2.0).center_y();
    for range in ChartRange::ALL {
        line = line.child(range_key(range, range == chosen));
    }
    line.child(el().grow(1.0)).child(
        text("now")
            .key("ranges-now")
            .font(TextStyle::Label.font())
            .color(Palette::ink3()),
    )
}

fn range_key(range: ChartRange, on: bool) -> El {
    let font = if on {
        rune::TextStyle {
            weight: 500.0,
            ..TextStyle::LabelNumber.font()
        }
    } else {
        TextStyle::LabelNumber.font()
    };
    let key = el()
        .key(range.raw())
        .px(9.0)
        .py(3.0)
        .radius(7.0)
        .hit()
        .cursor(Cursor::Pointer)
        .press_scale(Motion::BUTTON_SCALE)
        .label(format!("Chart range {}", range.raw()))
        .on_press(move |s: &mut Popover| {
            s.model.borrow_mut().set_chart_range(range);
            s.step_chart();
        })
        .child(text(range.raw()).key("range-text").font(font).color(if on {
            Rgb::hex(0x0B0C0E).color()
        } else {
            Palette::ink3()
        }));
    if !on {
        return key.hover(Color::WHITE.alpha(0.06));
    }
    // a chrome key, lit on its top edge
    key.fill(Rgb::hex(0xD9DDE3).color())
        .linear_gradient(
            std::f32::consts::PI,
            [
                (0.0, Rgb::hex(0xE9EDF2).color()),
                (0.55, Rgb::hex(0x8E949D).color()),
                (1.0, Rgb::hex(0xD9DDE3).color()),
            ],
        )
        .box_shadow(Shadow::core_animation(
            0.0,
            1.0,
            1.0,
            Color::BLACK.alpha(0.6),
        ))
        .box_shadow(Shadow {
            y: 1.0,
            color: Color::WHITE,
            inset: true,
            ..Shadow::default()
        })
}
