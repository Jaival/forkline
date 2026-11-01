//! SQLite storage: connection pool, PRAGMAs, migrations and read queries.
//!
//! The UI only reads through `queries::*`. Writes come from sync, the action
//! queue and the fake generator.

pub mod migrate;
pub mod queries;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;

pub use rusqlite;

const READERS: usize = 4;

/// `%LOCALAPPDATA%\Forkline`.
pub fn default_data_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map_or_else(|| PathBuf::from("."), PathBuf::from);
    base.join("Forkline")
}

pub fn db_path(data_dir: &Path) -> PathBuf {
    data_dir.join("forkline.db")
}

/// Open one connection with the standard PRAGMAs.
pub fn open_conn(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA mmap_size = 268435456;
         PRAGMA temp_store = MEMORY;
         PRAGMA busy_timeout = 5000;",
    )?;
    conn.set_prepared_statement_cache_capacity(128);
    Ok(conn)
}

/// One writer connection and a few reader connections (WAL allows parallel reads).
pub struct Db {
    writer: Mutex<Connection>,
    readers: Vec<Mutex<Connection>>,
    next: AtomicUsize,
    path: PathBuf,
}

impl Db {
    /// Open (and create / migrate) the database file at `path`.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let mut writer = open_conn(path)?;
        migrate::run(&mut writer)?;
        let readers = (0..READERS)
            .map(|_| open_conn(path).map(Mutex::new))
            .collect::<rusqlite::Result<_>>()?;
        Ok(Self {
            writer: Mutex::new(writer),
            readers,
            next: AtomicUsize::new(0),
            path: path.to_owned(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Run `f` on a free reader connection.
    pub fn read<T>(
        &self,
        f: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> rusqlite::Result<T> {
        let start = self.next.fetch_add(1, Ordering::Relaxed);
        let guard = (0..READERS)
            .find_map(|i| self.readers[(start + i) % READERS].try_lock().ok())
            .unwrap_or_else(|| lock(&self.readers[start % READERS]));
        f(&guard)
    }

    /// Run `f` on the single writer connection.
    pub fn write<T>(
        &self,
        f: impl FnOnce(&mut Connection) -> rusqlite::Result<T>,
    ) -> rusqlite::Result<T> {
        f(&mut lock(&self.writer))
    }
}

/// A poisoned lock only means another thread panicked mid-query; the
/// connection itself is still usable.
fn lock(m: &Mutex<Connection>) -> MutexGuard<'_, Connection> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
