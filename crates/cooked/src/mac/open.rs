//! A click on a row (release spec, section 1, Click on a row; `SessionOpener.swift` and `T3Code.swift`): the place
//! the session runs in, else a Terminal that resumes it.
//!
//! `plan` decides what to do from what is installed and running (`Apps`, faked in tests); `perform` does it with
//! `NSWorkspace`. Tests never open anything: they check the plan.

use std::path::{Path, PathBuf};
use std::time::Duration;

use cooked_model::t3_code;
use cooked_model::token_sources::{OpenTarget, Place};
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_app_kit::{NSRunningApplication, NSWorkspace, NSWorkspaceOpenConfiguration};
use objc2_foundation::{NSArray, NSString, NSURL};

pub const CLAUDE_BUNDLE_ID: &str = "com.anthropic.claudefordesktop";
pub const CODEX_BUNDLE_ID: &str = "com.openai.codex";
pub const TERMINAL_BUNDLE_ID: &str = "com.apple.Terminal";

/// A link sent while the app is still loading fails ("Couldn't open that session", seen on 2026-11-04): a closed
/// app opens first and gets the link this much later.
pub const LAUNCH_WAIT: Duration = Duration::from_secs(5);

/// What macOS says about the apps.
pub trait Apps {
    /// The installed app's bundle path (`URLForApplicationWithBundleIdentifier`).
    fn installed(&self, bundle_id: &str) -> Option<PathBuf>;
    fn is_running(&self, bundle_id: &str) -> bool;
    /// The bundle path of the first running instance.
    fn running_path(&self, bundle_id: &str) -> Option<PathBuf>;
}

/// Where Cooked keeps its files and finds T3 Code's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Places {
    /// The `.command` files a Terminal fallback opens.
    pub commands_dir: PathBuf,
    /// T3 Code's environment id (`t3_code::environment_file`).
    pub t3_environment: PathBuf,
}

impl Places {
    /// Cooked's folder (`paths::data_dir`) and the real home.
    pub fn app() -> Option<Self> {
        let home = std::env::var("HOME").ok()?;
        Some(Self {
            commands_dir: crate::paths::data_dir()?.join("open"),
            t3_environment: t3_code::environment_file(&home),
        })
    }
}

/// What a click does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Sends `url` to the app at `app`; `launch_first`: it is not running, so it opens first and gets the link
    /// `LAUNCH_WAIT` later.
    Link {
        url: String,
        app: PathBuf,
        launch_first: bool,
    },
    /// Brings the running app to the front.
    Activate { bundle_id: String },
    /// Opens the app.
    Launch { app: PathBuf },
    /// Writes `script` to `path` (mode 755) and opens it in Terminal: a `.command` file needs no Automation
    /// permission.
    Terminal {
        path: PathBuf,
        script: String,
        terminal: PathBuf,
    },
}

/// Where a click on `target` goes; `None` when there is nowhere (no T3 Code, no Terminal).
pub fn plan(target: Option<&OpenTarget>, apps: &dyn Apps, places: &Places) -> Option<Action> {
    let Some(target) = target else {
        return t3_plan(None, apps, places);
    };
    let link = match target.place {
        Place::T3Code => return t3_plan(Some(&target.id), apps, places),
        Place::Codex => Some((CODEX_BUNDLE_ID, format!("codex://threads/{}", target.id))),
        // The Claude app only finds `~/.claude` sessions; other accounts fail there without telling Cooked.
        Place::Claude => target.config_dir.is_none().then(|| {
            (
                CLAUDE_BUNDLE_ID,
                format!("claude://resume?session={}", target.id),
            )
        }),
    };
    if let Some((bundle_id, url)) = link
        && let Some(app) = apps.installed(bundle_id)
    {
        return Some(Action::Link {
            url,
            app,
            launch_first: !apps.is_running(bundle_id),
        });
    }
    terminal_plan(target, apps, &places.commands_dir)
}

/// T3 Code: the thread in the T3 Code that is running (Alpha, Nightly and custom builds share the bundle id and
/// the scheme, so the link goes to that app, not to whichever one macOS picks). No thread or no link: only to
/// the front.
fn t3_plan(thread: Option<&str>, apps: &dyn Apps, places: &Places) -> Option<Action> {
    let app = apps
        .running_path(t3_code::BUNDLE_ID)
        .or_else(|| apps.installed(t3_code::BUNDLE_ID))?;
    if let Some(url) = thread.and_then(|t| t3_code::link(t, &places.t3_environment)) {
        return Some(Action::Link {
            url,
            app,
            launch_first: false,
        });
    }
    Some(if apps.is_running(t3_code::BUNDLE_ID) {
        Action::Activate {
            bundle_id: t3_code::BUNDLE_ID.to_owned(),
        }
    } else {
        Action::Launch { app }
    })
}

