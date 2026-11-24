use std::cell::RefCell;

use cooked_model::demo_data::{self, Feed};
use cooked_model::histogram_clock::ChartRange;
use cooked_model::island_model::{DismissReason, IslandState};
use cooked_model::notice::Notice;
use cooked_model::tiles::TileLevel;
use cooked_model::token_sources::OpenTarget;
use cooked_model::usage_snapshot::Account;
use rune::{Key, KeyEvent};

use super::scenes::{self, popover};
use super::*;
use crate::island::Prefs;
use crate::snaps::tests::assert_matches_snap;

fn anchor(capsule_x: f64) -> Anchor {
    // a 1440 x 900 display with a 24 pt menu bar; the capsule 8 pt under it
    Anchor {
        capsule: Rect::new(capsule_x, 828.0, 290.0, 40.0),
        visible: Rect::new(0.0, 0.0, 1440.0, 876.0),
        primary_height: 900.0,
    }
}

// ---- placement ----

#[test]
fn the_page_hangs_centred_under_the_capsule() {
    let a = anchor(575.0);
    let f = frame(&a, 600.0);
    assert_eq!(f.width, WIDTH as f64 + 2.0 * MARGIN_X as f64);
    assert_eq!(f.mid_x(), a.capsule.mid_x());
    // the page's top edge GAP under the capsule; the margin above it
    assert_eq!(f.max_y() - MARGIN_TOP as f64, a.capsule.min_y() - GAP);
    assert_eq!(f.height, 600.0 + MARGIN_TOP as f64 + MARGIN_BOTTOM as f64);
}

#[test]
fn the_page_stays_on_its_display() {
    let left = frame(&anchor(0.0), 600.0);
    assert_eq!(left.x + MARGIN_X as f64, 0.0);
    let right = frame(&anchor(1440.0 - 290.0), 600.0);
    assert_eq!(right.max_x() - MARGIN_X as f64, 1440.0);
}

#[test]
fn a_tall_page_is_cut_at_the_display_bottom() {
    let f = frame(&anchor(575.0), 5000.0);
    let page_bottom = f.min_y() + MARGIN_BOTTOM as f64;
    assert_eq!(page_bottom, SCREEN_BOTTOM_GAP);
}

#[test]
fn placement_flips_to_rune_points() {
    let a = anchor(575.0);
    let f = frame(&a, 600.0);
    let OverlayPlacement::Rect {
        x,
        y,
        width,
        height,
    } = placement(&a, 600.0)
    else {
        panic!("a rect");
    };
    assert_eq!(
        (x, width, height),
        (f.x as f32, f.width as f32, f.height as f32)
    );
    // 24 menu bar + 8 gap + 40 capsule + 10 gap - 8 margin
    assert_eq!(y, 74.0);
}

// ---- state ----

thread_local! {
    static REQUESTS: RefCell<Vec<Request>> = const { RefCell::new(Vec::new()) };
}

fn record(r: Request) {
    REQUESTS.with(|q| q.borrow_mut().push(r));
}

