//! Where Cooked keeps its own files (the minute cache, the limits store). Like the settings suite
//! (`mac::defaults::suite_name`), only the app bundle uses the real folder: a bare binary (`cargo run`, tests,
//! `target/perf/cooked`) gets its own, so it never rewrites the installed app's cache.

use std::path::{Path, PathBuf};

use crate::mac::defaults::{self, DOMAIN};

/// Overrides the folder.
pub const DATA_DIR_ENV: &str = "COOKED_DATA_DIR";
/// The installed app's folder, under `~/Library/Application Support`.
const APP_DIR: &str = "Cooked";
/// A bare binary's folder, next to the app's.
const DEV_DIR: &str = "Cooked.dev";

/// The folder: `COOKED_DATA_DIR` when set, `~/Library/Application Support/Cooked` inside the app bundle,
/// `.../Cooked.dev` otherwise. `None` without a home and no override.
pub fn data_dir_for(
    env: Option<&str>,
    bundle_id: Option<&str>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(dir) = env.filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let name = if bundle_id == Some(DOMAIN) {
        APP_DIR
    } else {
        DEV_DIR
    };
    Some(home?.join("Library/Application Support").join(name))
}

/// The folder for this process.
pub fn data_dir() -> Option<PathBuf> {
    let env = std::env::var(DATA_DIR_ENV).ok();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    data_dir_for(
        env.as_deref(),
        defaults::bundle_id().as_deref(),
        home.as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_app_bundle_uses_the_real_folder() {
        let home = Path::new("/Users/someone");
        let support = home.join("Library/Application Support");
        assert_eq!(
            data_dir_for(None, Some(DOMAIN), Some(home)),
            Some(support.join("Cooked"))
        );
        assert_eq!(
            data_dir_for(None, None, Some(home)),
            Some(support.join("Cooked.dev"))
        );
        assert_eq!(
            data_dir_for(None, Some("com.other.app"), Some(home)),
            Some(support.join("Cooked.dev"))
        );
        assert_eq!(
            data_dir_for(Some(""), None, Some(home)),
            Some(support.join("Cooked.dev"))
        );
        assert_eq!(data_dir_for(None, None, None), None);
    }

    #[test]
    fn the_env_override_wins() {
        let home = Path::new("/Users/someone");
        assert_eq!(
            data_dir_for(Some("/tmp/c"), Some(DOMAIN), Some(home)),
            Some(PathBuf::from("/tmp/c"))
        );
        assert_eq!(
            data_dir_for(Some("/tmp/c"), None, None),
            Some(PathBuf::from("/tmp/c"))
        );
    }

    #[test]
    fn a_test_binary_is_never_the_app() {
        if std::env::var_os(DATA_DIR_ENV).is_none() {
            let dir = data_dir().expect("HOME is set");
            assert!(dir.ends_with("Cooked.dev"), "{}", dir.display());
        }
    }
}
