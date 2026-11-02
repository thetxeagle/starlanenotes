-- Exact immutable replay creation evidence; survives later page/draft lifecycle.
CREATE TABLE document_create_replay_receipts (
 provider TEXT NOT NULL, incoming_id TEXT NOT NULL, actor_id TEXT NOT NULL,
 entity_id TEXT NOT NULL, action TEXT NOT NULL, payload_fingerprint TEXT NOT NULL,
 space_id TEXT NOT NULL, storage_op_id TEXT NOT NULL, receipt_json TEXT NOT NULL,
 PRIMARY KEY(provider,incoming_id)
);
