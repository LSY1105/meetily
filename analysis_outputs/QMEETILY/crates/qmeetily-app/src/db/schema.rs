//! SQL schema — idempotent CREATE IF NOT EXISTS.
//!
//! Borrowed from meetily's schema, simplified. Adds `decisions` table for
//! structured deal/decision/action extraction.
//!
//! Note: full-text search used to be backed by an FTS5 virtual table
//! (`transcripts_fts`) that v0.1 never wired up — db/transcripts::search
//! uses LIKE instead. We drop the table up front so any user who already
//! ran an earlier build cleans up automatically on next launch.

pub const SCHEMA_SQL: &str = r#"
DROP TABLE IF EXISTS transcripts_fts;

CREATE TABLE IF NOT EXISTS meetings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    language_primary TEXT,
    audio_path TEXT,
    participants_json TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS transcripts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    meeting_id INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    sequence_id INTEGER NOT NULL,
    start_ms INTEGER NOT NULL,
    end_ms INTEGER NOT NULL,
    text TEXT NOT NULL,
    rewritten_text TEXT,
    language TEXT,
    speaker_label TEXT,
    confidence REAL,
    is_partial INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    UNIQUE(meeting_id, sequence_id)
);
CREATE INDEX IF NOT EXISTS idx_transcripts_meeting ON transcripts(meeting_id);
CREATE INDEX IF NOT EXISTS idx_transcripts_created ON transcripts(created_at);

CREATE TABLE IF NOT EXISTS decisions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    meeting_id INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    transcript_id INTEGER REFERENCES transcripts(id) ON DELETE SET NULL,
    type TEXT NOT NULL CHECK(type IN ('deal','decision','action','question','risk')),
    text TEXT NOT NULL,
    owner TEXT,
    due_date TEXT,
    confidence REAL,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_decisions_meeting ON decisions(meeting_id);

CREATE TABLE IF NOT EXISTS hotwords (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    term TEXT NOT NULL UNIQUE,
    case_sensitive INTEGER NOT NULL DEFAULT 0,
    hits INTEGER NOT NULL DEFAULT 0,
    last_hit_at INTEGER,
    is_active INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS summaries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    meeting_id INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    summary_markdown TEXT NOT NULL,
    language TEXT,
    model TEXT,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_summaries_meeting ON summaries(meeting_id);
"#;
