-- Same-transaction incoming hard-deletion evidence; survives page recreation.
-- Files are retained; this receipt never grants read/recovery or erasure authority.
CREATE TABLE document_delete_replay_receipts (
 provider TEXT NOT NULL, incoming_id TEXT NOT NULL, actor_id TEXT NOT NULL,
 entity_id TEXT NOT NULL, action TEXT NOT NULL, payload_fingerprint TEXT NOT NULL,
 space_id TEXT NOT NULL, source_fingerprint TEXT NOT NULL, created_at TEXT NOT NULL,
 receipt_json TEXT NOT NULL, PRIMARY KEY(provider,incoming_id)
);