fn taken() -> Vec<Request> {
    REQUESTS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

fn open(feed: Feed) -> Popover {
    let mut p = popover(feed);
    p.model.borrow_mut().open();
    p.requests = std::rc::Rc::new(record);
    p
}

#[test]
fn esc_and_any_key_close_it() {
    let mut tx = rune::Tx::default();
    for (key, reason) in [(Key::Escape, "esc"), (Key::Text("a".into()), "typing")] {
        let mut p = open(Feed::DemoCalm);
        assert!(
            on_key(
                &mut p,
                &KeyEvent {
                    key,
                    mods: Default::default()
                },
                &mut tx
            ),
            "{reason}"
        );
        assert!(!p.is_shown(), "{reason}");
        assert!(!p.model.borrow().is_open, "{reason}");
    }
}

#[test]
fn losing_the_keyboard_closes_it_and_marks_the_click() {
    let mut tx = rune::Tx::default();
    let mut p = open(Feed::DemoCalm);
    // never had the keyboard: losing it says nothing
    on_focus(&mut p, false, &mut tx);
    assert!(p.is_shown());
    on_focus(&mut p, true, &mut tx);
    assert!(p.is_shown());
    on_focus(&mut p, false, &mut tx);
    assert!(!p.is_shown());
    let m = p.model.borrow();
    assert!(!m.is_open);
    // the island's own click right after doesn't reopen it
    assert!(m.closed_by_this_click(cooked_model::clock::now()));
}

#[test]
fn open_flag_pins_it_until_a_key() {
    let mut p = Popover::new(
        popover(Feed::DemoCalm).model,
        &["cooked".into(), "--open".into()],
    );
    p.shown = true;
    p.dismiss(DismissReason::ClickOutside);
    p.dismiss(DismissReason::ResignedKey);
    assert!(p.is_shown());
    p.dismiss(DismissReason::Escape);
    assert!(!p.is_shown());
}

#[test]
fn closing_folds_the_list_again() {
    let mut p = open(Feed::DemoPeak);
    p.expanded = true;
    p.dismiss(DismissReason::ClickOutside);
    assert!(!p.expanded);
}

#[test]
fn grids_by_mark() {
    let p = popover(Feed::DemoPeak);
    let h = p.tile_grid(HISTOGRAM_MARK).expect("histogram");
    assert_eq!((h.cols, h.rows), (tpm::COLS, tpm::ROWS));
    // the peak goes over the threshold: red tiles, and lit ones under the line
    assert!(h.cells.contains(&TileLevel::Red) && h.cells.contains(&TileLevel::Lit));
    let rows = p.model.borrow().snapshot.rows().len();
    for i in 0..2 * rows as u32 {
        let b = p.tile_grid(FIRST_BAR_MARK + i).expect("bar");
        assert_eq!((b.cols, b.rows), usage::BAR);
    }
    assert!(p.tile_grid(FIRST_BAR_MARK + 2 * rows as u32).is_none());
    assert!(p.tile_grid(0).is_none());
}

#[test]
fn a_range_redraws_the_chart_and_is_kept() {
    let mut p = popover(Feed::DemoPeak);
    let ten = p.histogram.clone();
    p.model.borrow_mut().set_chart_range(ChartRange::Day);
    p.step_chart();
    assert_ne!(p.histogram, ten);
    assert_eq!(p.model.borrow().chart_range(), ChartRange::Day);
}

#[test]
fn notice_buttons_ask_the_app() {
    let mut p = open(Feed::DemoCalm);
    p.act(&Notice::NoAccounts);
    p.act(&Notice::Paused);
    let changed = Notice::FormatChanged {
        account: "work".into(),
        file: "/x.jsonl".into(),
    };
    p.act(&changed);
    assert!(p.copied);
    assert_eq!(
        taken(),
        [
            Request::OpenSetup,
            Request::OpenPercentSettings,
            Request::Copy(changed.details().unwrap()),
        ]
    );
    // "Not now" hides it until the accounts change
    let missing = Notice::NoPercent {
        id: "work".into(),
        account: "work".into(),
    };
    p.model.borrow_mut().show(vec![missing.clone()]);
    p.put_off(&missing);
    assert_eq!(p.model.borrow().notice(), None);
}

#[test]
fn notice_buttons_per_notice() {
    let percent = Notice::NoPercent {
        id: "a".into(),
        account: "a".into(),
    };
    assert_eq!(
        notice::buttons(&percent, false),
        [("Connect", true), ("Not now", false)]
    );
    assert_eq!(
        notice::buttons(&Notice::NoAccounts, false),
        [("Set up…", true)]
    );
    let changed = Notice::FormatChanged {
        account: "a".into(),
        file: "f".into(),
    };
    assert_eq!(notice::buttons(&changed, true), [("Copied", false)]);
}

// ---- cards ----

#[test]
fn burners_show_three_until_opened() {
    let mut peak = IslandState::<Prefs>::tallest(5);
    assert!(peak.sources.len() > burners::COLLAPSED);
    assert_eq!(
        burners::shown(&peak.sources, false).len(),
        burners::COLLAPSED
    );
    assert_eq!(
        burners::shown(&peak.sources, true).len(),
        peak.sources.len()
    );
    // red only in urgency, on the top source of the account that fills
    assert_eq!(burners::hot(&peak), Some("t1"));
    peak.urgency = None;
    assert_eq!(burners::hot(&peak), None);
}

#[test]
fn opening_a_source_asks_for_its_thread() {
    let p = open(Feed::DemoPeak);
    let target = p.model.borrow().snapshot.sources[0]
        .target()
        .expect("t1 has a thread");
    p.request(Request::OpenSession(target.clone()));
    assert_eq!(taken(), [Request::OpenSession(OpenTarget::t3_code("t1"))]);
}

/// `PaceTests`: the pills count what is left.
#[test]
fn pills_count_what_is_left() {
    assert_eq!(Account::left(74), 26);
    assert_eq!(Account::left(140), 0);
}

#[test]
fn pill_times_follow_the_red_rule() {
    let peak = demo_data::peak();
    let work = &peak.accounts[0];
    assert!(usage::fills(work));
    assert_eq!(
        usage::five_hour_times(work),
        Some(("full 23:14".into(), true))
    );
    assert_eq!(usage::week_times(work), Some(("resets Mon".into(), false)));
    let team = &peak.accounts[1];
    assert!(!usage::fills(team));
    assert_eq!(
        usage::five_hour_times(team),
        Some(("resets 04:40".into(), false))
    );
    let mut blocked = team.clone();
    blocked.is_blocked = true;
    assert_eq!(
        usage::five_hour_times(&blocked),
        Some(("resets 04:40".into(), true))
    );
    let mut unknown = team.clone();
    unknown.has_percent = false;
    assert_eq!(usage::five_hour_times(&unknown), None);
    let mut week_full = team.clone();
    week_full.week_full_at = Some("18:00".into());
    assert_eq!(
        usage::week_times(&week_full),
        Some(("full 18:00".into(), true))
    );
}

#[test]
fn unused_accounts_fold_into_one_line() {
    assert_eq!(
        usage::unused_line(&demo_data::calm()).as_deref(),
        Some("personal")
    );
    assert_eq!(usage::unused_line(&demo_data::peak()), None);
    // the folded account has no pill and no bars
    let calm = demo_data::calm();
    assert_eq!(usage::bars(&calm).len(), 2 * (calm.accounts.len() - 1));
}

#[test]
fn a_bar_that_runs_out_has_red_tiles() {
    let bars = usage::bars(&demo_data::peak());
    // work: 5 hours fills before the reset; its week doesn't
    assert!(bars[0].cells.contains(&TileLevel::Red));
    assert!(!bars[1].cells.contains(&TileLevel::Red));
    assert!(bars[1].cells.contains(&TileLevel::Lit));
}

// ---- scenes ----

/// Runs a scene through Rune's `--tree` and returns the tree.
fn tree(scene: &str) -> serde_json::Value {
    let dir = std::env::temp_dir().join(format!("cooked-tree-{}-{scene}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let out = dir.join("tree.json");
    scenes::app()
        .args([
            "--tree".to_owned(),
            out.to_string_lossy().into_owned(),
            "--scene".to_owned(),
            scene.to_owned(),
        ])
        .try_run()
        .expect("headless tree");
    let json = std::fs::read_to_string(&out).expect("tree written");
    let _ = std::fs::remove_dir_all(&dir);
    serde_json::from_str(&json).expect("tree json")
}

/// Every text in the tree with its color.
fn texts(t: &serde_json::Value) -> Vec<(String, String)> {
    t["elements"]
        .as_array()
        .expect("elements")
        .iter()
        .filter_map(|e| {
            Some((
                e.get("text")?.as_str()?.to_owned(),
                e.get("color")?.as_str()?.to_owned(),
            ))
        })
        .collect()
}

fn color_of(t: &[(String, String)], text: &str) -> String {
    t.iter()
        .find(|(s, _)| s == text)
        .unwrap_or_else(|| panic!("no text {text:?} in {t:?}"))
        .1
        .clone()
}

const RED: &str = "#ff453aff";
const INK: &str = "#f4f1eaff";

#[test]
fn peak_scene_tree() {
    let t = texts(&tree("popover-peak"));
    assert_eq!(color_of(&t, "184k"), RED, "the hero is red in urgency");
    assert_eq!(color_of(&t, "now"), "#f4f1eab8");
    assert_eq!(color_of(&t, "113k"), RED, "the source that fills");
    assert_eq!(color_of(&t, "18k"), INK);
    assert_eq!(color_of(&t, "full 23:14"), RED);
    assert_eq!(color_of(&t, "resets 04:40"), "#f4f1ea8c");
    for label in [
        "Tokens per minute",
        "What burns tokens",
        "last 5 min",
        "Usage",
        "5 hours",
        "week",
    ] {
        assert_eq!(color_of(&t, label), "#f4f1ea8c", "{label}");
    }
    // ranges, the chosen one dark on its chrome key
    for r in ["30m", "1h", "4h", "1d"] {
        assert_eq!(color_of(&t, r), "#f4f1ea8c");
    }
    assert_eq!(color_of(&t, "10m"), "#0b0c0eff");
    assert!(t.iter().any(|(s, _)| s == "18%"));
}

#[test]
fn calm_scene_tree() {
    let t = texts(&tree("popover-calm"));
    let calm = demo_data::calm();
    assert_eq!(
        color_of(&t, &cooked_model::token_format::short(calm.rate_now as f64)),
        INK
    );
    assert!(
        !t.iter().any(|(s, _)| s == "What burns tokens"),
        "no sources, no card"
    );
    assert_eq!(color_of(&t, "personal"), "#f4f1eab8", "the folded line");
    assert!(!t.iter().any(|(_, c)| c == RED), "no red when calm: {t:?}");
}

#[test]
fn notice_scene_tree() {
    let t = texts(&tree("popover-notice"));
    assert_eq!(color_of(&t, "No 5-hour percent for work"), INK);
    assert_eq!(color_of(&t, "Connect"), "#111111ff");
    assert_eq!(color_of(&t, "Not now"), INK);
}

#[test]
fn popover_calm_matches_the_checked_in_snap() {
    assert_matches_snap(scenes::app(), "popover-calm");
}

#[test]
fn popover_peak_matches_the_checked_in_snap() {
    assert_matches_snap(scenes::app(), "popover-peak");
}

#[test]
fn popover_notice_matches_the_checked_in_snap() {
    assert_matches_snap(scenes::app(), "popover-notice");
}
