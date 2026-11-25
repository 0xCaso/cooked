//! The Setup and Settings views: a 40 pt title band, then Setup's step rail, the step and its foot, or Settings' tabs,
//! the tab and the "Remove Cooked's changes…" row, and the confirmation sheet hanging from the title band.

use cooked_model::island_model::DisplayMode;
use cooked_model::live_snapshot;
use cooked_model::setup_model::{LastUse, SettingsFile, Step};
use cooked_model::status_line_setup::State;
use rune::{Color, Cx, El, Family, Shadow, Span, col, edit_access, el, icon, rich, row, text};
use std::time::UNIX_EPOCH;

use super::controls::{
    self, ButtonKind, Glyph, button, checkbox, disclosure, disclosure_body, glyph, line,
    name_field, rail_line, segment, segments, step_transition, switch,
};
use super::{Mode, Setup, TITLEBAR, Tab, WIDTH};
use crate::theme::{Palette, TextStyle, card, well};

/// A heading: Plex Sans 500 at `size` (h2 17/22, the sheet's h3 15/20).
fn heading(size: f32, line_height: f32) -> rune::TextStyle {
    rune::TextStyle {
        family: Family::Sans,
        line_height,
        ..rune::TextStyle::new(size, 500.0)
    }
}

fn label(key: impl Into<String>, t: impl Into<String>, style: TextStyle, color: Color) -> El {
    text(t.into()).key(key).font(style.font()).color(color)
}

/// A hairline on top of an element.
fn top_line(e: El) -> El {
    e.border_sides([0.5, 0.0, 0.0, 0.0], line())
}

fn bottom_line(e: El) -> El {
    e.border_sides([0.0, 0.0, 0.5, 0.0], line())
}

pub fn view(s: &Setup, cx: &mut Cx<Setup>) -> El {
    let reduce_motion = s.reduce_motion || cx.is_reduced_motion();
    let root = col()
        .key("root")
        .w(WIDTH)
        .fill(Palette::bg_deep())
        .child(title_band(s));
    match s.mode {
        Mode::Setup => root
            .child(rail(s))
            .child(step(s, reduce_motion))
            .child(foot(s)),
        Mode::Settings => {
            let root = root
                .child(tabs(s))
                .child(tab(s, reduce_motion))
                .child(remove_row());
            if s.confirming {
                root.child(sheet(s, reduce_motion))
            } else {
                root
            }
        }
    }
}

/// The traffic lights' band and the title in the middle.
fn title_band(s: &Setup) -> El {
    el().key("titlebar")
        .w_full()
        .h(TITLEBAR)
        .center()
        .child(label("title", s.title(), TextStyle::Body, Palette::ink2()))
}

// Setup

fn rail(s: &Setup) -> El {
    let steps = s.model.steps();
    let mut r = row()
        .key("rail")
        .w_full()
        .gap(10.0)
        .center_y()
        .pt(4.0)
        .px(24.0)
        .pb(14.0);
    for (i, step) in steps.iter().enumerate() {
        if i > 0 {
            r = r.child(rail_line().key(format!("line-{i}")));
        }
        r = r.child(rail_step(*step, i + 1, s.model.step));
    }
    bottom_line(r)
}

fn rail_step(step: Step, number: usize, current: Step) -> El {
    let (is_current, done) = (step == current, step < current);
    let mark = el().key("mark").size(20.0, 20.0).radius(10.0).center();
    let mark = if is_current {
        mark.fill(Palette::ink()).child(label(
            "n",
            number.to_string(),
            TextStyle::NumberEmphasis,
            controls::dark_ink(),
        ))
    } else if done {
        mark.border(1.0, Palette::ink().alpha(0.55))
            .child(icon(&controls::TICK, 12.0).color(Palette::ink()))
    } else {
        mark.border(1.0, Palette::ink().alpha(0.3)).child(label(
            "n",
            number.to_string(),
            TextStyle::NumberEmphasis,
            Palette::ink3(),
        ))
    };
    let (style, color) = if is_current {
        (TextStyle::Emphasis, Palette::ink())
    } else if done {
        (TextStyle::Body, Palette::ink2())
    } else {
        (TextStyle::Body, Palette::ink3())
    };
    row()
        .key(format!("step-{number}"))
        .gap(8.0)
        .center_y()
        .label(format!(
            "{}{}",
            step.title(),
            if is_current {
                ", current step"
            } else if done {
                ", done"
            } else {
                ""
            }
        ))
        .child(mark)
        .child(label("title", step.title(), style, color))
}

