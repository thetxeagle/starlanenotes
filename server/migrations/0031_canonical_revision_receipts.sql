-- Non-draft canonical rewrite evidence survives supersession/deletion.
-- No synthetic draft identity and no canonical/source FK.
CREATE TABLE canonical_revision_receipts (
 op_id TEXT PRIMARY KEY, space_id TEXT NOT NULL, document_id TEXT NOT NULL,
 actor_id TEXT NOT NULL, reason TEXT NOT NULL,
 provider TEXT NOT NULL, incoming_id TEXT NOT NULL, entity_type TEXT NOT NULL,
 entity_id TEXT NOT NULL, action TEXT NOT NULL, payload_fingerprint TEXT NOT NULL,
 source_path TEXT NOT NULL, source_revision TEXT NOT NULL, published_stamp TEXT NOT NULL,
 source_sha256 TEXT, content_path TEXT NOT NULL, revision TEXT NOT NULL,
 sha256 TEXT NOT NULL, byte_length BIGINT NOT NULL CHECK(byte_length>=0),
 manifest_json TEXT NOT NULL, verified_at TEXT,
 UNIQUE(provider,incoming_id)
);
CREATE INDEX canonical_revision_receipts_document ON canonical_revision_receipts(document_id);
