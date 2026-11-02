-- Immutable incoming deletion evidence; no blob cleanup or recovery authority.
CREATE TABLE asset_delete_replay_receipts (
 provider TEXT NOT NULL, incoming_id TEXT NOT NULL, actor_id TEXT NOT NULL,
 entity_id TEXT NOT NULL, action TEXT NOT NULL, payload_fingerprint TEXT NOT NULL,
 space_id TEXT NOT NULL, document_id TEXT NOT NULL, outcome TEXT NOT NULL,
 receipt_json TEXT NOT NULL, PRIMARY KEY(provider,incoming_id)
);
