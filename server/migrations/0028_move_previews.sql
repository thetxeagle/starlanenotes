-- Review records only; no relocation or metadata changes are authorized by this table.
-- Store only binding/fingerprint and versioned summary, never raw content or grants.
-- Avoid source-definition foreign keys so later definition deletion cannot rewrite a review.
CREATE TABLE IF NOT EXISTS move_previews (
    id TEXT PRIMARY KEY NOT NULL,
    actor_id TEXT NOT NULL,
    source_space_id TEXT NOT NULL,
    root_kind TEXT NOT NULL,
    root_id TEXT NOT NULL,
    request_json TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    manifest_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    cancelled_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_move_previews_actor ON move_previews(actor_id, expires_at);
