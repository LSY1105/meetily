//! Database — SQLite + sqlx.
//!
//! Borrowed patterns from meetily, simplified: one .db file, one schema,
//! schema for meetings / transcripts / decisions.
//!
//! Full-text search in v0.1 is implemented with simple `LIKE` against
//! `text` / `rewritten_text` in `db/transcripts`. A future revision can
//! re-introduce FTS5 with proper INSERT/UPDATE/DELETE triggers.

pub mod schema;
pub mod meetings;
pub mod transcripts;
pub mod decisions;
pub mod summaries;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

use crate::error::Result;

pub struct Db {
    pool: SqlitePool,
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

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}