fn h2(t: &str) -> El {
    text(t.to_owned())
        .key("h2")
        .font(heading(17.0, 22.0))
        .color(Palette::ink())
        .role("heading")
}

fn lead(t: &str) -> El {
    text(t.to_owned())
        .key("lead")
        .font(TextStyle::Body.font())
        .color(Palette::ink2())
        .w_full()
        .wrap()
        .mb(10.0)
}

fn step(s: &Setup, reduce_motion: bool) -> El {
    let key = match s.model.step {
        Step::Accounts => "step-accounts",
        Step::Percent => "step-percent",
        Step::Login => "step-login",
    };
    let body = col()
        .key(key)
        .w_full()
        .gap(6.0)
        .pt(22.0)
        .px(24.0)
        .pb(20.0)
        .transition(step_transition(s.forward, reduce_motion));
    let body = match s.model.step {
        Step::Accounts => body
            .child(h2("Pick your accounts"))
            .child(lead("The name is what the menu bar shows."))
            .child(accounts_list(s)),
        Step::Percent => {
            let open = s.open.contains("what");
            let toggle = disclosure("what", "What changes", open, reduce_motion)
                .on_press(|s: &mut Setup| s.toggle_disclosure("what"));
            let inner = open.then(|| {
                disclosure_body("what-body", reduce_motion)
                    .gap(8.0)
                    .child(facts("facts", &what_changes(true)))
                    .child(trees(&s.model.settings_files()))
            });
            with_problem(
                s,
                body.child(h2("Add Cooked to your status line?"))
                    .child(lead(
                        "It gives the Claude percent in the terminal. T3 Code and Codex already show it.",
                    ))
                    .child(disclosure_group(toggle, inner)),
            )
        }
        Step::Login => body.child(
            row()
                .key("titlerow")
                .w_full()
                .gap(16.0)
                .center_y()
                .child(h2("Open Cooked at login?").grow(1.0))
                .child(
                    switch(
                        "login",
                        s.model.open_at_login,
                        "Open Cooked at login",
                        reduce_motion,
                    )
                    .on_press(|s: &mut Setup| s.set_open_at_login(!s.model.open_at_login)),
                ),
        ),
    };
    // the step's slot clips the 12 pt slide
    el().key("step").w_full().clip_corners([0.0; 4]).child(body)
}

fn foot(s: &Setup) -> El {
    let mut f = row()
        .key("foot")
        .w_full()
        .gap(8.0)
        .center_y()
        .pt(12.0)
        .px(16.0)
        .pb(14.0);
    if s.model.step != Step::Login {
        f = f.child(
            button("skip", "Skip", ButtonKind::Ghost).on_press_tx(|s: &mut Setup, tx| {
                if tx.by_keys() {
                    tx.instant();
                }
                s.go(true);
            }),
        );
    }
    f = f.child(el().key("spacer").grow(1.0));
    if s.model.back(s.model.step).is_some() {
        f = f.child(button("back", "Back", ButtonKind::Secondary).on_press_tx(
            |s: &mut Setup, tx| {
                if tx.by_keys() {
                    tx.instant();
                }
                s.back();
            },
        ));
    }
    f = f.child(
        button("primary", s.primary_title(), ButtonKind::Primary)
            .default_button()
            .on_press_tx(|s: &mut Setup, tx| {
                // Enter: instant, keyboard actions repeat
                if tx.by_keys() {
                    tx.instant();
                }
                if !s.go(false) {
                    tx.close_window();
                }
            }),
    );
    top_line(f)
}

// Accounts (Setup step 1 and the Accounts tab)

