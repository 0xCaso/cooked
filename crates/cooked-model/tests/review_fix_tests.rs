//! Findings of the final review, each pinned by a test. The cases that drive AppKit live in `crates/cooked`.

use cooked_model::defaults::MemoryDefaults;
use cooked_model::demo_data;
use cooked_model::island_model::IslandState;

#[test]
fn chart_epoch_survives_open_close_and_demo() {
    let mut model = IslandState::new(demo_data::calm(), None, MemoryDefaults::default(), 1234.5);
    let epoch = model.chart_epoch();
    model.open();
    model.close();
    model.toggle_demo();
    model.toggle_demo();
    assert_eq!(model.chart_epoch(), epoch);
}
