-- Immutable incoming metadata intent/outcome; no grants or cascade.
CREATE TABLE document_metadata_replay_receipts (
 provider TEXT NOT NULL,incoming_id TEXT NOT NULL,actor_id TEXT NOT NULL,
 document_id TEXT NOT NULL,space_id TEXT NOT NULL,action TEXT NOT NULL,
 payload_fingerprint TEXT NOT NULL,receipt_json TEXT NOT NULL,
 PRIMARY KEY(provider,incoming_id)
);