fn accounts_list(s: &Setup) -> El {
    let settings = s.mode == Mode::Settings;
    let now = s.now();
    let mut list = col().key("accounts").w_full().py(4.0).px(12.0);
    for (i, r) in s.model.rows.iter().enumerate() {
        let last = r
            .found
            .last_used
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs_f64());
        let Some(edit) = s.names.get(i) else {
            continue;
        };
        let field = name_field(edit, r.ticked, |input| {
            let input = input
                .on_edit(edit_access(move |s: &mut Setup| &mut s.names[i]))
                .on_input_change(|s: &mut Setup, _| s.name_edited());
            if settings {
                input.on_submit(|s: &mut Setup, _| s.commit_names())
            } else {
                input
            }
        });
        let mut line_row = row()
            .key(r.id().to_owned())
            .w_full()
            .gap(12.0)
            .min_h(40.0)
            .center_y()
            .child(
                checkbox("tick", r.ticked, &format!("Read {}", r.found.path))
                    .on_press(move |s: &mut Setup| s.toggle_row(i)),
            )
            .child(el().key("name").w(150.0).child(field))
            .child(
                label(
                    "path",
                    s.model.tilde(&r.found.path),
                    TextStyle::LabelNumber,
                    Palette::ink3(),
                )
                .grow(1.0)
                .shrink()
                .min_w(0.0)
                .truncate_middle(),
            )
            .child(el().key("last").w(84.0).justify_end().child(label(
                "text",
                LastUse::text(last, now, s.tz.as_ref()),
                TextStyle::LabelNumber,
                if r.ticked {
                    Palette::ink2()
                } else {
                    Palette::ink3()
                },
            )));
        if i > 0 {
            line_row = top_line(line_row);
        }
        list = list.child(line_row);
    }
    if s.model.rows.is_empty() {
        list = list.child(row().key("empty").min_h(40.0).center_y().child(label(
            "text",
            "No Claude Code or Codex folder found in your home folder.",
            TextStyle::Body,
            Palette::ink2(),
        )));
    }
    card(list, 16.0)
}

// Facts, files, changes

/// One fact: plain pieces at 72%, numbers and commands in Plex Mono (some at full ink).
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub text: String,
    pub mono: bool,
    pub ink: bool,
}

fn plain(t: impl Into<String>) -> Piece {
    Piece {
        text: t.into(),
        mono: false,
        ink: false,
    }
}

fn mono(t: impl Into<String>, ink: bool) -> Piece {
    Piece {
        text: t.into(),
        mono: true,
        ink,
    }
}

/// "What changes": the setup step adds what Skip means.
pub fn what_changes(skip: bool) -> Vec<Vec<Piece>> {
    let mut out = vec![
        vec![
            plain("Only the "),
            mono("statusLine", false),
            plain(" key changes. A backup is made once."),
        ],
        vec![plain(
            "Your status line looks the same. Disconnect puts back the exact old text.",
        )],
    ];
    if skip {
        out.push(vec![plain(
            "Skip: the terminal shows tokens per minute, no percent.",
        )]);
    }
    out
}

fn connected_facts(s: &Setup) -> Vec<Vec<Piece>> {
    let mut out = Vec::new();
    match s.model.last_terminal_percent() {
        Some((name, percent, at)) => out.push(vec![
            plain(format!("Last terminal percent: {name} ")),
            mono(format!("{percent}%"), false),
            plain(" at "),
            mono(live_snapshot::clock(at, s.tz.as_ref()), false),
            plain("."),
        ]),
        None => out.push(vec![plain(
            "No terminal percent yet: it comes after your next message in claude.",
        )]),
    }
    if let Some(command) = s.model.wrapped_command() {
        out.push(vec![
            plain("Your status line still runs "),
            mono(command, true),
            plain("."),
        ]);
    }
    out
}

fn fact(key: String, pieces: &[Piece]) -> El {
    let mut content = String::new();
    let mut spans = Vec::new();
    for p in pieces {
        let start = content.len();
        content.push_str(&p.text);
        if p.mono || p.ink {
            let mut span = Span::new(start, content.len());
            if p.mono {
                span = span.mono();
            }
            if p.ink {
                span = span.color(Palette::ink());
            }
            spans.push(span);
        }
    }
    rich(content, spans)
        .key(key)
        .font(TextStyle::Label.font())
        .color(Palette::ink2())
        .w_full()
}

fn facts(key: &'static str, list: &[Vec<Piece>]) -> El {
    let mut c = col().key(key).w_full().gap(4.0);
    for (i, pieces) in list.iter().enumerate() {
        c = c.child(fact(format!("fact-{i}"), pieces));
    }
    c
}

