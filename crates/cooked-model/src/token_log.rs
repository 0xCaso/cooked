//! Port of `Model/TokenLog.swift`. T3 Code's own database, read-only: the T3 Code extra. It names the sessions T3 Code
//! started by their thread, and gives the account of a thread for T3 Code's limit logs. No number of the island comes
//! from it.
//!
//! The Swift app links the system SQLite; so does this port, through a few `extern "C"` calls into macOS's
//! `libsqlite3`, instead of a crate that would compile its own copy.

use crate::files;
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_int};
use std::fmt;
use std::path::PathBuf;

/// A T3 Code thread and its project, as the app names them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thread {
    pub id: String,
    pub title: String,
    /// The project's title; `None` for "No project" (T3's scratch folder).
    pub project: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Open(String),
    Query(String),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Open(path) => write!(f, "cannot open {path}"),
            Failure::Query(message) => write!(f, "query failed: {message}"),
        }
    }
}

impl std::error::Error for Failure {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenLog {
    pub path: PathBuf,
}

const THREAD_COLUMNS: &str = "t.thread_id, t.title, NULLIF(p.title, 'No project')";

fn thread_join(on: &str) -> String {
    format!(
        "LEFT JOIN orchestration_v2_projection_threads t ON t.thread_id = {on}\n\
         LEFT JOIN projection_projects p ON p.project_id = t.project_id"
    )
}

impl TokenLog {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// `~/.t3/userdata/statev2.sqlite`.
    pub fn default_path() -> Option<PathBuf> {
        Some(files::home()?.join(".t3/userdata/statev2.sqlite"))
    }

    /// The T3 Code threads of these Claude sessions (`nativeThreadRef.nativeId`), by session id.
    pub fn threads(&self, ids: &[String]) -> Result<HashMap<String, Thread>, Failure> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let db = Db::open(&self.path)?;
        let list = ids
            .iter()
            .map(|id| format!("'{}'", id.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT json_extract(pth.payload_json,'$.nativeThreadRef.nativeId') AS native, {THREAD_COLUMNS}\n\
             FROM orchestration_v2_projection_provider_threads pth {}\n\
             WHERE native IN ({list}) AND t.thread_id IS NOT NULL",
            thread_join("pth.thread_id")
        );
        let mut out = HashMap::new();
        db.query(&sql, |row| {
            if let (Some(native), Some(id), Some(title)) = (row(0), row(1), row(2)) {
                out.insert(
                    native,
                    Thread {
                        id,
                        title,
                        project: row(3),
                    },
                );
            }
        })?;
        Ok(out)
    }

    /// T3 Code thread id → its account (`providerInstanceId`), for rate-limit events that do not name it.
    pub fn thread_accounts(&self) -> Result<HashMap<String, String>, Failure> {
        let db = Db::open(&self.path)?;
        let mut out = HashMap::new();
        db.query(
            "SELECT thread_id, json_extract(payload_json,'$.providerInstanceId') FROM orchestration_v2_projection_threads",
            |row| {
                if let (Some(id), Some(account)) = (row(0), row(1)) {
                    out.insert(id, account);
                }
            },
        )?;
        Ok(out)
    }
}

// The few SQLite calls the reader needs, from the system library.
#[repr(C)]
struct Sqlite3 {
    _private: [u8; 0],
}
#[repr(C)]
struct Stmt {
    _private: [u8; 0],
}

const SQLITE_OK: c_int = 0;
const SQLITE_ROW: c_int = 100;
const SQLITE_OPEN_READONLY: c_int = 0x0000_0001;
const SQLITE_OPEN_URI: c_int = 0x0000_0040;

#[link(name = "sqlite3")]
unsafe extern "C" {
    fn sqlite3_open_v2(
        filename: *const c_char,
        db: *mut *mut Sqlite3,
        flags: c_int,
        vfs: *const c_char,
    ) -> c_int;
    fn sqlite3_close(db: *mut Sqlite3) -> c_int;
    fn sqlite3_busy_timeout(db: *mut Sqlite3, ms: c_int) -> c_int;
    fn sqlite3_errmsg(db: *mut Sqlite3) -> *const c_char;
    fn sqlite3_prepare_v2(
        db: *mut Sqlite3,
        sql: *const c_char,
        n: c_int,
        stmt: *mut *mut Stmt,
        tail: *mut *const c_char,
    ) -> c_int;
    fn sqlite3_step(stmt: *mut Stmt) -> c_int;
    fn sqlite3_column_text(stmt: *mut Stmt, i: c_int) -> *const u8;
    fn sqlite3_finalize(stmt: *mut Stmt) -> c_int;
}

/// An open read-only connection, closed on drop.
struct Db(*mut Sqlite3);

impl Db {
    fn open(path: &std::path::Path) -> Result<Db, Failure> {
        let name = path.to_string_lossy().into_owned();
        let uri = CString::new(format!("file:{name}?mode=ro"))
            .map_err(|_| Failure::Open(name.clone()))?;
        let mut db: *mut Sqlite3 = std::ptr::null_mut();
        // SAFETY: `uri` is a valid C string for the call; `db` receives a handle we own and close on drop.
        let rc = unsafe {
            sqlite3_open_v2(
                uri.as_ptr(),
                &mut db,
                SQLITE_OPEN_READONLY | SQLITE_OPEN_URI,
                std::ptr::null(),
            )
        };
        let db = Db(db);
        if rc != SQLITE_OK {
            return Err(Failure::Open(name));
        }
        // SAFETY: `db.0` is an open handle.
        unsafe { sqlite3_busy_timeout(db.0, 2000) };
        Ok(db)
    }

    /// Runs `sql` and calls `row` for each result row with a reader of its text columns.
    fn query(
        &self,
        sql: &str,
        mut row: impl FnMut(&dyn Fn(c_int) -> Option<String>),
    ) -> Result<(), Failure> {
        let sql =
            CString::new(sql).map_err(|_| Failure::Query("SQL holds a NUL byte".to_owned()))?;
        let mut stmt: *mut Stmt = std::ptr::null_mut();
        // SAFETY: `self.0` is open, `sql` lives for the call, `stmt` receives a statement finalized below.
        let rc = unsafe {
            sqlite3_prepare_v2(self.0, sql.as_ptr(), -1, &mut stmt, std::ptr::null_mut())
        };
        if rc != SQLITE_OK || stmt.is_null() {
            // SAFETY: `sqlite3_errmsg` returns a C string owned by the connection, valid until the next call.
            let message = unsafe { CStr::from_ptr(sqlite3_errmsg(self.0)) }
                .to_string_lossy()
                .into_owned();
            return Err(Failure::Query(message));
        }
        let text = |i: c_int| -> Option<String> {
            // SAFETY: `stmt` is a live statement on a row; the text pointer is valid until the next step.
            let p = unsafe { sqlite3_column_text(stmt, i) };
            if p.is_null() {
                return None;
            }
            // SAFETY: SQLite's text is NUL-terminated UTF-8.
            Some(
                unsafe { CStr::from_ptr(p.cast::<c_char>()) }
                    .to_string_lossy()
                    .into_owned(),
            )
        };
        // SAFETY: `stmt` is live until finalized after the loop.
        while unsafe { sqlite3_step(stmt) } == SQLITE_ROW {
            row(&text);
        }
        // SAFETY: finalizes the statement prepared above, once.
        unsafe { sqlite3_finalize(stmt) };
        Ok(())
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        // SAFETY: closes the handle opened in `open` (a null or failed handle is fine for `sqlite3_close`).
        unsafe { sqlite3_close(self.0) };
    }
}
