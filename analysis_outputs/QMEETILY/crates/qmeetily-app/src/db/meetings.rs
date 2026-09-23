use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::Db;
use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct Meeting {
    pub id: i64,
    pub title: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub language_primary: Option<String>,
    pub audio_path: Option<String>,
    pub participants: Vec<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct NewMeeting {
    pub title: String,
    pub started_at: DateTime<Utc>,
    pub language_primary: Option<String>,
    pub audio_path: Option<String>,
    pub participants: Vec<String>,
}

impl Db {
    pub async fn create_meeting(&self, m: &NewMeeting) -> Result<i64> {
        let id = sqlx::query_scalar(
            "INSERT INTO meetings (title, started_at, language_primary, audio_path, participants_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?)
             RETURNING id",
        )
        .bind(&m.title)
        .bind(m.started_at.timestamp_millis())
        .bind(&m.language_primary)
        .bind(&m.audio_path)
        .bind(serde_json::to_string(&m.participants)?)
        .bind(Utc::now().timestamp_millis())
        .fetch_one(self.pool())
        .await?;
        Ok(id)
    }

    pub async fn end_meeting(&self, id: i64, ended_at: DateTime<Utc>) -> Result<()> {
        sqlx::query("UPDATE meetings SET ended_at = ? WHERE id = ?")
            .bind(ended_at.timestamp_millis())
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn list_meetings(&self, limit: u32, offset: u32) -> Result<Vec<Meeting>> {
        let rows: Vec<(i64, String, i64, Option<i64>, Option<String>, Option<String>, String)> =
            sqlx::query_as(
                "SELECT id, title, started_at, ended_at, language_primary, audio_path, participants_json
                 FROM meetings ORDER BY started_at DESC LIMIT ? OFFSET ?",
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(self.pool())
            .await?;

        Ok(rows.into_iter().map(map_meeting).collect())
    }

    pub async fn get_meeting(&self, id: i64) -> Result<Option<Meeting>> {
        let row: Option<(i64, String, i64, Option<i64>, Option<String>, Option<String>, String)> =
            sqlx::query_as(
                "SELECT id, title, started_at, ended_at, language_primary, audio_path, participants_json
                 FROM meetings WHERE id = ?",
            )
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        Ok(row.map(map_meeting))
    }
}

fn map_meeting(
    (id, title, started_at, ended_at, lang, audio, participants_json): (
        i64,
        String,
        i64,
        Option<i64>,
        Option<String>,
        Option<String>,
        String,
    ),
) -> Meeting {
    let participants: Vec<String> = serde_json::from_str(&participants_json).unwrap_or_default();
    Meeting {
        id,
        title,
        started_at: DateTime::from_timestamp_millis(started_at).unwrap_or_else(Utc::now),
        ended_at: ended_at.and_then(|t| DateTime::from_timestamp_millis(t)),
        language_primary: lang,
        audio_path: audio,
        participants,
    }
}