/// Which accounts read each settings file: "the file" or "link", so one Connect covering several is no surprise.
fn trees(files: &[SettingsFile]) -> El {
    let mut c = col().key("files").w_full().gap(8.0);
    for (n, file) in files.iter().enumerate() {
        let mut body = col().key(format!("file-{n}")).w_full().py(10.0).px(12.0);
        if file.readers.len() > 1 {
            body = body.child(
                row()
                    .key("head")
                    .w_full()
                    .justify_between()
                    .gap(12.0)
                    .mb(6.0)
                    .child(label(
                        "count",
                        format!("One file for {} accounts", file.readers.len()),
                        TextStyle::Label,
                        Palette::ink2(),
                    ))
                    .child(label(
                        "note",
                        "connecting once covers all of them",
                        TextStyle::Label,
                        Palette::ink3(),
                    )),
            );
        }
        for (i, reader) in file.readers.iter().enumerate() {
            let kind = if reader.is_link { "link" } else { "the file" };
            let mut r = row()
                .key(format!("reader-{i}"))
                .w_full()
                .gap(10.0)
                .min_h(22.0)
                .center_y()
                .child(
                    label(
                        "name",
                        reader.name.clone(),
                        TextStyle::Label,
                        Palette::ink(),
                    )
                    .w(64.0)
                    .truncate(),
                )
                .child(
                    label(
                        "path",
                        reader.path.clone(),
                        TextStyle::LabelNumber,
                        Palette::ink3(),
                    )
                    .grow(1.0)
                    .shrink()
                    .min_w(0.0)
                    .truncate_middle(),
                )
                .child(
                    row()
                        .key("kind")
                        .w(72.0)
                        .gap(5.0)
                        .center_y()
                        .justify_end()
                        .child(
                            icon(
                                if reader.is_link {
                                    &controls::LINK
                                } else {
                                    &controls::FILE
                                },
                                12.0,
                            )
                            .color(Palette::ink3()),
                        )
                        .child(label("text", kind, TextStyle::Label, Palette::ink3())),
                );
            if i > 0 {
                r = top_line(r);
            }
            body = body.child(r);
        }
        c = c.child(well(body, 8.0));
    }
    c
}

/// What the `statusLine` key ran and runs now, for the changed state.
fn change_well(s: &Setup) -> El {
    let cell = |key: &'static str, t: String, w: Option<f32>, style: TextStyle, color: Color| {
        let e = label(key, t, style, color).truncate();
        match w {
            Some(w) => e.w(w),
            None => e.grow(1.0).shrink().min_w(0.0),
        }
    };
    let was = match s.model.wrapped_command() {
        Some(c) => format!("Cooked bridge, which ran {c}"),
        None => "Cooked bridge".to_owned(),
    };
    let mut body = col().key("change").w_full().py(8.0).px(12.0).child(
        row()
            .key("was")
            .w_full()
            .gap(8.0)
            .min_h(22.0)
            .center_y()
            .child(cell(
                "key",
                "statusLine".into(),
                Some(72.0),
                TextStyle::LabelNumber,
                Palette::ink2(),
            ))
            .child(cell(
                "when",
                "was".into(),
                Some(40.0),
                TextStyle::LabelNumber,
                Palette::ink3(),
            ))
            .child(cell(
                "what",
                was,
                None,
                TextStyle::LabelNumber,
                Palette::ink2(),
            )),
    );
    if let Some(now) = s.model.current_command() {
        body = body.child(
            row()
                .key("now")
                .w_full()
                .gap(8.0)
                .min_h(22.0)
                .center_y()
                .child(el().key("key").w(72.0))
                .child(cell(
                    "when",
                    "now".into(),
                    Some(40.0),
                    TextStyle::LabelNumber,
                    Palette::ink3(),
                ))
                .child(cell(
                    "what",
                    now,
                    None,
                    TextStyle::LabelNumber,
                    Palette::ink(),
                )),
        );
    }
    well(body, 8.0)
}

/// The last error writing a settings file or the login item, under the step or tab.
fn with_problem(s: &Setup, body: El) -> El {
    match s.model.problem() {
        Some(p) => body.child(
            label("problem", p.to_owned(), TextStyle::Label, Palette::ink2())
                .w_full()
                .wrap(),
        ),
        None => body,
    }
}

// Settings

fn tabs(s: &Setup) -> El {
    let mut r = row()
        .key("tabs")
        .w_full()
        .gap(4.0)
        .justify_center()
        .pb(12.0);
    for t in Tab::ALL {
        let current = t == s.tab;
        let glyph = match t {
            Tab::Accounts => &controls::PEOPLE,
            Tab::Percent => &controls::PERCENT,
            Tab::General => &controls::POWER,
        };
        let color = if current {
            Palette::ink()
        } else {
            Palette::ink2()
        };
        let mut b = col()
            .key(t.key())
            .gap(4.0)
            .items_center()
            .py(6.0)
            .px(14.0)
            .radius(10.0)
            .hit()
            .cursor(rune::Cursor::Pointer)
            .role("tab")
            .checked(current)
            .label(t.title())
            .child(icon(glyph, 18.0).key("icon").color(color))
            .child(label("label", t.title(), TextStyle::Label, color))
            .on_press_tx(move |s: &mut Setup, tx| {
                if tx.by_keys() {
                    tx.instant();
                }
                s.select_tab(t);
            });
        if current {
            b = b
                .fill(rune::color::hex("#1c1c1c").unwrap_or(Color::BLACK))
                .border(0.5, Color::WHITE.alpha(0.12));
        }
        r = r.child(b);
    }
    bottom_line(r)
}

