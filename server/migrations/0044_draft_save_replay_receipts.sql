-- Exact incoming draft-save evidence survives publication/deletion/reused IDs.
-- No foreign keys: historical ACK never reapplies current state or grants access.
CREATE TABLE draft_save_replay_receipts (
 provider TEXT NOT NULL, incoming_id TEXT NOT NULL, actor_id TEXT NOT NULL,
 entity_type TEXT NOT NULL, entity_id TEXT NOT NULL, action TEXT NOT NULL,
 payload_fingerprint TEXT NOT NULL, space_id TEXT NOT NULL, document_id TEXT NOT NULL,
 draft_id TEXT NOT NULL, source_digest TEXT NOT NULL, result_digest TEXT NOT NULL,
 created_at TEXT NOT NULL, receipt_json TEXT NOT NULL,
 PRIMARY KEY(provider,incoming_id)
);
