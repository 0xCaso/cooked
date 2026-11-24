//! File helpers the log readers share: the walk, reads at an offset, byte search and atomic writes.

use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Every file under `root`, like `FileManager`'s enumerator with `.skipsHiddenFiles`: hidden files and folders are
/// left out, symlinked folders are not followed.
pub(crate) fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.file_name().as_encoded_bytes().first() == Some(&b'.') {
                continue;
            }
            match entry.file_type() {
                Ok(t) if t.is_dir() => stack.push(entry.path()),
                Ok(_) => out.push(entry.path()),
                Err(_) => {}
            }
        }
    }
    out
}

pub(crate) fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

pub(crate) fn len(path: &Path) -> Option<u64> {
    Some(fs::metadata(path).ok()?.len())
}

/// Up to `max` bytes from `offset`.
pub(crate) fn read_at(path: &Path, offset: u64, max: u64) -> Option<Vec<u8>> {
    let mut f = fs::File::open(path).ok()?;
    f.seek(SeekFrom::Start(offset)).ok()?;
    let mut data = Vec::new();
    f.take(max).read_to_end(&mut data).ok()?;
    Some(data)
}

/// The first `needle` at or after `from`.
pub(crate) fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(from);
    }
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

pub(crate) fn contains(hay: &[u8], needle: &[u8]) -> bool {
    find(hay, needle, 0).is_some()
}

/// Lines split on `\n`, empty ones left out (Swift's `split(separator:)`).
pub(crate) fn lines(data: &[u8]) -> impl DoubleEndedIterator<Item = &[u8]> {
    data.split(|b| *b == b'\n').filter(|l| !l.is_empty())
}

/// `$HOME`.
pub(crate) fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// `~/Library/Application Support/Cooked/<name>`.
pub(crate) fn support(name: &str) -> Option<PathBuf> {
    Some(
        home()?
            .join("Library/Application Support/Cooked")
            .join(name),
    )
}

/// Writes through a temporary file and a rename, creating the folder: `Data.write(options: .atomic)`.
pub(crate) fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".tmp-{}", std::process::id()));
    let tmp = PathBuf::from(tmp);
    let mut f = fs::File::create(&tmp)?;
    f.write_all(data)?;
    fs::rename(&tmp, path)
}