fn tab(s: &Setup, reduce_motion: bool) -> El {
    let key = match s.tab {
        Tab::Accounts => "content-accounts",
        Tab::Percent => "content-percent",
        Tab::General => "content-general",
    };
    let body = col()
        .key(key)
        .w_full()
        .gap(12.0)
        .pt(18.0)
        .px(24.0)
        .pb(20.0)
        .transition(step_transition(s.forward, reduce_motion));
    let body = match s.tab {
        Tab::Accounts => body.child(accounts_list(s)).child(
            label(
                "note",
                "Ticked accounts show in the menu bar. The name is what it shows.",
                TextStyle::Label,
                Palette::ink3(),
            )
            .w_full()
            .wrap(),
        ),
        Tab::Percent => percent(s, body, reduce_motion),
        Tab::General => general(s, body, reduce_motion),
    };
    let body = with_problem(s, body);
    el().key("tab").w_full().clip_corners([0.0; 4]).child(body)
}

/// A status line: the glyph, the title and one line, the action on the right.
fn status(kind: Glyph, title: &str, line: &str, action: Option<El>) -> El {
    let mut r = row()
        .key("status")
        .w_full()
        .gap(12.0)
        .center_y()
        .child(glyph(kind))
        .child(
            col()
                .key("text")
                .grow(1.0)
                .shrink()
                .min_w(0.0)
                .child(
                    label("title", title, TextStyle::Emphasis, Palette::ink())
                        .w_full()
                        .wrap(),
                )
                .child(
                    label("line", line, TextStyle::Body, Palette::ink2())
                        .w_full()
                        .wrap(),
                ),
        );
    if let Some(action) = action {
        r = r.child(action);
    }
    r
}

fn percent(s: &Setup, body: El, reduce_motion: bool) -> El {
    let files = s.model.settings_files();
    if files.is_empty() {
        return body.child(status(
            Glyph::Off,
            "No Claude account",
            "Tick a Claude account in Accounts first.",
            None,
        ));
    }
    let (key, title) = match s.model.percent() {
        State::NotConnected => ("what", "What changes"),
        _ => ("details", "Details"),
    };
    let open = s.open.contains(key);
    let head = match s.model.percent() {
        State::Connected => status(
            Glyph::Connected,
            "Connected",
            "Your status line looks the same.",
            Some(
                button("disconnect", "Disconnect", ButtonKind::Secondary)
                    .on_press(|s: &mut Setup| s.model.disconnect()),
            ),
        ),
        State::NotConnected => status(
            Glyph::Off,
            "Not connected",
            "The terminal shows tokens per minute only.",
            Some(
                button("connect", "Connect", ButtonKind::Primary)
                    .default_button()
                    .on_press(|s: &mut Setup| s.model.connect()),
            ),
        ),
        State::Changed => status(
            Glyph::Warn,
            "Your status line changed. Connect again?",
            "The terminal percent is paused.",
            Some(
                button("connect", "Connect again", ButtonKind::Primary)
                    .default_button()
                    .on_press(|s: &mut Setup| s.model.connect()),
            ),
        ),
    };
    let toggle = disclosure(key, title, open, reduce_motion);
    let toggle = if key == "what" {
        toggle.on_press(|s: &mut Setup| s.toggle_disclosure("what"))
    } else {
        toggle.on_press(|s: &mut Setup| s.toggle_disclosure("details"))
    };
    let body = body.child(head);
    let inner = open.then(|| {
        let inner = disclosure_body("disclosure-body", reduce_motion).gap(8.0);
        match s.model.percent() {
            State::Connected => inner
                .child(facts("facts", &connected_facts(s)))
                .child(trees(&files)),
            State::NotConnected => inner
                .child(facts("facts", &what_changes(false)))
                .child(trees(&files)),
            State::Changed => inner
                .child(facts(
                    "facts",
                    &[vec![plain(
                        "Something replaced Cooked's command. Cooked did not touch it. Connect again keeps the new command and adds Cooked around it.",
                    )]],
                ))
                .child(change_well(s)),
        }
    });
    body.child(disclosure_group(toggle, inner))
}

