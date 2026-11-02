-- Create-only storage intents committed with canonical document/draft inserts.
-- Identity is immutable in the application; keep receipts after stage recovery.
-- No cascading FK: receipt evidence survives canonical deletion for safe review.
CREATE TABLE storage_journal_receipts (
    op_id TEXT PRIMARY KEY,
    space_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    draft_id TEXT NOT NULL,
    content_path TEXT NOT NULL,
    initial_revision TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    byte_length BIGINT NOT NULL CHECK (byte_length >= 0),
    state TEXT NOT NULL CHECK (state IN ('pending', 'complete')),
    completed_at TEXT
);
CREATE INDEX storage_journal_receipts_document ON storage_journal_receipts(document_id);
