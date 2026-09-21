//! Database — SQLite + sqlx + FTS5 + sqlite-vec.
//!
//! Borrowed patterns from meetily, simplified: one .db file, one schema.
//! Borrowed from meetily: `transcripts_fts` virtual table for search,
//! schema for meetings / transcripts / decisions.

pub mod schema;
pub mod meetings;
pub mod transcripts;
pub mod decisions;

use rusqlite::{Connection, OpenFlags};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

use crate::error::Result;

pub struct Db {
    pool: SqlitePool,
    raw: std::sync::Mutex<Connection>,
}

impl Db {
    pub async fn open(path: &Path) -> Result<Self> {
        let url = format!("sqlite://{}?mode=rwc", path.display());
        let opts = SqliteConnectOptions::from_str(&url)?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .min_connections(1)
            .connect_with(opts)
            .await?;

        sqlx::query(schema::SCHEMA_SQL).execute(&pool).await?;
        let raw = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        raw.execute_batch(schema::FTS_SQL)?;

        Ok(Self {
            pool,
            raw: std::sync::Mutex::new(raw),
        })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn raw(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.raw.lock().expect("raw connection mutex poisoned")
    }
}