/// A disclosure's title and, when open, its body right under it.
fn disclosure_group(toggle: El, body: Option<El>) -> El {
    let group = col().key("disclosure").w_full();
    match body {
        Some(body) => group.child(toggle).child(body),
        None => group.child(toggle),
    }
}

fn general(s: &Setup, body: El, reduce_motion: bool) -> El {
    let login = card(
        row()
            .key("login")
            .w_full()
            .gap(16.0)
            .center_y()
            .padding(12.0)
            .child(
                label(
                    "title",
                    "Open Cooked at login",
                    TextStyle::Emphasis,
                    Palette::ink(),
                )
                .grow(1.0),
            )
            .child(
                switch(
                    "switch",
                    s.model.open_at_login,
                    "Open Cooked at login",
                    reduce_motion,
                )
                .on_press(|s: &mut Setup| s.set_open_at_login(!s.model.open_at_login)),
            ),
        16.0,
    );
    let mut body = body.child(login);
    if let Some(mode) = s.display_mode() {
        body = body.child(card(
            row()
                .key("show")
                .w_full()
                .gap(16.0)
                .center_y()
                .padding(12.0)
                .child(
                    col()
                        .key("text")
                        .grow(1.0)
                        .shrink()
                        .min_w(0.0)
                        .gap(2.0)
                        .child(label(
                            "title",
                            "Show Cooked in",
                            TextStyle::Emphasis,
                            Palette::ink(),
                        ))
                        .child(
                            label(
                                "line",
                                "The island under the menu bar, or a small item in the menu bar.",
                                TextStyle::Label,
                                Palette::ink3(),
                            )
                            .w_full()
                            .wrap(),
                        ),
                )
                .child(display_picker(mode)),
            16.0,
        ));
    }
    body
}

/// "Show Cooked in": the island or the menu bar.
fn display_picker(mode: DisplayMode) -> El {
    let mut track = segments("show-mode");
    for m in DisplayMode::ALL {
        track = track.child(
            segment(m.raw(), m.title(), m == mode)
                .on_press(move |s: &mut Setup| s.set_display_mode(m)),
        );
    }
    track
}

/// "Remove Cooked's changes…" under every tab.
fn remove_row() -> El {
    top_line(
        row()
            .key("remove-row")
            .w_full()
            .pt(10.0)
            .px(14.0)
            .pb(14.0)
            .child(
                button("remove", "Remove Cooked's changes…", ButtonKind::Ghost)
                    .h(28.0)
                    .on_press(|s: &mut Setup| s.confirming = true),
            ),
    )
}

/// The confirmation, hanging from the title band over a 45% shade; Esc or a click on the shade cancels.
fn sheet(s: &Setup, reduce_motion: bool) -> El {
    let panel = col()
        .key("sheet")
        .w(400.0)
        .padding(20.0)
        .corners([0.0, 0.0, 18.0, 18.0])
        .continuous_corners()
        .fill(Palette::bg_base())
        .border(0.5, Color::WHITE.alpha(0.16))
        .box_shadow(Shadow {
            y: 20.0,
            blur: 40.0,
            color: Color::BLACK.alpha(0.6),
            ..Shadow::default()
        })
        .hit()
        .modal(false, |s: &mut Setup| s.confirming = false)
        .transition(controls::sheet_transition(reduce_motion))
        .child(
            text("Remove Cooked's changes?")
                .key("h3")
                .font(heading(15.0, 20.0))
                .color(Palette::ink())
                .mb(10.0),
        )
        .child(
            text(s.model.remove_sentence())
                .key("sentence")
                .font(TextStyle::Body.font())
                .color(Palette::ink2())
                .w_full()
                .wrap()
                .mb(4.0),
        )
        .child(
            row()
                .key("acts")
                .w_full()
                .gap(8.0)
                .mt(12.0)
                .justify_end()
                .child(
                    button("cancel", "Cancel", ButtonKind::Secondary)
                        .on_press(|s: &mut Setup| s.confirming = false),
                )
                .child(
                    button("confirm", "Remove changes", ButtonKind::Primary)
                        .default_button()
                        .on_press(|s: &mut Setup| s.remove_all()),
                ),
        );
    col()
        .key("shade")
        .absolute(0.0, TITLEBAR)
        .w_full()
        .h_full()
        .fill(Color::BLACK.alpha(0.45))
        .hit()
        .on_press(|s: &mut Setup| s.confirming = false)
        .transition(controls::fade_transition(reduce_motion))
        .items_center()
        .child(panel)
}
