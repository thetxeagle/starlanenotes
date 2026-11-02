-- Immutable publication evidence survives deletion/supersession of canonical pages.
-- This journal is distinct from create-only v1 receipts; no disposable source FK.
CREATE TABLE publication_receipts (
    op_id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    draft_id TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    source_path TEXT NOT NULL,
    source_revision TEXT NOT NULL,
    content_path TEXT NOT NULL,
    revision TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    byte_length BIGINT NOT NULL CHECK (byte_length >= 0),
    payload_fingerprint TEXT NOT NULL,
    manifest_json TEXT NOT NULL,
    verified_at TEXT
);
CREATE INDEX publication_receipts_document ON publication_receipts(document_id);
