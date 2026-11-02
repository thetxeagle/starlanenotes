-- Immutable asset-create incoming proof. No cleanup/recovery journal authority.
CREATE TABLE asset_replay_receipts (
 provider TEXT NOT NULL, incoming_id TEXT NOT NULL, actor_id TEXT NOT NULL,
 entity_type TEXT NOT NULL, entity_id TEXT NOT NULL, action TEXT NOT NULL,
 payload_fingerprint TEXT NOT NULL, space_id TEXT NOT NULL, document_id TEXT NOT NULL,
 storage_path TEXT NOT NULL, filename TEXT NOT NULL, original_name TEXT NOT NULL,
 mime_type TEXT NOT NULL, byte_length BIGINT NOT NULL, binary_sha256 TEXT NOT NULL,
 source_kind TEXT NOT NULL, source_id TEXT, source_revision TEXT, body_sha256 TEXT,
 receipt_json TEXT NOT NULL, created_at TEXT NOT NULL, PRIMARY KEY(provider,incoming_id)
);
