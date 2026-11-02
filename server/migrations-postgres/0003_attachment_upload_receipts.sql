-- Immutable direct upload commit decision; filesystem installation remains separately evidenced.
CREATE TABLE attachment_upload_receipts (
    operation_id TEXT PRIMARY KEY NOT NULL,
    origin TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    attempt_id TEXT NOT NULL,
    storage_path TEXT NOT NULL,
    filename TEXT NOT NULL,
    original_name TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    byte_length BIGINT NOT NULL CHECK(byte_length >= 0),
    binary_sha256 TEXT NOT NULL,
    created_at TEXT NOT NULL,
    intent_sha256 TEXT NOT NULL,
    install_proof_sha256 TEXT NOT NULL,
    receipt_json TEXT NOT NULL
);
CREATE INDEX attachment_upload_receipts_document_idx ON attachment_upload_receipts(space_id, document_id);
