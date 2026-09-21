use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::Db;
use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub id: i64,
    pub meeting_id: i64,
    pub sequence_id: i64,
    pub start_ms: i32,
    pub end_ms: i32,
    pub text: String,
    pub rewritten_text: Option<String>,
    pub language: Option<String>,
    pub speaker_label: Option<String>,
    pub confidence: Option<f32>,
    pub is_partial: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewTranscript {
    pub meeting_id: i64,
    pub sequence_id: i64,
    pub start_ms: i32,
    pub end_ms: i32,
    pub text: String,
    pub rewritten_text: Option<String>,
    pub language: Option<String>,
    pub speaker_label: Option<String>,
    pub confidence: Option<f32>,
    pub is_partial: bool,
}

impl Db {
    pub async fn insert_transcript(&self, t: &NewTranscript) -> Result<i64> {
        let id = sqlx::query_scalar(
            "INSERT INTO transcripts (meeting_id, sequence_id, start_ms, end_ms, text,
                                      rewritten_text, language, speaker_label,
                                      confidence, is_partial, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(meeting_id, sequence_id) DO UPDATE SET
                text = excluded.text,
                rewritten_text = excluded.rewritten_text,
                end_ms = excluded.end_ms,
                is_partial = excluded.is_partial
             RETURNING id",
        )
        .bind(t.meeting_id)
        .bind(t.sequence_id)
        .bind(t.start_ms)
        .bind(t.end_ms)
        .bind(&t.text)
        .bind(&t.rewritten_text)
        .bind(&t.language)
        .bind(&t.speaker_label)
        .bind(t.confidence)
        .bind(t.is_partial as i32)
        .bind(Utc::now().timestamp_millis())
        .fetch_one(self.pool())
        .await?;
        Ok(id)
    }

    pub async fn get_meeting_transcripts(&self, meeting_id: i64) -> Result<Vec<Transcript>> {
        let rows: Vec<(
            i64, i64, i64, i32, i32, String, Option<String>,
            Option<String>, Option<String>, Option<f32>, i32, i64,
        )> = sqlx::query_as(
            "SELECT id, meeting_id, sequence_id, start_ms, end_ms, text,
                    rewritten_text, language, speaker_label, confidence, is_partial, created_at
             FROM transcripts WHERE meeting_id = ? ORDER BY sequence_id",
        )
        .bind(meeting_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(map_transcript).collect())
    }

    pub async fn search(&self, query: &str, limit: u32) -> Result<Vec<TranscriptSearchHit>> {
        let pattern = format!("%{query}%");
        let rows: Vec<(i64, i64, i64, String, Option<String>)> = sqlx::query_as(
            "SELECT id, meeting_id, sequence_id, text, rewritten_text
             FROM transcripts WHERE text LIKE ? OR rewritten_text LIKE ?
             ORDER BY created_at DESC LIMIT ?",
        )
        .bind(&pattern)
        .bind(&pattern)
        .bind(limit)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, meeting_id, sequence_id, text, rewritten_text)| TranscriptSearchHit {
                transcript_id: id,
                meeting_id,
                sequence_id,
                text,
                rewritten_text,
            })
            .collect())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TranscriptSearchHit {
    pub transcript_id: i64,
    pub meeting_id: i64,
    pub sequence_id: i64,
    pub text: String,
    pub rewritten_text: Option<String>,
}

fn map_transcript(
    (id, meeting_id, sequence_id, start_ms, end_ms, text, rewritten_text, language, speaker_label, confidence, is_partial, created_at): (
        i64, i64, i64, i32, i32, String, Option<String>,
        Option<String>, Option<String>, Option<f32>, i32, i64,
    ),
) -> Transcript {
    Transcript {
        id, meeting_id, sequence_id, start_ms, end_ms, text, rewritten_text,
        language, speaker_label, confidence,
        is_partial: is_partial != 0,
        created_at: DateTime::from_timestamp_millis(created_at).unwrap_or_else(Utc::now),
    }
}
