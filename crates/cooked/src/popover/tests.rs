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
    Anchor::island(
        Rect::new(capsule_x, 828.0, 290.0, 40.0),
        Rect::new(0.0, 0.0, 1440.0, 876.0),
        900.0,
    )
}

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

/// Centred under the menu bar item, `GAP_BELOW_ITEM`
/// below it; next to the clock it is pushed left so the page ends at the screen edge.
#[test]
fn popover_hangs_under_the_item_and_stays_on_screen() {
    use cooked_model::island_placement::GAP_BELOW_ITEM;
    let visible = Rect::new(0.0, 0.0, 1512.0, 945.0);
    let middle = frame(
        &Anchor::item(Rect::new(700.0, 945.0, 60.0, 37.0), visible, 982.0),
        700.0,
    );
    assert_eq!(middle.mid_x(), 730.0);
    assert_eq!(middle.max_y() - MARGIN_TOP as f64, 945.0 - GAP_BELOW_ITEM);
    let edge = frame(
        &Anchor::item(Rect::new(1400.0, 945.0, 60.0, 37.0), visible, 982.0),
        700.0,
    );
    assert_eq!(edge.max_x(), 1512.0 + MARGIN_X as f64);
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
        &crate::cli::Launch {
            open: true,
            ..Default::default()
        },
    );
    p.shown = true;
    p.dismiss(DismissReason::ClickOutside);
    p.dismiss(DismissReason::ResignedKey);
    assert!(p.is_shown());
    p.dismiss(DismissReason::Escape);
    assert!(!p.is_shown());
}

/// Reduce Motion going off while the popover is up: the chart moves again, and the page draws a frame so its
/// metal reads the setting; `--reduce-motion` keeps both still.
#[test]
fn reduce_motion_follows_the_system_while_open() {
    let mut p = open(Feed::DemoCalm);
    p.still_at = None;
    p.reduce_motion = true;
    assert!(p.system_reduce_motion(false), "shown: draw a frame");
    assert!(!p.reduce_motion);
    p.hide();
    assert!(!p.system_reduce_motion(true), "hidden: nothing to draw");
    assert!(p.reduce_motion);
    let mut flagged = Popover::new(
        popover(Feed::DemoCalm).model,
        &crate::cli::Launch {
            reduce_motion: true,
            ..Default::default()
        },
    );
    flagged.system_reduce_motion(false);
    assert!(flagged.reduce_motion);
}

#[test]
fn closing_folds_the_list_again() {
    let mut p = open(Feed::DemoPeak);
    p.expanded = true;
    p.dismiss(DismissReason::ClickOutside);
    assert!(!p.expanded);
}

/// The peak with `accounts` accounts and the full sources list, folded or open.
fn tall(accounts: usize, expanded: bool) -> Popover {
    let mut p = popover(Feed::DemoPeak);
    p.model.borrow_mut().snapshot = IslandState::<Prefs>::tallest(accounts);
    p.expanded = expanded;
    p.refresh();
    p
}

/// The open list and 7 accounts under a capsule with only `SHORT_ROOM` points free below it.
fn tall_on_a_short_display() -> Popover {
    let mut p = tall(7, true);
    p.anchor = Some(short_anchor());
    p
}

/// A display whose visible frame ends `SHORT_ROOM` points under the page's top.
const SHORT_ROOM: f64 = 400.0;

fn short_anchor() -> Anchor {
    let a = anchor(575.0);
    let page_top = a.capsule.min_y() - a.gap;
    let bottom = page_top - SHORT_ROOM - SCREEN_BOTTOM_GAP;
    Anchor {
        visible: Rect::new(0.0, bottom, 1440.0, 876.0 - bottom),
        ..a
    }
}

