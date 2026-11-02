-- Responsibility is metadata, never membership, private participation or authority.
ALTER TABLE documents ADD COLUMN maintenance_user_id TEXT REFERENCES users(id) ON DELETE SET NULL;
ALTER TABLE documents ADD COLUMN maintenance_group_id TEXT REFERENCES groups(id) ON DELETE SET NULL
 CHECK (maintenance_user_id IS NULL OR maintenance_group_id IS NULL);
CREATE INDEX documents_maintenance_user ON documents(maintenance_user_id);
CREATE INDEX documents_maintenance_group ON documents(maintenance_group_id);
-- Immutable incoming evidence survives the page and responsibility target.
CREATE TABLE document_maintenance_receipts (
 provider TEXT NOT NULL,incoming_id TEXT NOT NULL,actor_id TEXT NOT NULL,
 document_id TEXT NOT NULL,space_id TEXT NOT NULL,payload_fingerprint TEXT NOT NULL,
 receipt_json TEXT NOT NULL,PRIMARY KEY(provider,incoming_id)
);
