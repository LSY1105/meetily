-- Generic key/value table for small app-wide user preferences.
-- Single row per key; updated_at bumped on every write.
-- Used by PR-48 to persist the LLM health-check interval set in
-- the diagnostics panel; future PRs can reuse it without adding
-- bespoke repository methods.
CREATE TABLE IF NOT EXISTS app_settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);