/// A scene of `tall` through Rune's `--tree` (after `script`, if any) on a window tall enough for any of them.
fn tall_tree(scene: &str, script: Option<&str>) -> serde_json::Value {
    let dir = std::env::temp_dir().join(format!("cooked-tall-{}-{scene}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let out = dir.join("tree.json");
    let mut args = vec![
        "--tree".to_owned(),
        out.to_string_lossy().into_owned(),
        "--scene".to_owned(),
        scene.to_owned(),
    ];
    if let Some(script) = script {
        let file = dir.join("script.json");
        std::fs::write(&file, script).expect("script written");
        args.extend(["--script".to_owned(), file.to_string_lossy().into_owned()]);
    }
    rune::App::new(rune::AppSpec {
        scenes: &[
            ("folded-5", |s: &mut Popover| *s = tall(5, false)),
            ("open-5", |s: &mut Popover| *s = tall(5, true)),
            ("open-7", |s: &mut Popover| *s = tall(7, true)),
            ("open-7-short", |s: &mut Popover| {
                *s = tall_on_a_short_display()
            }),
        ],
        fonts: crate::theme::PlexFont::FILES,
        families: crate::theme::PlexFont::FAMILIES,
        ..rune::AppSpec::new(
            "Cooked popover",
            (scenes::SIZE.0, 2000.0),
            || tall(5, false),
            view,
        )
    })
    .args(args)
    .try_run()
    .expect("headless tree");
    let json = std::fs::read_to_string(&out).expect("tree written");
    let _ = std::fs::remove_dir_all(&dir);
    serde_json::from_str(&json).expect("tree json")
}

/// The rect (x, y, width, height) of the element keyed `key`.
fn rect_of(t: &serde_json::Value, key: &str) -> [f64; 4] {
    let e = t["elements"]
        .as_array()
        .expect("elements")
        .iter()
        .find(|e| e.get("key").and_then(|k| k.as_str()) == Some(key))
        .unwrap_or_else(|| panic!("no element {key:?}"));
    let r: Vec<f64> = e["rect"]
        .as_array()
        .expect("rect")
        .iter()
        .filter_map(|v| v.as_f64())
        .collect();
    [r[0], r[1], r[2], r[3]]
}

/// The page's height in a scene of `tall`.
fn page_height(scene: &str) -> f64 {
    rect_of(&tall_tree(scene, None), "popover")[3]
}

/// Taller than the room under the capsule, the page stops at the room's bottom and its lower part (what burns
/// tokens, the usage) scrolls: the last account is out of view, then a swipe over it brings it in.
#[test]
fn a_page_taller_than_the_room_scrolls_its_lower_part() {
    assert!(
        page_height("open-7") > SHORT_ROOM + 100.0,
        "the page is taller"
    );
    let t = tall_tree("open-7-short", None);
    let page = rect_of(&t, "popover");
    assert!(page[3] <= SHORT_ROOM + 0.5, "the page {page:?}");
    let f = frame(&short_anchor(), page[3] as f32);
    assert!(
        f.height - (MARGIN_TOP + MARGIN_BOTTOM) as f64 >= page[3] - 0.5,
        "the window holds the page"
    );
    let lower = rect_of(&t, "lower");
    let (lower_top, lower_bottom) = (lower[1], lower[1] + lower[3]);
    assert!(
        lower_bottom <= page[1] + page[3] + 0.5,
        "the scroll ends inside the page"
    );
    let last = rect_of(&t, "account-measure-6");
    assert!(
        last[1] > lower_bottom,
        "the last account starts out of view: {last:?}"
    );
    let script = r#"[{"t": 0.1, "type": "scroll", "key": "lower", "dy": -3000, "dur": 0.3}]"#;
    let scrolled = tall_tree("open-7-short", Some(script));
    let last = rect_of(&scrolled, "account-measure-6");
    let last_bottom = last[1] + last[3];
    assert!(
        last[1] >= lower_top && last_bottom <= lower_bottom + 0.5,
        "the last account scrolled into view: {last:?} in {lower:?}"
    );
}

/// A scroll moves the lower content and reports its layout every frame: only a change of whether it scrolls runs
/// the view again.
#[test]
fn a_scroll_runs_the_view_only_when_the_overflow_changes() {
    let mut p = popover(Feed::DemoCalm);
    assert!(
        !p.measure_lower(Some(400.0), Some(300.0)),
        "fits, as before"
    );
    assert!(p.measure_lower(None, Some(900.0)), "now overflows");
    assert!(
        !p.measure_lower(None, Some(900.0)),
        "same height on a scroll frame"
    );
    assert!(!p.measure_lower(Some(400.2), None), "under half a point");
    assert!(p.measure_lower(Some(950.0), None), "fits again");
}

/// The page grows with the open sources list and with more
/// accounts (its overlay follows the measured height, `Popover::measured`).
#[test]
fn the_open_list_makes_the_page_taller() {
    let folded = page_height("folded-5");
    let open = page_height("open-5");
    let seven = page_height("open-7");
    assert!(open > folded + 100.0, "open {open}, folded {folded}");
    assert!(seven > open, "7 accounts {seven}, 5 accounts {open}");
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
    // a reset meant for an earlier press leaves "Copied"; the last press's clears it
    p.act(&changed);
    assert!(!p.copied_expired(1));
    assert!(p.copied);
    assert!(p.copied_expired(2));
    assert!(!p.copied);
    assert_eq!(
        taken(),
        [
            Request::OpenSetup,
            Request::OpenPercentSettings,
            Request::Copy(changed.details().unwrap()),
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
    p.request(Request::OpenSession(Some(target)));
    assert_eq!(
        taken(),
        [Request::OpenSession(Some(OpenTarget::t3_code("t1")))]
    );
}

#[test]
fn the_row_under_the_pointer_lifts_and_says_where_it_opens() {
    let mut p = open(Feed::DemoPeak);
    p.opens = std::rc::Rc::new(|target: Option<&OpenTarget>| {
        assert_eq!(target, Some(&OpenTarget::t3_code("t1")));
        Some("Open in T3 Code ↗")
    });
    let target = p.model.borrow().snapshot.sources[0].target();
    p.hover("t1", target.as_ref(), true);
    assert_eq!(
        p.hovered,
        Some(burners::Lift {
            id: "t1".into(),
            open: Some("Open in T3 Code ↗"),
        })
    );
    // leaving another row changes nothing; leaving this one drops the lift
    p.hover("t2", None, false);
    assert!(p.hovered.is_some());
    p.hover("t1", target.as_ref(), false);
    assert_eq!(p.hovered, None);
    // closing the popover drops it too
    p.hover("t1", target.as_ref(), true);
    p.hide();
    assert_eq!(p.hovered, None);
}

/// Reduce Motion: the lifted row does not rise.
#[test]
fn no_lift_motion_with_reduce_motion() {
    assert_eq!(burners::lift_offset(false), -1.0);
    assert_eq!(burners::lift_offset(true), 0.0);
}

#[test]
fn hover_scene_tree() {
    let t = texts(&tree("popover-peak-hover"));
    assert_eq!(color_of(&t, "Open in T3 Code ↗"), INK);
    assert_eq!(color_of(&t, "1 agent + 3 sub"), "#f4f1ea8c");
    // the other rows rest: their account, no agent count
    assert!(!t.iter().any(|(s, _)| s == "1 agent"));
    let works = |t: &[(String, String)]| t.iter().filter(|(s, _)| s == "work").count();
    let rest = texts(&tree("popover-peak"));
    assert_eq!(
        works(&t) + 1,
        works(&rest),
        "only the lifted row hides its account"
    );
}

/// The pills count what is left.
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

/// Runs a scene through Rune's `--tree` and returns the tree.
fn tree(scene: &str) -> serde_json::Value {
    // its own folder per call: two tests may render the same scene at once
    static RUN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let run = RUN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("cooked-tree-{}-{run}-{scene}", std::process::id()));
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

/// The demo peak's "personal" fills before its reset: its 5-hour pill shows "full 01:50" alone, so the name reads
/// whole (with the reset time too it was cut to "perso…", 2026-11-25-popover-details).
#[test]
fn a_full_pill_leaves_the_name_whole() {
    let t = tree("popover-peak");
    let els = t["elements"].as_array().expect("elements");
    let name = els
        .iter()
        .find(|e| {
            e["text"] == "personal"
                && e["path"]
                    .as_str()
                    .is_some_and(|p| p.ends_with("five-hour/top/name"))
        })
        .expect("the personal 5-hour name");
    // "personal" in 13 pt Plex Sans 500 is 52 pt wide; "perso…" would be about 38
    assert!(name["rect"][2].as_f64().expect("width") > 50.0, "{name}");
    let texts = texts(&t);
    assert_eq!(color_of(&texts, "full 01:50"), RED);
    assert!(
        !texts.iter().any(|(s, _)| s == "02:40"),
        "no reset time after full"
    );
}

/// The lifted line rises 1 pt as a card inside a box that tracks the pointer and does not move: with the hit area on
/// the rising card, the pointer on its bottom point would leave it, drop it and catch it again.
#[test]
fn the_hover_box_is_not_the_lifted_card() {
    let t = tree("popover-peak-hover");
    let at = |end: &str| {
        t["elements"]
            .as_array()
            .expect("elements")
            .iter()
            .find(|e| e["path"].as_str().is_some_and(|p| p.ends_with(end)))
            .unwrap_or_else(|| panic!("no {end}"))
            .clone()
    };
    let (hit, card) = (at("/source-t1"), at("/source-t1/line"));
    assert_eq!(hit["role"], "button");
    assert_eq!(hit["fill"], "#00000000", "the hit box draws nothing");
    assert_eq!(card["fill"], "#1a1b1dff", "the card is its child");
    assert_eq!(hit["rect"], card["rect"]);
}

/// An ended line with nowhere to open it keeps "9 min ago" under the pointer, not a "↗" that does nothing.
#[test]
fn an_ended_line_with_nowhere_to_open_rests() {
    let mut ended = demo_data::peak().sources[0].clone();
    ended.ended_ago = Some(9);
    let nowhere = burners::Lift {
        id: ended.id.clone(),
        open: None,
    };
    assert_eq!(
        burners::right(&ended, Some(&nowhere)),
        burners::right(&ended, None)
    );
}
