//! `summaries` table accessors.
//!
//! Created by the export refactor in #2 (test coverage pass). The previous
//! inline SQL lived inside `commands::export_meeting` which made end-to-end
//! testing require a live `AppState`. Centralizing lets tests hit
//! `Db::open(...) -> summaries::get_latest(...)` directly.

use sqlx::SqlitePool;

use crate::error::Result;

/// Return the most recent summary markdown for a meeting, or `None` if the
/// meeting has never been summarized.
pub async fn get_latest(pool: &SqlitePool, meeting_id: i64) -> Result<Option<String>> {
    let row: Option<String> = sqlx::query_scalar(
        "SELECT summary_markdown FROM summaries \
         WHERE meeting_id = ? ORDER BY created_at DESC LIMIT 1",
    )
    .bind(meeting_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}