/// A new Terminal window resumes the session in its folder.
fn terminal_plan(target: &OpenTarget, apps: &dyn Apps, dir: &Path) -> Option<Action> {
    let terminal = apps.installed(TERMINAL_BUNDLE_ID)?;
    let name: String = target
        .id
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect();
    Some(Action::Terminal {
        path: dir.join(format!("{name}.command")),
        script: target.script(),
        terminal,
    })
}

/// The real apps, from `NSWorkspace` and `NSRunningApplication`.
pub struct Workspace;

impl Apps for Workspace {
    fn installed(&self, bundle_id: &str) -> Option<PathBuf> {
        let url = NSWorkspace::sharedWorkspace()
            .URLForApplicationWithBundleIdentifier(&NSString::from_str(bundle_id))?;
        Some(PathBuf::from(url.path()?.to_string()))
    }

    fn is_running(&self, bundle_id: &str) -> bool {
        first_running(bundle_id).is_some()
    }

    fn running_path(&self, bundle_id: &str) -> Option<PathBuf> {
        let url = first_running(bundle_id)?.bundleURL()?;
        Some(PathBuf::from(url.path()?.to_string()))
    }
}

fn first_running(bundle_id: &str) -> Option<Retained<NSRunningApplication>> {
    NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(bundle_id))
        .firstObject()
}

/// Opens the session (Swift `SessionOpener.open`).
pub fn open(target: Option<&OpenTarget>) {
    let Some(places) = Places::app() else { return };
    let Some(action) = plan(target, &Workspace, &places) else {
        return;
    };
    perform(action);
}

/// Does what `plan` decided.
fn perform(action: Action) {
    match action {
        Action::Link {
            url,
            app,
            launch_first: false,
        } => open_link(&url, &app),
        Action::Link {
            url,
            app,
            launch_first: true,
        } => {
            launch(&app);
            let _ = std::thread::Builder::new()
                .name("cooked-open".into())
                .spawn(move || {
                    std::thread::sleep(LAUNCH_WAIT);
                    open_link(&url, &app);
                });
        }
        Action::Activate { bundle_id } => {
            if let Some(app) = first_running(&bundle_id) {
                // SAFETY: `-[NSRunningApplication activate]` (macOS 14), Swift's `activate()`.
                let _: Bool = unsafe { msg_send![&*app, activate] };
            }
        }
        Action::Launch { app } => launch(&app),
        Action::Terminal {
            path,
            script,
            terminal,
        } => {
            if write_script(&path, &script).is_ok() {
                open_with(file_url(&path), &terminal);
            }
        }
    }
}

fn write_script(path: &Path, script: &str) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // like `write(toFile:atomically:)`: a half-written script never runs
    let tmp = path.with_extension("command.tmp");
    std::fs::write(&tmp, script)?;
    std::fs::rename(&tmp, path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

fn file_url(path: &Path) -> Retained<NSURL> {
    NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()))
}

fn open_link(url: &str, app: &Path) {
    if let Some(url) = NSURL::URLWithString(&NSString::from_str(url)) {
        open_with(url, app);
    }
}

/// `NSWorkspace.open([url], withApplicationAt: app, configuration: .init())`.
fn open_with(url: Retained<NSURL>, app: &Path) {
    let urls = NSArray::from_retained_slice(&[url]);
    NSWorkspace::sharedWorkspace().openURLs_withApplicationAtURL_configuration_completionHandler(
        &urls,
        &file_url(app),
        &NSWorkspaceOpenConfiguration::configuration(),
        None,
    );
}

