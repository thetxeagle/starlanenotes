-- Durable original-operation evidence: intentionally no cascading foreign keys.
CREATE TABLE snapshot_recovery_receipts (
 operation_id TEXT PRIMARY KEY,
 actor_id TEXT NOT NULL,
 space_id TEXT NOT NULL,
 document_id TEXT NOT NULL,
 snapshot_id TEXT NOT NULL,
 request_json TEXT NOT NULL,
 evidence_json TEXT NOT NULL,
 evidence_digest TEXT NOT NULL
);
CREATE INDEX snapshot_recovery_receipts_document_idx ON snapshot_recovery_receipts(document_id);
