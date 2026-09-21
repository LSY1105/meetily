use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::Db;
use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecisionType {
    Deal,
    Decision,
    Action,
    Question,
    Risk,
}

impl DecisionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DecisionType::Deal => "deal",
            DecisionType::Decision => "decision",
            DecisionType::Action => "action",
            DecisionType::Question => "question",
            DecisionType::Risk => "risk",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: i64,
    pub meeting_id: i64,
    pub transcript_id: Option<i64>,
    #[serde(rename = "type")]
    pub decision_type: String,
    pub text: String,
    pub owner: Option<String>,
    pub due_date: Option<String>,
    pub confidence: Option<f32>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewDecision {
    pub meeting_id: i64,
    pub transcript_id: Option<i64>,
    pub decision_type: DecisionType,
    pub text: String,
    pub owner: Option<String>,
    pub due_date: Option<String>,
    pub confidence: Option<f32>,
}

impl Db {
    pub async fn insert_decision(&self, d: &NewDecision) -> Result<i64> {
        let id = sqlx::query_scalar(
            "INSERT INTO decisions (meeting_id, transcript_id, type, text, owner, due_date, confidence, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(d.meeting_id)
        .bind(d.transcript_id)
        .bind(d.decision_type.as_str())
        .bind(&d.text)
        .bind(&d.owner)
        .bind(&d.due_date)
        .bind(d.confidence)
        .bind(Utc::now().timestamp_millis())
        .fetch_one(self.pool())
        .await?;
        Ok(id)
    }

    pub async fn list_meeting_decisions(&self, meeting_id: i64) -> Result<Vec<Decision>> {
        let rows: Vec<(
            i64, i64, Option<i64>, String, String, Option<String>,
            Option<String>, Option<f32>, i64,
        )> = sqlx::query_as(
            "SELECT id, meeting_id, transcript_id, type, text, owner, due_date, confidence, created_at
             FROM decisions WHERE meeting_id = ? ORDER BY created_at",
        )
        .bind(meeting_id)
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, meeting_id, transcript_id, decision_type, text, owner, due_date, confidence, created_at)| {
                Decision {
                    id, meeting_id, transcript_id, decision_type, text, owner, due_date, confidence,
                    created_at: DateTime::from_timestamp_millis(created_at).unwrap_or_else(Utc::now),
                }
            })
            .collect())
    }
}