/// `NSWorkspace.openApplication(at: app, configuration: .init())`.
fn launch(app: &Path) {
    NSWorkspace::sharedWorkspace().openApplicationAtURL_configuration_completionHandler(
        &file_url(app),
        &NSWorkspaceOpenConfiguration::configuration(),
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Installed and running apps by bundle id.
    #[derive(Default)]
    struct FakeApps {
        installed: HashMap<&'static str, &'static str>,
        running: HashMap<&'static str, &'static str>,
    }

    impl FakeApps {
        fn with(
            installed: &[(&'static str, &'static str)],
            running: &[(&'static str, &'static str)],
        ) -> Self {
            Self {
                installed: installed.iter().copied().collect(),
                running: running.iter().copied().collect(),
            }
        }
    }

    impl Apps for FakeApps {
        fn installed(&self, bundle_id: &str) -> Option<PathBuf> {
            self.installed.get(bundle_id).map(PathBuf::from)
        }

        fn is_running(&self, bundle_id: &str) -> bool {
            self.running.contains_key(bundle_id)
        }

        fn running_path(&self, bundle_id: &str) -> Option<PathBuf> {
            self.running.get(bundle_id).map(PathBuf::from)
        }
    }

    const T3: (&str, &str) = (t3_code::BUNDLE_ID, "/Applications/T3 Code.app");
    const T3_ALPHA: (&str, &str) = (t3_code::BUNDLE_ID, "/Applications/T3 Code (Alpha).app");
    const CODEX: (&str, &str) = (CODEX_BUNDLE_ID, "/Applications/Codex.app");
    const CLAUDE: (&str, &str) = (CLAUDE_BUNDLE_ID, "/Applications/Claude.app");
    const TERMINAL: (&str, &str) = (
        TERMINAL_BUNDLE_ID,
        "/System/Applications/Utilities/Terminal.app",
    );

    /// T3 Code's environment id in a file of its own (tests run in parallel), or none.
    fn places(env: Option<&str>) -> Places {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!("cooked-open-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let t3_environment = dir.join(format!(
            "environment-id-{}",
            N.fetch_add(1, Ordering::Relaxed)
        ));
        if let Some(env) = env {
            std::fs::write(&t3_environment, env).unwrap();
        }
        Places {
            commands_dir: PathBuf::from("/c/open"),
            t3_environment,
        }
    }

    fn link(url: &str, app: &str, launch_first: bool) -> Option<Action> {
        Some(Action::Link {
            url: url.into(),
            app: app.into(),
            launch_first,
        })
    }

    #[test]
    fn a_t3_thread_goes_to_the_running_t3_code() {
        let target = OpenTarget::t3_code("thread:a%3A1");
        // the running build wins over the one macOS would pick
        let apps = FakeApps::with(&[T3], &[T3_ALPHA]);
        assert_eq!(
            plan(Some(&target), &apps, &places(Some("env-1\n"))),
            link("t3code://threads/env-1/thread%3Aa%253A1", T3_ALPHA.1, false)
        );
        // not running: the installed one, link at once (T3 Code takes it while it starts)
        let apps = FakeApps::with(&[T3], &[]);
        assert_eq!(
            plan(Some(&target), &apps, &places(Some("env-1"))),
            link("t3code://threads/env-1/thread%3Aa%253A1", T3.1, false)
        );
    }

    #[test]
    fn t3_code_without_a_link_only_comes_to_the_front() {
        let target = OpenTarget::t3_code("t1");
        let running = FakeApps::with(&[T3], &[T3]);
        let closed = FakeApps::with(&[T3], &[]);
        assert_eq!(
            plan(Some(&target), &running, &places(None)),
            Some(Action::Activate {
                bundle_id: t3_code::BUNDLE_ID.into()
            })
        );
        assert_eq!(
            plan(None, &closed, &places(Some("env"))),
            Some(Action::Launch { app: T3.1.into() })
        );
        assert_eq!(plan(None, &FakeApps::default(), &places(None)), None);
    }

    #[test]
    fn codex_and_claude_open_in_their_app_after_it_starts() {
        let cx = OpenTarget::codex("cx-1", Some("/r".into()));
        let home = OpenTarget::claude("s-1", Some("/r".into()), None);
        let apps = FakeApps::with(&[CODEX, CLAUDE, TERMINAL], &[CODEX]);
        let p = places(None);
        assert_eq!(
            plan(Some(&cx), &apps, &p),
            link("codex://threads/cx-1", CODEX.1, false)
        );
        // Claude is closed: it opens first and gets the link LAUNCH_WAIT later
        assert_eq!(
            plan(Some(&home), &apps, &p),
            link("claude://resume?session=s-1", CLAUDE.1, true)
        );
    }

    #[test]
    fn other_claude_accounts_and_missing_apps_resume_in_terminal() {
        let work = OpenTarget::claude("w/1:x", Some("/r".into()), Some("/h/.claude-work".into()));
        let apps = FakeApps::with(&[CLAUDE, TERMINAL], &[CLAUDE]);
        let p = places(None);
        assert_eq!(
            plan(Some(&work), &apps, &p),
            Some(Action::Terminal {
                path: "/c/open/w1x.command".into(),
                script: work.script(),
                terminal: TERMINAL.1.into(),
            })
        );
        let cx = OpenTarget::codex("cx", None);
        assert!(matches!(
            plan(Some(&cx), &apps, &p),
            Some(Action::Terminal { .. })
        ));
        // no Terminal: nothing
        assert_eq!(plan(Some(&cx), &FakeApps::with(&[CLAUDE], &[]), &p), None);
    }

    #[test]
    fn the_script_is_written_executable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("cooked-open-script-{}", std::process::id()));
        let path = dir.join("open/s.command");
        write_script(&path, "#!/bin/sh\necho hi\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "#!/bin/sh\necho hi\n"
        );
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
